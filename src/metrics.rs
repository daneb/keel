//! The harness measured across every run on disk — one snapshot, no time axis.
//!
//! `compute()` is the pure aggregation `keel metrics` prints and
//! `report::insights` builds on. It lives outside `cmd/` because it is no
//! longer CLI-only: a computation two different surfaces trust should exist
//! once, not be re-derived at the second call site.
//!
//! PLAN.md §6 names the metric that matters most and it is not a quality
//! number: *"a gate that never fails in 20 runs is deleted or tightened."*
//! Gate theatre is invisible from inside a single run and obvious across
//! twenty, which is the whole reason this module exists.

use crate::failure;
use crate::gate::Verdict;
use crate::lesson;
use crate::paths::Paths;
use crate::run::Run;
use crate::trajectory::{self, UsageTotals};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, Serialize)]
pub struct CheckStats {
    pub gate: String,
    pub check: String,
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    pub blocked: usize,
}

impl CheckStats {
    /// A check that has never failed across enough runs to have had the chance.
    pub fn is_theatre(&self, threshold: usize) -> bool {
        self.runs >= threshold && self.failed == 0 && self.blocked == 0
    }

    /// Share of runs this check passed, `0.0` when it has never run.
    pub fn pass_rate(&self) -> f64 {
        if self.runs == 0 { 0.0 } else { self.passed as f64 / self.runs as f64 }
    }
}

/// One merge approval and how long the reviewer had the brief.
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalSpeed {
    pub spec: String,
    pub seconds: i64,
    pub reviewable_lines: usize,
    pub fast: bool,
}

/// Read from the file, not `Config`: `review` is also the `[[review]]` array.
fn min_seconds_per_100_lines(paths: &Paths) -> u64 {
    std::fs::read_to_string(paths.config())
        .ok()
        .and_then(|raw| raw.parse::<toml::Table>().ok())
        .and_then(|t| t.get("review")?.as_table()?.get("min_seconds_per_100_lines")?.as_integer())
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(30)
}

/// Merge approvals made against a brief; one with no brief has no start time.
fn approval_speeds(paths: &Paths) -> Result<Vec<ApprovalSpeed>> {
    let min = min_seconds_per_100_lines(paths) as f64;
    let mut out = Vec::new();
    for slug in crate::spec::list(paths)? {
        for a in crate::approval::history(paths, &slug)?.into_iter().filter(|a| a.stage == "merge") {
            let (Some(from), Some(lines)) = (&a.brief_at, a.reviewable_lines) else { continue };
            let parse = chrono::DateTime::parse_from_rfc3339;
            let (Ok(from), Ok(to)) = (parse(from), parse(&a.at)) else { continue };
            let seconds = (to - from).num_seconds();
            out.push(ApprovalSpeed {
                spec: slug.clone(),
                seconds,
                reviewable_lines: lines,
                fast: (seconds as f64) < min * lines as f64 / 100.0,
            });
        }
    }
    Ok(out)
}

pub fn print_approval_speed() -> Result<()> {
    let paths = Paths::require_init()?;
    println!("\nmerge approvals (brief to approval)");
    for a in approval_speeds(&paths)? {
        let flag = if a.fast { "  fast" } else { "" };
        println!("  {:<24} {:>5}s for {:>4} line(s){flag}", a.spec, a.seconds, a.reviewable_lines);
    }
    println!(
        "  fast = under {}s per 100 reviewable lines. This measures elapsed time, not attention.",
        min_seconds_per_100_lines(&paths)
    );
    Ok(())
}

#[derive(Debug, Default, Serialize)]
pub struct Metrics {
    pub merge_approvals: Vec<ApprovalSpeed>,
    pub runs: usize,
    pub gate_verdicts: BTreeMap<String, BTreeMap<String, usize>>,
    pub checks: Vec<CheckStats>,
    pub failure_classes: Vec<(String, usize)>,
    pub attribution: Vec<(String, usize)>,
    pub unattributable_rate: f64,
    /// Share of agentic failures a gate could plausibly have caught — computed
    /// by `failure::distribution` but not surfaced by `keel metrics`'s own text
    /// report; `report::insights` is the first consumer that shows it.
    pub harness_fixable_rate: f64,
    /// Injected: keel's own estimate of what it put in front of a model.
    pub tokens_total: usize,
    pub tokens_per_run: f64,
    /// Spent: what drivers and reviewers reported. Never added to the above.
    pub spent: UsageTotals,
    pub human_decisions: usize,
    /// Minutes between a run starting and each human decision on it — the
    /// closest honest proxy keel has for what a person's involvement cost.
    pub human_minutes_total: f64,
    pub human_minutes_per_run: f64,
    pub runs_awaiting_a_human: usize,
    pub lessons_in_force: usize,
    pub lessons_enforced: usize,
    pub lesson_fires: usize,
    /// Checks that have never failed in `theatre_threshold` runs.
    pub never_failed: Vec<String>,
    pub theatre_threshold: usize,
}

/// Aggregate every run on disk. Pure: reads, never writes, never prints.
pub fn compute(paths: &Paths, threshold: usize) -> Result<Metrics> {
    let ids = crate::run::list(paths)?;

    let mut m = Metrics { theatre_threshold: threshold, ..Default::default() };
    let mut by_check: BTreeMap<(String, String), CheckStats> = BTreeMap::new();
    let mut episodes = Vec::new();

    for id in &ids {
        let Ok(run) = Run::load(paths, id) else { continue };
        m.runs += 1;

        for g in run.gate_results()? {
            *m.gate_verdicts
                .entry(g.gate.clone())
                .or_default()
                .entry(g.verdict.glyph().to_lowercase())
                .or_default() += 1;

            for c in &g.checks {
                let e = by_check
                    .entry((g.gate.clone(), c.id.clone()))
                    .or_insert_with(|| CheckStats {
                        gate: g.gate.clone(),
                        check: c.id.clone(),
                        ..Default::default()
                    });
                e.runs += 1;
                match c.verdict {
                    Verdict::Pass => e.passed += 1,
                    Verdict::Fail => e.failed += 1,
                    Verdict::Blocked => e.blocked += 1,
                }
            }
        }

        let events = trajectory::read(&run.trajectory_path()).unwrap_or_default();
        m.tokens_total += trajectory::token_total(&events);
        m.spent.add(&trajectory::usage_total(&events));
        let humans: Vec<&crate::trajectory::Event> =
            events.iter().filter(|e| e.payload.kind() == "human").collect();
        m.human_decisions += humans.len();

        // Wall-clock from the run starting to a person deciding. It is a proxy
        // and a generous one — a decision made the next morning counts the night
        // — so it is reported as elapsed time, never as effort.
        if let (Some(first), Some(last)) = (events.first(), humans.last())
            && let (Ok(start), Ok(end)) = (
                chrono::DateTime::parse_from_rfc3339(&first.t),
                chrono::DateTime::parse_from_rfc3339(&last.t),
            )
        {
            let minutes = (end - start).num_seconds() as f64 / 60.0;
            if minutes >= 0.0 {
                m.human_minutes_total += minutes;
            }
        }
        // A finished run whose gates asked for a person and never got one.
        if humans.is_empty()
            && run.meta.finished_at.is_some()
            && events.iter().any(|e| matches!(&e.payload,
                crate::trajectory::Payload::Gate { gate, verdict, .. }
                    if gate == "G3" && verdict != "pass"))
        {
            m.runs_awaiting_a_human += 1;
        }
        episodes.extend(failure::extract(paths, &run)?);
    }

    m.merge_approvals = approval_speeds(paths)?;
    m.tokens_per_run = if m.runs == 0 { 0.0 } else { m.tokens_total as f64 / m.runs as f64 };
    m.human_minutes_per_run = if m.runs == 0 { 0.0 } else { m.human_minutes_total / m.runs as f64 };

    let d = failure::distribution(&episodes);
    m.failure_classes = d.by_class;
    m.attribution = d.by_attribution;
    m.unattributable_rate = d.unattributable_rate;
    m.harness_fixable_rate = d.harness_fixable_rate;

    let lessons = lesson::list(paths)?;
    m.lessons_in_force = lessons.len();
    m.lessons_enforced = lessons.iter().filter(|l| l.oracle().is_some()).count();
    let ledger = lesson::usage::Ledger::load(paths)?;
    m.lesson_fires = lessons.iter().filter(|l| ledger.last_used(&l.front.id).is_some()).count();

    m.never_failed = by_check
        .values()
        .filter(|c| c.is_theatre(threshold))
        .map(|c| format!("{}/{}", c.gate, c.check))
        .collect();
    m.checks = by_check.into_values().collect();

    Ok(m)
}
