//! The event schema for `trajectory.jsonl` (PLAN.md §4.5).
//!
//! One JSON object per line, append-only. The invariant borrowed from DeepSeek
//! Harness is that **anything that reached a model must be reconstructable from
//! this stream** — which is also the only thing that makes a gate verdict
//! reproducible rather than an opinion with a timestamp.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Payload {
    /// A run begins. Carries enough to know what was being attempted.
    RunStart {
        spec: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        task: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        driver: Option<String>,
        keel_version: String,
        store_hash: String,
    },
    /// Context keel put in front of the agent. Without this the stream cannot
    /// answer "did the lesson actually help?".
    Inject {
        source: String,
        tokens: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        bytes: Option<usize>,
    },
    DriverCall {
        driver: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        task: Option<String>,
        prompt_tokens: usize,
    },
    DriverResult {
        driver: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        files_changed: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// An oracle was executed. This is what G2's oracle-coverage reads.
    Oracle {
        criterion: String,
        oracle: String,
        verdict: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
    /// A gate reached a verdict.
    Gate {
        gate: String,
        verdict: String,
        /// Path of the gate result file, so the verdict is reproducible.
        result: String,
    },
    /// A person decided something.
    Human {
        stage: String,
        decision: String,
        by: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// A command keel ran on the agent's behalf, or to gather evidence.
    Command {
        cmd: String,
        exit_code: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        evidence: Option<String>,
    },
    RunEnd {
        verdict: String,
        /// u64, not u128: serde_json refuses u128, and a run measured in
        /// milliseconds has 584 million years of headroom either way.
        duration_ms: u64,
    },
}

impl Payload {
    pub fn kind(&self) -> &'static str {
        match self {
            Payload::RunStart { .. } => "run_start",
            Payload::Inject { .. } => "inject",
            Payload::DriverCall { .. } => "driver_call",
            Payload::DriverResult { .. } => "driver_result",
            Payload::Oracle { .. } => "oracle",
            Payload::Gate { .. } => "gate",
            Payload::Human { .. } => "human",
            Payload::Command { .. } => "command",
            Payload::RunEnd { .. } => "run_end",
        }
    }

    /// Tokens this event put in front of a model, if any.
    pub fn tokens(&self) -> usize {
        match self {
            Payload::Inject { tokens, .. } => *tokens,
            Payload::DriverCall { prompt_tokens, .. } => *prompt_tokens,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    /// RFC 3339 wall-clock time.
    pub t: String,
    /// 1-based, gapless within a run.
    pub seq: u64,
    #[serde(flatten)]
    pub payload: Payload,
}

impl Event {
    pub fn one_line(&self) -> Result<String, serde_json::Error> {
        // `to_string` never emits a newline, which is what makes the file
        // line-delimited rather than merely JSON-ish.
        serde_json::to_string(self)
    }

    /// A compact human rendering for `keel replay`.
    pub fn summary(&self) -> String {
        let detail = match &self.payload {
            Payload::RunStart { spec, task, driver, .. } => format!(
                "spec={spec}{}{}",
                task.as_ref().map(|t| format!(" task={t}")).unwrap_or_default(),
                driver.as_ref().map(|d| format!(" driver={d}")).unwrap_or_default()
            ),
            Payload::Inject { source, tokens, .. } => format!("{source} ({tokens} tokens)"),
            Payload::DriverCall { driver, prompt_tokens, .. } => {
                format!("{driver} ({prompt_tokens} prompt tokens)")
            }
            Payload::DriverResult { driver, status, detail, .. } => {
                format!("{driver} {status}{}", detail.as_ref().map(|d| format!(" — {d}")).unwrap_or_default())
            }
            Payload::Oracle { criterion, verdict, oracle, .. } => {
                format!("{criterion} {verdict} — {oracle}")
            }
            Payload::Gate { gate, verdict, .. } => format!("{gate} {verdict}"),
            Payload::Human { stage, decision, by, .. } => format!("{stage} {decision} by {by}"),
            Payload::Command { cmd, exit_code, .. } => format!("`{cmd}` exit {exit_code}"),
            Payload::RunEnd { verdict, duration_ms } => format!("{verdict} in {duration_ms}ms"),
        };
        format!("{:>5}  {:<14} {}", self.seq, self.payload.kind(), detail)
    }
}

/// A deliberately crude token estimate.
///
/// keel never sees the model's tokeniser, and a wrong-but-consistent number is
/// enough for the only two questions the trajectory needs to answer: is the
/// budget being spent, and on what. Calling this exact would be the lie.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> Event {
        Event {
            t: "2026-08-21T09:14:02Z".into(),
            seq: 412,
            payload: Payload::Inject {
                source: "store/lessons/L-0004.md".into(),
                tokens: 86,
                bytes: Some(344),
            },
        }
    }

    #[test]
    fn serialises_flat_with_a_kind_tag() {
        let json = event().one_line().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["kind"], "inject");
        assert_eq!(v["seq"], 412);
        assert_eq!(v["source"], "store/lessons/L-0004.md");
        assert!(!json.contains('\n'), "an event must be one line");
    }

    #[test]
    fn round_trips() {
        let e = event();
        let back: Event = serde_json::from_str(&e.one_line().unwrap()).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn every_payload_kind_round_trips() {
        let payloads = vec![
            Payload::RunStart { spec: "s".into(), task: None, driver: None, keel_version: "0".into(), store_hash: "h".into() },
            Payload::Inject { source: "a".into(), tokens: 1, bytes: None },
            Payload::DriverCall { driver: "d".into(), task: None, prompt_tokens: 2 },
            Payload::DriverResult { driver: "d".into(), status: "ok".into(), files_changed: Some(1), detail: None },
            Payload::Oracle { criterion: "AC-1".into(), oracle: "cmd".into(), verdict: "pass".into(), exit_code: Some(0) },
            Payload::Gate { gate: "G2".into(), verdict: "fail".into(), result: "gates/G2.json".into() },
            Payload::Human { stage: "spec".into(), decision: "approved".into(), by: "me".into(), note: None },
            Payload::Command { cmd: "cargo test".into(), exit_code: 0, evidence: None },
            Payload::RunEnd { verdict: "pass".into(), duration_ms: 42 },
        ];
        for p in payloads {
            let kind = p.kind();
            let e = Event { t: "t".into(), seq: 1, payload: p };
            let line = e.one_line().unwrap();
            let back: Event = serde_json::from_str(&line).unwrap();
            assert_eq!(back, e, "{kind} did not round trip");
            assert!(!back.summary().is_empty());
        }
    }

    #[test]
    fn token_accounting_only_counts_what_reached_the_model() {
        assert_eq!(Payload::Inject { source: "a".into(), tokens: 86, bytes: None }.tokens(), 86);
        assert_eq!(Payload::Gate { gate: "G2".into(), verdict: "pass".into(), result: "x".into() }.tokens(), 0);
    }

    #[test]
    fn token_estimate_is_monotonic_in_length() {
        assert!(estimate_tokens("a longer piece of text here") > estimate_tokens("short"));
        assert_eq!(estimate_tokens(""), 0);
    }
}
