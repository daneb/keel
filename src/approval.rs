//! Human approval checkpoints (PLAN.md Phase 1: "explicit, recorded").
//!
//! An approval records **what was approved**, not merely that someone approved.
//! Each entry carries the hash of the artefact as it stood at the moment of
//! sign-off; if the spec changes afterwards, the approval no longer applies and
//! the gate says so. Without that, "approved" degrades into a rubber stamp
//! applied once and inherited forever — which is the audit failure the whole
//! evidence design exists to prevent.

use crate::hashing::short;
use crate::paths::Paths;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub schema: String,
    /// `spec` or `plan`.
    pub stage: String,
    pub decision: Decision,
    /// Hash of the artefact at the moment of sign-off.
    pub artefact_hash: String,
    /// The gate verdict that was on the table when the human decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_verdict: Option<String>,
    pub by: String,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub const APPROVAL_SCHEMA: &str = "keel.approval/1";

fn log_path(paths: &Paths, slug: &str) -> PathBuf {
    Spec::dir(paths, slug).join("approvals.jsonl")
}

/// The artefact a stage signs off on.
pub fn artefact_path(paths: &Paths, slug: &str, stage: &str) -> PathBuf {
    match stage {
        "plan" => crate::plan::Plan::path_for(paths, slug),
        _ => Spec::path_for(paths, slug),
    }
}

/// Stages a human can sign off on, in pipeline order.
pub const STAGES: &[&str] = &["spec", "plan", "merge"];

/// Hash of the artefact a stage signs off on. For the plan stage this covers
/// `tasks.md` too — approving a plan whose task list can then change freely
/// would approve nothing.
pub fn artefact_hash(paths: &Paths, slug: &str, stage: &str) -> Result<String> {
    let mut hasher = crate::hashing::SetHasher::new();
    let mut inputs = vec![artefact_path(paths, slug, stage)];
    if stage == "plan" {
        inputs.push(crate::plan::Tasks::path_for(paths, slug));
    }
    if stage == "merge" {
        // Approving a merge approves the whole agreed shape of the work, so a
        // later edit to the plan or tasks supersedes it too.
        inputs.push(crate::plan::Plan::path_for(paths, slug));
        inputs.push(crate::plan::Tasks::path_for(paths, slug));
    }
    for p in inputs {
        let content = std::fs::read(&p)
            .with_context(|| format!("reading {} to hash it", p.display()))?;
        hasher.add(&p.file_name().unwrap_or_default().to_string_lossy(), &content);
    }
    Ok(hasher.finish())
}

pub fn record(
    paths: &Paths,
    slug: &str,
    stage: &str,
    decision: Decision,
    gate_verdict: Option<String>,
    note: Option<String>,
) -> Result<Approval> {
    let approval = Approval {
        schema: APPROVAL_SCHEMA.to_string(),
        stage: stage.to_string(),
        decision,
        artefact_hash: artefact_hash(paths, slug, stage)?,
        gate_verdict,
        by: current_user(paths),
        at: chrono::Local::now().to_rfc3339(),
        note,
    };
    let path = log_path(paths, slug);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let line = serde_json::to_string(&approval)?;
    // Append-only: a decision log you can rewrite is not a decision log.
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)
        .with_context(|| format!("appending to {}", path.display()))?;
    writeln!(f, "{line}")?;
    Ok(approval)
}

pub fn history(paths: &Paths, slug: &str) -> Result<Vec<Approval>> {
    let path = log_path(paths, slug);
    let Ok(raw) = std::fs::read_to_string(&path) else { return Ok(vec![]) };
    let mut out = Vec::new();
    for (n, line) in raw.lines().enumerate() {
        if line.trim().is_empty() { continue; }
        let a: Approval = serde_json::from_str(line)
            .with_context(|| format!("{}:{}", path.display(), n + 1))?;
        out.push(a);
    }
    Ok(out)
}

/// The most recent decision for a stage, whatever it was.
pub fn latest(paths: &Paths, slug: &str, stage: &str) -> Result<Option<Approval>> {
    Ok(history(paths, slug)?.into_iter().rfind(|a| a.stage == stage))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Approved, and the artefact has not changed since.
    Current { by: String, at: String },
    /// Approved, but the artefact has changed — the approval no longer applies.
    Superseded { approved_hash: String, current_hash: String },
    Rejected { by: String, note: Option<String> },
    Absent,
}

/// Whether a stage is approved *as it stands right now*.
pub fn standing(paths: &Paths, slug: &str, stage: &str) -> Result<Standing> {
    let Some(a) = latest(paths, slug, stage)? else { return Ok(Standing::Absent) };
    if a.decision == Decision::Rejected {
        return Ok(Standing::Rejected { by: a.by, note: a.note });
    }
    let current = artefact_hash(paths, slug, stage)?;
    if current == a.artefact_hash {
        Ok(Standing::Current { by: a.by, at: a.at })
    } else {
        Ok(Standing::Superseded {
            approved_hash: short(&a.artefact_hash).to_string(),
            current_hash: short(&current).to_string(),
        })
    }
}

/// Whoever git thinks is working here; the record needs a name against it.
fn current_user(paths: &Paths) -> String {
    let out = std::process::Command::new("git")
        .args(["config", "user.name"])
        .current_dir(&paths.repo)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if name.is_empty() { fallback_user() } else { name }
        }
        _ => fallback_user(),
    }
}

fn fallback_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(Paths);

    /// Unique per call, not merely per nanosecond: `SystemTime::now()` is coarse
    /// enough on some platforms that two threads starting together get the same
    /// value, and two tests sharing a directory is a flake that costs an
    /// afternoon to diagnose.
    fn unique_dir(prefix: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    impl Tmp {
        fn new() -> Self {
            let dir = unique_dir("keel-approval");
            std::fs::create_dir_all(dir.join(".keel/specs/demo")).unwrap();
            std::fs::write(dir.join(".keel/specs/demo/spec.md"), "---\nid: SPEC-1\nslug: demo\n---\n\n# S\n").unwrap();
            Self(Paths { repo: dir })
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0.repo); }
    }

    #[test]
    fn an_absent_approval_is_absent_not_approved() {
        let t = Tmp::new();
        assert_eq!(standing(&t.0, "demo", "spec").unwrap(), Standing::Absent);
    }

    #[test]
    fn approval_holds_while_the_artefact_is_unchanged() {
        let t = Tmp::new();
        record(&t.0, "demo", "spec", Decision::Approved, Some("pass".into()), None).unwrap();
        assert!(matches!(standing(&t.0, "demo", "spec").unwrap(), Standing::Current { .. }));
    }

    #[test]
    fn editing_the_spec_supersedes_its_approval() {
        let t = Tmp::new();
        record(&t.0, "demo", "spec", Decision::Approved, Some("pass".into()), None).unwrap();
        std::fs::write(
            t.0.repo.join(".keel/specs/demo/spec.md"),
            "---\nid: SPEC-1\nslug: demo\n---\n\n# S\n\nA new criterion appeared.\n",
        )
        .unwrap();
        match standing(&t.0, "demo", "spec").unwrap() {
            Standing::Superseded { approved_hash, current_hash } => {
                assert_ne!(approved_hash, current_hash);
            }
            other => panic!("expected superseded, got {other:?}"),
        }
    }

    #[test]
    fn rejection_is_recorded_and_wins_until_superseded() {
        let t = Tmp::new();
        record(&t.0, "demo", "spec", Decision::Rejected, Some("fail".into()), Some("too vague".into())).unwrap();
        match standing(&t.0, "demo", "spec").unwrap() {
            Standing::Rejected { note, .. } => assert_eq!(note.as_deref(), Some("too vague")),
            other => panic!("expected rejected, got {other:?}"),
        }
    }

    #[test]
    fn the_log_is_append_only_and_ordered() {
        let t = Tmp::new();
        record(&t.0, "demo", "spec", Decision::Rejected, None, Some("first".into())).unwrap();
        record(&t.0, "demo", "spec", Decision::Approved, None, Some("second".into())).unwrap();
        let h = history(&t.0, "demo").unwrap();
        assert_eq!(h.len(), 2, "an approval overwrote its predecessor");
        assert_eq!(h[0].note.as_deref(), Some("first"));
        assert_eq!(latest(&t.0, "demo", "spec").unwrap().unwrap().note.as_deref(), Some("second"));
    }
}
