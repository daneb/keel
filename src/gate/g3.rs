//! **G3 — the human decision.**
//!
//! Checks: the evidence bundle exists and verifies; the diff is small enough
//! that a person can actually review it; earlier gates passed; and a human
//! verdict is on the record against *this* run's evidence.
//!
//! The reviewable-size check is the one people argue with. It is here because a
//! 4,000-line diff does not get reviewed, it gets approved — and a gate that
//! records an approval nobody could have given is worse than no gate.

use super::{Check, GateResult, diff, run_plugins};
use crate::approval::{self, Standing};
use crate::config::Config;
use crate::paths::Paths;
use crate::run::Run;
use crate::spec::Spec;
use anyhow::Result;

pub fn run(paths: &Paths, cfg: &Config, spec: &Spec, run: &Run) -> Result<GateResult> {
    let mut checks = vec![
        earlier_gates(run)?,
        reviewable_size(paths, cfg, run),
        evidence_complete(run),
        human_oracles(paths, run),
        human_verdict(paths, &spec.front.slug)?,
    ];
    checks.extend(run_plugins(paths, cfg, "G3", Some(&spec.front.slug)));
    Ok(GateResult::new(paths, "G3", Some(spec.front.slug.clone()), checks))
}

/// G3 cannot be the first gate to notice a problem.
fn earlier_gates(run: &Run) -> Result<Check> {
    let results = run.gate_results()?;
    let relevant: Vec<&crate::gate::GateResult> =
        results.iter().filter(|r| r.gate == "G2" || r.gate == "G2.5").collect();

    if relevant.is_empty() {
        return Ok(Check::fail(
            "earlier-gates",
            "G2 and G2.5 have run",
            "neither has run for this run — `keel run` executes them",
        ));
    }
    let named = |v: super::Verdict| -> Vec<String> {
        relevant.iter().filter(|r| r.verdict == v).map(|r| r.gate.clone()).collect()
    };
    let failed = named(super::Verdict::Fail);
    let blocked = named(super::Verdict::Blocked);

    if !failed.is_empty() {
        return Ok(Check::fail("earlier-gates", "G2 and G2.5 pass", format!("{} failed", failed.join(", "))));
    }
    if !blocked.is_empty() {
        // A blocked earlier gate is not a failed one. Reporting it as `fail`
        // here would tell the Phase 3 taxonomy that the agent broke something,
        // when in fact keel never looked.
        return Ok(Check::blocked(
            "earlier-gates",
            format!("{} could not complete — G3 has nothing to ratify", blocked.join(", ")),
        ));
    }
    Ok(Check::pass(
        "earlier-gates",
        format!("{} passed", relevant.iter().map(|r| r.gate.as_str()).collect::<Vec<_>>().join(" and ")),
    ))
}

pub fn reviewable(paths: &Paths, cfg: &Config, run: &Run) -> Option<(usize, usize)> {
    let base = run.meta.base_commit.clone().unwrap_or_else(|| diff::default_base(paths));
    let d = diff::against(paths, &base).ok()?;
    let files: Vec<_> = d.files.iter().filter(|f| !super::g2::is_incidental_for(cfg, &f.path)).collect();
    Some((files.len(), files.iter().map(|f| f.churn()).sum()))
}

fn reviewable_size(paths: &Paths, cfg: &Config, run: &Run) -> Check {
    let Some((files, churn)) = reviewable(paths, cfg, run) else {
        return Check::blocked("reviewable-size", "could not read the diff");
    };
    let max = cfg.plan.max_reviewable_lines;
    if churn > max {
        return Check::fail(
            "reviewable-size",
            format!("at most {max} lines for one human review"),
            // Same filter as `churn`: keel's own run evidence (`.keel/runs/**`,
            // created by the very act of gating) is untracked until committed,
            // and counting it here would tell a reviewer their diff touches
            // dozens of files it does not.
            format!("{churn} lines across {files} files — split it, or raise the limit knowing what you are buying"),
        );
    }
    Check::pass("reviewable-size", format!("{churn}/{max} reviewable lines"))
}

fn evidence_complete(run: &Run) -> Check {
    let mut missing: Vec<String> = Vec::new();
    for m in crate::run::required_members() {
        if !run.dir.join(m).exists() {
            missing.push((*m).to_string());
        }
    }
    if run.gate_results().map(|g| g.is_empty()).unwrap_or(true) {
        missing.push("gates/".to_string());
    }
    if missing.is_empty() {
        Check::pass("evidence-complete", "trajectory, metadata and gate results present")
    } else {
        Check::fail("evidence-complete", "a complete run record", format!("missing: {}", missing.join(", ")))
    }
}

/// Each `human` oracle is its own judgement, bound to this run's evidence. One
/// merge approval proves a person said yes to a hash; it does not say they
/// looked at the criteria only a person can judge.
fn human_oracles(paths: &Paths, run: &Run) -> Check {
    let Ok(raw) = std::fs::read_to_string(run.evidence_dir().join("oracles.json")) else {
        return Check::blocked("human-oracles", "evidence/oracles.json is missing — G2 has not recorded the oracles");
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Check::blocked("human-oracles", "evidence/oracles.json is unreadable");
    };
    let mut open = Vec::new();
    let mut total = 0;
    for r in v["runs"].as_array().into_iter().flatten().filter(|r| r["outcome"] == "human") {
        let id = r["criterion"].as_str().unwrap_or("?");
        total += 1;
        let state = match approval::judgement_standing(paths, run, id) {
            Ok(Standing::Current { .. }) => continue,
            Ok(Standing::Absent) => "unjudged".to_string(),
            Ok(Standing::Rejected { by, note }) => {
                format!("judged FAIL by {by}{}", note.map(|n| format!(": {n}")).unwrap_or_default())
            }
            Ok(Standing::Superseded { approved_hash: a, current_hash: c }) => {
                format!("judgement superseded, evidence changed ({a} → {c})")
            }
            Err(e) => format!("could not read the judgement: {e:#}"),
        };
        open.push(format!("{id} {state} — {}", r["oracle"].as_str().unwrap_or("")));
    }
    if open.is_empty() {
        return Check::pass("human-oracles", format!("{total} human oracle(s) judged against this evidence"));
    }
    Check::fail("human-oracles", "every human oracle judged pass against the current evidence", open.join("; "))
}

fn human_verdict(paths: &Paths, slug: &str) -> Result<Check> {
    Ok(match approval::standing(paths, slug, "merge")? {
        Standing::Current { by, at } => {
            Check::pass("human-verdict", format!("approved by {by} at {}", &at[..at.len().min(19)]))
        }
        Standing::Absent => Check::fail(
            "human-verdict",
            "a recorded human decision on the change",
            format!("none — run `keel approve {slug} --stage merge`"),
        ),
        Standing::Rejected { by, note } => Check::fail(
            "human-verdict",
            "the change is approved",
            format!("rejected by {by}{}", note.map(|n| format!(": {n}")).unwrap_or_default()),
        ),
        Standing::Superseded { approved_hash, current_hash } => Check::fail(
            "human-verdict",
            "the approved artefacts are the current ones",
            format!("changed after approval ({approved_hash} → {current_hash}) — re-approve"),
        ),
    })
}
