//! The append-only trajectory stream (PLAN.md P5, §4.5).
//!
//! One `Trajectory` owns the single open handle for a run. Every writer in keel
//! takes `&mut Trajectory` rather than reaching for the path, so there is
//! exactly one place that can write the stream and exactly one place to enforce
//! the sequence invariant.

pub mod event;

use anyhow::{Context, Result, bail};
pub use event::{Event, Payload};
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct Trajectory {
    path: PathBuf,
    file: File,
    next_seq: u64,
}

impl Trajectory {
    /// Open a run's trajectory for appending, continuing its sequence.
    ///
    /// Opening an existing stream reads its last sequence number rather than
    /// restarting at 1: a resumed run that renumbers from the start silently
    /// destroys the ordering the whole design rests on.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let next_seq = match read(path) {
            Ok(events) => events.last().map(|e| e.seq + 1).unwrap_or(1),
            Err(_) if !path.exists() => 1,
            Err(e) => return Err(e),
        };
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening {} for append", path.display()))?;
        Ok(Self { path: path.to_path_buf(), file, next_seq })
    }

    /// Append one event, returning it as written.
    pub fn append(&mut self, payload: Payload) -> Result<Event> {
        let event = Event {
            t: chrono::Local::now().to_rfc3339(),
            seq: self.next_seq,
            payload,
        };
        let line = event.one_line()?;
        debug_assert!(!line.contains('\n'));
        // One write call per event: a partial line is an unparseable stream, and
        // the reader has no way to tell a truncated event from a corrupt one.
        self.file
            .write_all(format!("{line}\n").as_bytes())
            .with_context(|| format!("appending to {}", self.path.display()))?;
        self.file.flush()?;
        self.next_seq += 1;
        Ok(event)
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }
}

/// Something in the stream that a strict read refuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "anomaly", rename_all = "snake_case")]
pub enum Anomaly {
    /// A line that is not a trajectory event and is not the in-progress tail.
    Unparseable { line: usize, why: String },
    /// A sequence number that does not follow the one before it.
    Gap { line: usize, expected: u64, found: u64 },
    /// The final line is incomplete and the file has no closing newline — an
    /// append caught mid-write, which is expected while a run is in flight.
    TrailingPartial { line: usize },
}

impl Anomaly {
    fn message(&self, path: &Path) -> String {
        let p = path.display();
        match self {
            Self::Unparseable { line, why } => {
                format!("{p}:{line} is not a valid trajectory event: {why}")
            }
            Self::Gap { line, expected, found } => format!(
                "{p}:{line} sequence is {found} where {expected} was expected \
                 — the stream has a gap or a duplicate"
            ),
            Self::TrailingPartial { line } => {
                format!("{p}:{line} is a partial record — the stream was cut mid-append")
            }
        }
    }
}

/// Every event a trajectory yields, plus whatever could not be read.
pub struct Scan {
    pub events: Vec<Event>,
    pub anomalies: Vec<Anomaly>,
}

/// Read a trajectory, tolerating damage and reporting exactly what it was.
///
/// This is for readers that must render a stream they do not control — one
/// being appended to right now, or one that was truncated by a crash. It never
/// invents an event to fill a gap and never drops a line without recording
/// that it did, so a caller can always say what it could not see. Callers that
/// need the strict guarantee want [`read`] instead.
pub fn scan(path: &Path) -> Result<Scan> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    // A file not ending in a newline has an append in flight, so its last line
    // is expected to be incomplete rather than corrupt.
    let closed = raw.is_empty() || raw.ends_with('\n');
    let lines: Vec<&str> = raw.lines().collect();
    let last = lines.len().saturating_sub(1);

    let mut events: Vec<Event> = Vec::new();
    let mut anomalies = Vec::new();
    let mut prev_seq: Option<u64> = None;

    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Event>(line) {
            Ok(event) => {
                let expected = prev_seq.map(|p| p + 1).unwrap_or(1);
                if event.seq != expected {
                    anomalies.push(Anomaly::Gap { line: i + 1, expected, found: event.seq });
                }
                prev_seq = Some(event.seq);
                events.push(event);
            }
            Err(_) if i == last && !closed => {
                anomalies.push(Anomaly::TrailingPartial { line: i + 1 });
            }
            Err(why) => {
                anomalies.push(Anomaly::Unparseable { line: i + 1, why: why.to_string() });
            }
        }
    }

    Ok(Scan { events, anomalies })
}

/// Read every event in a trajectory, in sequence order.
///
/// A malformed line is an error naming the file and line number, never a
/// silently skipped record: a stream you cannot fully parse cannot support the
/// claim that a verdict is reproducible from it.
///
/// Implemented as [`scan`] plus "refuse if anything was wrong", so the strict
/// guarantee and the lenient one are the same parser and cannot drift apart.
pub fn read(path: &Path) -> Result<Vec<Event>> {
    let scanned = scan(path)?;
    match scanned.anomalies.first() {
        Some(a) => bail!("{}", a.message(path)),
        None => Ok(scanned.events),
    }
}

/// Total tokens this run put in front of a model.
pub fn token_total(events: &[Event]) -> usize {
    events.iter().map(|e| e.payload.tokens()).sum()
}

/// Every gate verdict in the stream, in order.
pub fn gate_verdicts(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            Payload::Gate { gate, verdict, .. } => Some((gate.clone(), verdict.clone())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tmp() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "keel-traj-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("trajectory.jsonl")
    }

    fn inject(n: usize) -> Payload {
        Payload::Inject { source: format!("store/{n}.md"), tokens: n, bytes: None }
    }

    #[test]
    fn appends_one_line_per_event() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        for n in 1..=3 {
            t.append(inject(n)).unwrap();
        }
        let raw = std::fs::read_to_string(&p).unwrap();
        assert_eq!(raw.lines().count(), 3);
        for line in raw.lines() {
            serde_json::from_str::<Event>(line).expect("each line parses alone");
        }
    }

    #[test]
    fn sequence_starts_at_one_and_is_gapless() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        for n in 1..=5 {
            assert_eq!(t.append(inject(n)).unwrap().seq, n as u64);
        }
        let seqs: Vec<u64> = read(&p).unwrap().iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn reopening_continues_the_sequence_and_keeps_existing_lines() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        t.append(inject(1)).unwrap();
        t.append(inject(2)).unwrap();
        drop(t);

        let mut t = Trajectory::open(&p).unwrap();
        assert_eq!(t.next_seq(), 3, "a reopened stream restarted its numbering");
        t.append(inject(3)).unwrap();

        let events = read(&p).unwrap();
        assert_eq!(events.len(), 3, "reopening truncated the stream");
        assert_eq!(events.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn a_corrupt_line_names_the_file_and_line() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        t.append(inject(1)).unwrap();
        drop(t);
        let mut raw = std::fs::read_to_string(&p).unwrap();
        raw.push_str("{not json}\n");
        std::fs::write(&p, raw).unwrap();

        let err = format!("{:#}", read(&p).unwrap_err());
        assert!(err.contains(":2"), "line number missing from `{err}`");
        assert!(err.contains("trajectory.jsonl"), "file name missing from `{err}`");
    }

    #[test]
    fn a_gap_in_the_sequence_is_an_error() {
        let p = tmp();
        let e = Event { t: "t".into(), seq: 7, payload: inject(1) };
        std::fs::write(&p, format!("{}\n", e.one_line().unwrap())).unwrap();
        let err = format!("{:#}", read(&p).unwrap_err());
        assert!(err.contains("gap or a duplicate"), "{err}");
    }

    #[test]
    fn blank_lines_are_tolerated() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        t.append(inject(1)).unwrap();
        drop(t);
        let raw = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, format!("{raw}\n\n")).unwrap();
        assert_eq!(read(&p).unwrap().len(), 1);
    }

    // --- scan: the lenient reader ----------------------------------------

    #[test]
    fn scan_keeps_the_events_around_a_gap_and_names_it() {
        let p = tmp();
        let a = Event { t: "t".into(), seq: 1, payload: inject(1) };
        let b = Event { t: "t".into(), seq: 5, payload: inject(2) };
        let c = Event { t: "t".into(), seq: 6, payload: inject(3) };
        std::fs::write(
            &p,
            format!(
                "{}\n{}\n{}\n",
                a.one_line().unwrap(),
                b.one_line().unwrap(),
                c.one_line().unwrap()
            ),
        )
        .unwrap();

        let s = scan(&p).unwrap();
        assert_eq!(s.events.len(), 3, "a gap discarded events after it");
        assert!(!s.anomalies.is_empty());
        assert_eq!(
            s.anomalies,
            vec![Anomaly::Gap { line: 2, expected: 2, found: 5 }],
            "one anomaly per discontinuity, not one per following event"
        );
    }

    #[test]
    fn an_unterminated_last_line_is_a_partial_append_not_corruption() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        t.append(inject(1)).unwrap();
        drop(t);
        let raw = std::fs::read_to_string(&p).unwrap();
        // No trailing newline: an append caught mid-write.
        std::fs::write(&p, format!("{raw}{{\"t\":\"2026")).unwrap();

        let s = scan(&p).unwrap();
        assert_eq!(s.events.len(), 1);
        assert_eq!(s.anomalies, vec![Anomaly::TrailingPartial { line: 2 }]);
    }

    #[test]
    fn a_broken_line_with_a_newline_after_it_is_corruption_not_a_partial() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        t.append(inject(1)).unwrap();
        drop(t);
        let raw = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, format!("{raw}{{not json}}\n")).unwrap();

        let s = scan(&p).unwrap();
        assert_eq!(s.events.len(), 1);
        assert!(
            matches!(s.anomalies.as_slice(), [Anomaly::Unparseable { line: 2, .. }]),
            "{:?}",
            s.anomalies
        );
    }

    #[test]
    fn scan_and_read_agree_on_a_healthy_stream() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        for n in 1..=4 {
            t.append(inject(n)).unwrap();
        }
        drop(t);
        let s = scan(&p).unwrap();
        assert!(s.anomalies.is_empty());
        assert_eq!(s.events.len(), read(&p).unwrap().len());
    }

    #[test]
    fn token_and_gate_summaries_read_the_stream() {
        let p = tmp();
        let mut t = Trajectory::open(&p).unwrap();
        t.append(inject(10)).unwrap();
        t.append(inject(32)).unwrap();
        t.append(Payload::Gate {
            gate: "G2".into(), verdict: "fail".into(), result: "gates/G2.json".into(),
        }).unwrap();
        let events = read(&p).unwrap();
        assert_eq!(token_total(&events), 42);
        assert_eq!(gate_verdicts(&events), vec![("G2".to_string(), "fail".to_string())]);
    }
}
