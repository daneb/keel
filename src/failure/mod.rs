//! Failure episodes: extraction from a run, then classification.
//!
//! An episode is a failure signal plus the first thing that happened next
//! (PLAN.md Phase 3). Both halves matter: the signal says what broke, and the
//! recovery says what the harness or the human did about it — which is the only
//! evidence available for telling an agentic failure from a flaky environment.

pub mod taxonomy;

use crate::gate::Verdict;
use crate::paths::Paths;
use crate::run::Run;
use crate::trajectory::{Event, Payload};
use anyhow::Result;
use serde::{Deserialize, Serialize};
pub use taxonomy::{Attribution, Class};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum Signal {
    /// A named check inside a gate did not pass.
    GateCheck {
        gate: String,
        check: String,
        verdict: Verdict,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actual: Option<String>,
    },
    /// A criterion's oracle did not pass.
    Oracle { criterion: String, oracle: String, verdict: String },
    /// A command keel ran exited non-zero.
    Command { cmd: String, exit_code: i32 },
    /// The driver reported failure or could not run.
    Driver { status: String, #[serde(default)] detail: Option<String> },
}

impl Signal {
    pub fn key(&self) -> String {
        match self {
            Signal::GateCheck { gate, check, .. } => format!("{gate}/{check}"),
            Signal::Oracle { criterion, .. } => format!("oracle/{criterion}"),
            Signal::Command { cmd, .. } => format!("command/{}", first_word(cmd)),
            Signal::Driver { status, .. } => format!("driver/{status}"),
        }
    }

    /// The check or command name alone, for grouping evidence.
    pub fn key_ref(&self) -> &str {
        match self {
            Signal::GateCheck { check, .. } => check,
            Signal::Oracle { criterion, .. } => criterion,
            Signal::Command { cmd, .. } => first_word(cmd),
            Signal::Driver { status, .. } => status,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Signal::GateCheck { gate, check, expected, actual, .. } => match (expected, actual) {
                (Some(e), Some(a)) => format!("{gate} {check}: expected {e}, got {a}"),
                _ => format!("{gate} {check} did not pass"),
            },
            Signal::Oracle { criterion, oracle, verdict } => {
                format!("{criterion} oracle {verdict}: {oracle}")
            }
            Signal::Command { cmd, exit_code } => format!("`{cmd}` exited {exit_code}"),
            Signal::Driver { status, detail } => {
                format!("driver {status}{}", detail.as_ref().map(|d| format!(": {d}")).unwrap_or_default())
            }
        }
    }
}

/// What happened immediately after the signal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recovery {
    pub kind: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub run: String,
    pub spec: String,
    /// Sequence number of the signal in the trajectory, when it came from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    pub signal: Signal,
    pub attribution: Attribution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<Class>,
    /// Why the classifier decided what it did — so a human can disagree.
    pub rationale: String,
    /// `repo`, `dir:src/api` or `file:src/api/mod.rs`.
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

impl Episode {
    /// A stable identity for "this same thing going wrong again", and the key a
    /// lesson card is filed under.
    ///
    /// Deliberately excludes the run id, the file names and the *check that
    /// caught it*: `blast-radius` and `line-budget` both catch scope creep, and
    /// keying on the check produced two identical lessons for one mistake.
    pub fn signature(&self) -> String {
        format!("{}|{}", self.class.map(|c| c.code()).unwrap_or("-"), self.scope)
    }
}

/// Extract every failure episode from a completed run.
pub fn extract(paths: &Paths, run: &Run) -> Result<Vec<Episode>> {
    let events = crate::trajectory::read(&run.trajectory_path()).unwrap_or_default();
    let gates = run.gate_results()?;
    let mut episodes = Vec::new();

    // --- from gate results ---------------------------------------------------
    // Gate checks carry expected/actual, which is the richest signal available.
    for g in &gates {
        for check in &g.checks {
            if check.verdict == Verdict::Pass {
                continue;
            }
            let signal = Signal::GateCheck {
                gate: g.gate.clone(),
                check: check.id.clone(),
                verdict: check.verdict,
                expected: check.expected.clone(),
                actual: check.actual.clone().or_else(|| check.detail.clone()),
            };
            episodes.push(build(run, signal, &events, None, check.evidence.clone()));
        }
    }

    // --- from the stream -----------------------------------------------------
    for (i, e) in events.iter().enumerate() {
        let signal = match &e.payload {
            Payload::Command { cmd, exit_code, evidence } if *exit_code != 0 => {
                let s = Signal::Command { cmd: cmd.clone(), exit_code: *exit_code };
                episodes.push(build(run, s, &events, Some((e.seq, i)), evidence.clone()));
                continue;
            }
            Payload::Oracle { criterion, oracle, verdict, .. }
                if verdict != "pass" && verdict != "human" =>
            {
                Signal::Oracle {
                    criterion: criterion.clone(),
                    oracle: oracle.clone(),
                    verdict: verdict.clone(),
                }
            }
            Payload::DriverResult { status, detail, .. } if status != "ok" => {
                Signal::Driver { status: status.clone(), detail: detail.clone() }
            }
            _ => continue,
        };
        episodes.push(build(run, signal, &events, Some((e.seq, i)), None));
    }

    // Deterministic order, and stable ids within a run.
    episodes.sort_by_key(|e| e.signal.key());
    for (n, ep) in episodes.iter_mut().enumerate() {
        ep.id = format!("{}#{}", run.meta.id, n + 1);
    }
    let _ = paths;
    Ok(episodes)
}

fn build(
    run: &Run,
    signal: Signal,
    events: &[Event],
    at: Option<(u64, usize)>,
    evidence: Option<String>,
) -> Episode {
    let (attribution, class, rationale) = classify(&signal, events);
    // Scope is only meaningful for a failure attributable to the agent's output.
    // For a PROCESS or UNATTRIBUTABLE episode the paths mentioned are usually
    // remediation advice ("set verify.build in .keel/keel.toml"), not a location.
    let scope = if attribution == Attribution::Agentic {
        infer_scope(&signal)
    } else {
        "repo".to_string()
    };
    Episode {
        id: String::new(),
        run: run.meta.id.clone(),
        spec: run.meta.spec.clone(),
        seq: at.map(|(seq, _)| seq),
        scope,
        recovery: at.and_then(|(_, i)| recovery_after(events, i)),
        attribution,
        class,
        rationale,
        signal,
        evidence,
    }
}

/// The first event after the signal — "the first recovery action".
fn recovery_after(events: &[Event], i: usize) -> Option<Recovery> {
    events.get(i + 1).map(|e| Recovery {
        kind: e.payload.kind().to_string(),
        summary: e.summary().trim().to_string(),
    })
}

// ---------------------------------------------------------------------------
// classification
// ---------------------------------------------------------------------------

/// Attribution first, then class. Returns the reasoning too, because a
/// classifier a human cannot argue with is one they will stop trusting.
pub fn classify(signal: &Signal, events: &[Event]) -> (Attribution, Option<Class>, String) {
    // A human decision anywhere in the run reframes what followed it.
    let human_redirected = events.iter().any(|e| matches!(&e.payload, Payload::Human { decision, .. } if decision == "rejected"));

    match signal {
        // A blocked check means keel could not look. That is never the agent's
        // fault and never a lesson (P6: PROCESS and UNATTRIBUTABLE never promote).
        Signal::GateCheck { verdict: Verdict::Blocked, check, .. } => (
            Attribution::Process,
            None,
            format!("`{check}` could not run; a blocked check is an environment fact, not an agent output"),
        ),

        Signal::Driver { status, detail } if status == "blocked" => (
            Attribution::Process,
            None,
            format!(
                "the driver never ran{}",
                detail.as_ref().map(|d| format!(" ({d})")).unwrap_or_default()
            ),
        ),

        Signal::Driver { status, .. } if status == "failed" => (
            Attribution::Agentic,
            Some(Class::EditRuntime),
            "the driver ran and reported it could not complete the task".to_string(),
        ),

        // A status keel does not recognise. Guessing would be inventing a cause.
        Signal::Driver { status, .. } => (
            Attribution::Unattributable,
            None,
            format!("the driver reported an unrecognised status `{status}`"),
        ),

        Signal::Command { cmd, exit_code } => {
            let (class, why) = classify_command(cmd);
            (
                Attribution::Agentic,
                Some(class),
                format!("`{}` exited {exit_code}; {why}", first_word(cmd)),
            )
        }

        Signal::Oracle { verdict, criterion, .. } if verdict == "blocked" => (
            Attribution::Process,
            None,
            format!("{criterion}'s oracle could not be executed"),
        ),

        Signal::Oracle { criterion, .. } => (
            Attribution::Agentic,
            Some(Class::EditRuntime),
            format!("{criterion}'s oracle ran and did not pass"),
        ),

        Signal::GateCheck { gate, check, .. } => {
            if human_redirected {
                return (
                    Attribution::Human,
                    None,
                    format!("`{check}` failed in a run a person had already rejected"),
                );
            }
            match class_for_check(check) {
                Some(class) => (
                    Attribution::Agentic,
                    Some(class),
                    format!("`{check}` in {gate} is a {} failure", class.locus()),
                ),
                // The honest outcome: keel saw something fail and has no basis
                // for saying whose fault it was. Counted, never learned from.
                None => (
                    Attribution::Unattributable,
                    None,
                    format!("`{check}` failed and no rule maps it to a cause"),
                ),
            }
        }
    }
}

/// Map a gate check id to a taxonomy class.
fn class_for_check(check: &str) -> Option<Class> {
    Some(match check {
        "build" => Class::EditCompile,
        "test" | "oracle-coverage" => Class::EditRuntime,
        "lint" | "conventions" => Class::ConvViolation,
        "blast-radius" | "line-budget" | "reviewable-size" | "task-files-in-scope" => Class::ScopeCreep,
        "test-invalidation" | "test-movement" => Class::TestInvalid,
        "ambiguity" | "ears-conformance" | "no-placeholders" => Class::SpecAmbig,
        "oracle-presence" | "oracle-wellformed" | "criteria-covered" | "criteria-present" => Class::SpecMissing,
        "store-drift" | "blast-radius-current" => Class::CtxStale,
        "baseline-ratchet" => Class::ConvViolation,
        _ => return None,
    })
}

fn classify_command(cmd: &str) -> (Class, &'static str) {
    let c = cmd.to_ascii_lowercase();
    if c.contains("build") || c.contains("compile") || c.contains("tsc") {
        (Class::EditCompile, "a build command failing is a compile-time failure")
    } else if c.contains("clippy") || c.contains("lint") || c.contains("fmt") {
        (Class::ConvViolation, "a lint command failing is a house-rule breach")
    } else {
        (Class::EditRuntime, "a check command failing at run time")
    }
}

/// Where the failure lives, for scoping a lesson.
///
/// Paths named in the failure narrow the scope; without them a lesson applies
/// repo-wide, which is the honest default but also the least useful one.
fn infer_scope(signal: &Signal) -> String {
    let text = match signal {
        Signal::GateCheck { actual, expected, .. } => {
            format!("{} {}", actual.clone().unwrap_or_default(), expected.clone().unwrap_or_default())
        }
        Signal::Oracle { oracle, .. } => oracle.clone(),
        Signal::Command { cmd, .. } => cmd.clone(),
        Signal::Driver { detail, .. } => detail.clone().unwrap_or_default(),
    };
    match common_dir(&paths_in(&text)) {
        Some(d) if !d.is_empty() => format!("dir:{d}"),
        _ => "repo".to_string(),
    }
}

/// Path-looking tokens: contain a `/` and a file extension, or end in `/`.
fn paths_in(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-'))
        .filter(|t| t.contains('/') && t.len() > 3 && !t.starts_with("http"))
        .map(|t| t.trim_end_matches('/').to_string())
        .collect()
}

/// Longest common directory across a set of paths.
fn common_dir(paths: &[String]) -> Option<String> {
    let dirs: Vec<Vec<&str>> = paths
        .iter()
        .map(|p| {
            let d = p.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            d.split('/').filter(|s| !s.is_empty() && *s != "**" && *s != "*").collect()
        })
        .collect();
    let first = dirs.first()?;
    let mut common: Vec<&str> = first.clone();
    for d in dirs.iter().skip(1) {
        let n = common.iter().zip(d.iter()).take_while(|(a, b)| a == b).count();
        common.truncate(n);
    }
    if common.is_empty() { None } else { Some(common.join("/")) }
}

fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or(s)
}

// ---------------------------------------------------------------------------
// reporting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct Distribution {
    pub total: usize,
    pub by_attribution: Vec<(String, usize)>,
    pub by_class: Vec<(String, usize)>,
    /// The number PLAN.md insists stays visible.
    pub unattributable_rate: f64,
    /// Share of agentic failures gates could plausibly fix.
    pub harness_fixable_rate: f64,
}

pub fn distribution(episodes: &[Episode]) -> Distribution {
    let total = episodes.len();
    let by_attribution = Attribution::all()
        .iter()
        .map(|a| (a.code().to_string(), episodes.iter().filter(|e| e.attribution == *a).count()))
        .filter(|(_, n)| *n > 0)
        .collect();
    let by_class = Class::all()
        .iter()
        .map(|c| (c.code().to_string(), episodes.iter().filter(|e| e.class == Some(*c)).count()))
        .filter(|(_, n)| *n > 0)
        .collect();

    let unattributable = episodes.iter().filter(|e| e.attribution == Attribution::Unattributable).count();
    let agentic: Vec<&Episode> = episodes.iter().filter(|e| e.attribution == Attribution::Agentic).collect();
    let fixable = agentic.iter().filter(|e| e.class.is_some_and(|c| c.is_harness_fixable())).count();

    Distribution {
        total,
        by_attribution,
        by_class,
        unattributable_rate: if total == 0 { 0.0 } else { unattributable as f64 / total as f64 },
        harness_fixable_rate: if agentic.is_empty() { 0.0 } else { fixable as f64 / agentic.len() as f64 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate_check(check: &str, verdict: Verdict, actual: &str) -> Signal {
        Signal::GateCheck {
            gate: "G2".into(),
            check: check.into(),
            verdict,
            expected: Some("something".into()),
            actual: Some(actual.into()),
        }
    }

    #[test]
    fn a_blocked_check_is_process_never_agentic() {
        let (a, c, why) = classify(&gate_check("lint", Verdict::Blocked, "no tool"), &[]);
        assert_eq!(a, Attribution::Process);
        assert_eq!(c, None);
        assert!(!a.is_promotable(), "a blocked check must never become a lesson");
        assert!(why.contains("could not run"), "{why}");
    }

    #[test]
    fn a_blocked_driver_is_process() {
        let s = Signal::Driver { status: "blocked".into(), detail: Some("no PATH".into()) };
        let (a, c, _) = classify(&s, &[]);
        assert_eq!(a, Attribution::Process);
        assert_eq!(c, None);
    }

    #[test]
    fn scope_creep_is_recognised_from_the_check_that_caught_it() {
        for check in ["blast-radius", "line-budget", "reviewable-size"] {
            let (a, c, _) = classify(&gate_check(check, Verdict::Fail, "src/main.rs"), &[]);
            assert_eq!(a, Attribution::Agentic);
            assert_eq!(c, Some(Class::ScopeCreep), "{check}");
        }
    }

    #[test]
    fn an_unmapped_check_is_unattributable_not_guessed() {
        let (a, c, why) = classify(&gate_check("some-new-plugin-check", Verdict::Fail, "x"), &[]);
        assert_eq!(a, Attribution::Unattributable, "the classifier invented a cause");
        assert_eq!(c, None);
        assert!(why.contains("no rule maps it"), "{why}");
    }

    #[test]
    fn a_run_a_human_rejected_attributes_to_the_human() {
        let events = vec![Event {
            t: "t".into(),
            seq: 1,
            payload: Payload::Human {
                stage: "merge".into(), decision: "rejected".into(),
                by: "me".into(), note: None,
            },
        }];
        let (a, c, _) = classify(&gate_check("blast-radius", Verdict::Fail, "x"), &events);
        assert_eq!(a, Attribution::Human);
        assert_eq!(c, None);
    }

    #[test]
    fn an_unrecognised_driver_status_is_unattributable() {
        let s = Signal::Driver { status: "weird".into(), detail: None };
        let (a, c, _) = classify(&s, &[]);
        assert_eq!(a, Attribution::Unattributable);
        assert_eq!(c, None);
    }

    #[test]
    fn commands_are_classified_by_what_they_are() {
        let build = Signal::Command { cmd: "cargo build --quiet".into(), exit_code: 101 };
        assert_eq!(classify(&build, &[]).1, Some(Class::EditCompile));
        let lint = Signal::Command { cmd: "cargo clippy --all".into(), exit_code: 1 };
        assert_eq!(classify(&lint, &[]).1, Some(Class::ConvViolation));
        let test = Signal::Command { cmd: "cargo test".into(), exit_code: 101 };
        assert_eq!(classify(&test, &[]).1, Some(Class::EditRuntime));
    }

    #[test]
    fn a_process_episode_is_not_scoped_by_its_remediation_advice() {
        let run = crate::run::RunMeta {
            schema: "keel.run/1".into(), id: "r1".into(), spec: "demo".into(),
            task: None, driver: None, keel_version: "0".into(), store_hash: "h".into(),
            started_at: "t".into(), finished_at: None, verdict: None, base_commit: None,
        };
        let run = crate::run::Run { dir: std::path::PathBuf::from("."), meta: run };
        let signal = Signal::GateCheck {
            gate: "G2".into(), check: "build".into(), verdict: Verdict::Blocked,
            expected: None,
            actual: Some("no `build` command configured — set verify.build in .keel/keel.toml".into()),
        };
        let ep = build(&run, signal, &[], None, None);
        assert_eq!(ep.attribution, Attribution::Process);
        assert_eq!(ep.scope, "repo", "advice text was mistaken for a failure location");
    }

    #[test]
    fn scope_is_narrowed_to_a_common_directory() {
        let s = gate_check("blast-radius", Verdict::Fail, "src/api/mod.rs (+1 -0), src/api/routes.rs (+2 -0)");
        assert_eq!(infer_scope(&s), "dir:src/api");
    }

    #[test]
    fn scope_falls_back_to_repo_when_paths_disagree() {
        let s = gate_check("blast-radius", Verdict::Fail, "src/api/mod.rs, web/app/index.ts");
        assert_eq!(infer_scope(&s), "repo");
    }

    #[test]
    fn scope_is_repo_when_no_paths_are_named() {
        let s = Signal::Command { cmd: "cargo build".into(), exit_code: 1 };
        assert_eq!(infer_scope(&s), "repo");
    }

    #[test]
    fn the_signature_collapses_the_same_mistake_across_runs() {
        let mk = |run: &str| Episode {
            id: format!("{run}#1"), run: run.into(), spec: "s".into(), seq: None,
            signal: gate_check("blast-radius", Verdict::Fail, "src/api/mod.rs"),
            attribution: Attribution::Agentic, class: Some(Class::ScopeCreep),
            rationale: String::new(), scope: "dir:src/api".into(),
            recovery: None, evidence: None,
        };
        assert_eq!(mk("run-a").signature(), mk("run-b").signature());
        // And the check that caught it must not split one mistake in two.
        let mut other_check = mk("run-a");
        other_check.signal = Signal::GateCheck {
            gate: "G2".into(), check: "line-budget".into(), verdict: Verdict::Fail,
            expected: None, actual: None,
        };
        assert_eq!(other_check.signature(), mk("run-a").signature());
    }

    #[test]
    fn the_distribution_keeps_the_unattributable_rate_visible() {
        let mk = |a: Attribution| Episode {
            id: "x".into(), run: "r".into(), spec: "s".into(), seq: None,
            signal: gate_check("x", Verdict::Fail, ""), attribution: a, class: None,
            rationale: String::new(), scope: "repo".into(), recovery: None, evidence: None,
        };
        let d = distribution(&[
            mk(Attribution::Agentic),
            mk(Attribution::Unattributable),
            mk(Attribution::Unattributable),
            mk(Attribution::Process),
        ]);
        assert_eq!(d.total, 4);
        assert!((d.unattributable_rate - 0.5).abs() < 1e-9, "{}", d.unattributable_rate);
        assert!(d.by_attribution.iter().any(|(k, n)| k == "UNATTRIBUTABLE" && *n == 2));
    }
}
