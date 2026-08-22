//! The gate contract (PLAN.md P1, §4.4).
//!
//! > A gate is a predicate over artefacts that returns `pass | fail | blocked`,
//! > with the evidence attached. If a gate cannot fail, it is documentation,
//! > not a gate.
//!
//! `blocked` is deliberately distinct from `fail`: the check could not run at
//! all (missing tool, no network, absent index). A blocked check never silently
//! passes, and — per P6 — never counts as an agentic failure either. Collapsing
//! it into `fail` teaches the failure taxonomy to lie; collapsing it into
//! `pass` is the gate theatre this whole design exists to prevent.

pub mod g0;
pub mod g1;
pub mod g2;
pub mod g25;
pub mod g3;
pub mod g4;
pub mod diff;
pub mod oracle_exec;
pub mod ratchet;

use crate::config::{CheckPlugin, Config};
use crate::paths::Paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const GATE_SCHEMA: &str = "keel.gate/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pass,
    Fail,
    Blocked,
}

impl Verdict {
    pub fn glyph(&self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "FAIL",
            Verdict::Blocked => "BLOCKED",
        }
    }

    /// Exit code for a gate verdict. `blocked` is distinct from `fail` on the
    /// wire too, so a caller can tell "you broke it" from "I could not look".
    pub fn exit_code(&self) -> i32 {
        match self {
            Verdict::Pass => 0,
            Verdict::Fail => 1,
            Verdict::Blocked => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub id: String,
    pub verdict: Verdict,
    /// What the check required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// What it found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// One line a human can act on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Path to the artefact backing this verdict, relative to the gate file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// Where this check came from, e.g. `lesson:L-0012`. Answers "why?".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

impl Check {
    pub fn pass(id: &str, detail: impl Into<String>) -> Self {
        Self { id: id.into(), verdict: Verdict::Pass, expected: None, actual: None,
               detail: Some(detail.into()), evidence: None, from: None }
    }

    pub fn fail(id: &str, expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self { id: id.into(), verdict: Verdict::Fail, expected: Some(expected.into()),
               actual: Some(actual.into()), detail: None, evidence: None, from: None }
    }

    pub fn blocked(id: &str, why: impl Into<String>) -> Self {
        Self { id: id.into(), verdict: Verdict::Blocked, expected: None, actual: None,
               detail: Some(why.into()), evidence: None, from: None }
    }

    /// A single line for the terminal.
    pub fn line(&self) -> String {
        let mut s = format!("  {:<8} {}", self.verdict.glyph(), self.id);
        if let Some(d) = &self.detail {
            s.push_str(&format!(" — {d}"));
        } else if let (Some(e), Some(a)) = (&self.expected, &self.actual) {
            s.push_str(&format!("\n           expected: {e}\n           actual:   {a}"));
        }
        if let Some(from) = &self.from {
            s.push_str(&format!("  [{from}]"));
        }
        s
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub schema: String,
    pub gate: String,
    pub run: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    pub verdict: Verdict,
    pub generated_at: String,
    pub checks: Vec<Check>,
}

impl GateResult {
    pub fn new(gate: &str, spec: Option<String>, checks: Vec<Check>) -> Self {
        Self {
            schema: GATE_SCHEMA.to_string(),
            gate: gate.to_string(),
            run: run_id(),
            spec,
            verdict: roll_up(&checks),
            generated_at: chrono::Local::now().to_rfc3339(),
            checks,
        }
    }

    pub fn write(&self, dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.json", self.gate));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, format!("{json}\n"))
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let p = self.checks.iter().filter(|c| c.verdict == Verdict::Pass).count();
        let f = self.checks.iter().filter(|c| c.verdict == Verdict::Fail).count();
        let b = self.checks.iter().filter(|c| c.verdict == Verdict::Blocked).count();
        (p, f, b)
    }
}

/// Any failure fails the gate; otherwise any blocked check blocks it.
///
/// A gate with no checks at all is `blocked`, not `pass` — an empty gate is a
/// misconfiguration, and reporting it as success is precisely how a pipeline
/// ends up with gates that cannot fail.
pub fn roll_up(checks: &[Check]) -> Verdict {
    if checks.is_empty() {
        return Verdict::Blocked;
    }
    if checks.iter().any(|c| c.verdict == Verdict::Fail) {
        return Verdict::Fail;
    }
    if checks.iter().any(|c| c.verdict == Verdict::Blocked) {
        return Verdict::Blocked;
    }
    Verdict::Pass
}

/// `2026-08-21-7c1` — sortable by date, unique enough within a day.
pub fn run_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    let salt = now ^ (std::process::id() as u64) << 17;
    format!("{}-{:03x}", crate::store::today(), salt & 0xfff)
}

/// Directory holding gate results for a spec.
pub fn dir_for(paths: &Paths, slug: &str) -> PathBuf {
    crate::spec::Spec::dir(paths, slug).join("gates")
}

/// Load a previously recorded verdict, if any.
pub fn previous(paths: &Paths, slug: &str, gate: &str) -> Option<GateResult> {
    let p = dir_for(paths, slug).join(format!("{gate}.json"));
    GateResult::read(&p).ok()
}

// ---------------------------------------------------------------------------
// External checks (P7)
// ---------------------------------------------------------------------------

/// Run the configured plugin checks for a gate.
///
/// A plugin that cannot be executed yields `blocked`, never `fail`: the
/// distinction is the whole reason the third verdict exists.
pub fn run_plugins(paths: &Paths, cfg: &Config, gate: &str, slug: Option<&str>) -> Vec<Check> {
    let Some(gate_cfg) = cfg.gate.get(gate) else { return vec![] };
    gate_cfg
        .checks
        .iter()
        .map(|plugin| run_plugin(paths, plugin, gate, slug))
        .collect()
}

fn run_plugin(paths: &Paths, plugin: &CheckPlugin, gate: &str, slug: Option<&str>) -> Check {
    let mut parts = plugin.cmd.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>();
    if parts.is_empty() {
        return Check::blocked(&plugin.id, "check has an empty cmd");
    }
    let program = parts.remove(0);

    let mut command = std::process::Command::new(&program);
    command
        .args(&parts)
        .current_dir(&paths.repo)
        .env("KEEL_REPO", &paths.repo)
        .env("KEEL_STORE", paths.store())
        .env("KEEL_GATE", gate);
    if let Some(s) = slug {
        command.env("KEEL_SPEC", s).env("KEEL_SPEC_DIR", crate::spec::Spec::dir(paths, s));
    }

    let output = match command.output() {
        Ok(o) => o,
        Err(e) => {
            return with_from(
                Check::blocked(&plugin.id, format!("could not run `{}`: {e}", plugin.cmd)),
                plugin,
            );
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let check = match serde_json::from_str::<Check>(stdout.trim()) {
        Ok(mut c) => {
            // The plugin does not get to rename itself.
            c.id = plugin.id.clone();
            c
        }
        Err(_) if stdout.trim().is_empty() && output.status.success() => {
            Check::pass(&plugin.id, "exited 0 with no output")
        }
        Err(e) => Check::blocked(
            &plugin.id,
            format!(
                "did not print a valid check result ({e}); stderr: {}",
                truncate(String::from_utf8_lossy(&output.stderr).trim(), 160)
            ),
        ),
    };
    with_from(check, plugin)
}

fn with_from(mut c: Check, plugin: &CheckPlugin) -> Check {
    if c.from.is_none() {
        c.from = plugin.from.clone();
    }
    c
}

/// Join a list for a gate message, capping it so one failing check cannot
/// produce a paragraph nobody reads.
pub(crate) fn join_capped(items: &[String], cap: usize) -> String {
    if items.len() <= cap {
        return items.join(", ");
    }
    format!(
        "{}, … and {} more",
        items[..cap].join(", "),
        items.len() - cap
    )
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    s.chars().take(max.saturating_sub(1)).chain(['…']).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_failure_fails_the_gate() {
        let checks = vec![
            Check::pass("a", "fine"),
            Check::blocked("b", "no network"),
            Check::fail("c", "0 drift", "1 drift"),
        ];
        assert_eq!(roll_up(&checks), Verdict::Fail);
    }

    #[test]
    fn blocked_never_silently_passes() {
        let checks = vec![Check::pass("a", "fine"), Check::blocked("b", "tool missing")];
        assert_eq!(roll_up(&checks), Verdict::Blocked);
        assert_ne!(roll_up(&checks), Verdict::Pass);
    }

    #[test]
    fn an_empty_gate_is_blocked_not_passed() {
        assert_eq!(roll_up(&[]), Verdict::Blocked);
    }

    #[test]
    fn all_passing_passes() {
        assert_eq!(roll_up(&[Check::pass("a", "x"), Check::pass("b", "y")]), Verdict::Pass);
    }

    #[test]
    fn verdicts_have_distinct_exit_codes() {
        assert_eq!(Verdict::Pass.exit_code(), 0);
        assert_eq!(Verdict::Fail.exit_code(), 1);
        assert_eq!(Verdict::Blocked.exit_code(), 3);
    }

    #[test]
    fn gate_result_round_trips_through_json() {
        let r = GateResult::new("G0", Some("rate-limit".into()), vec![
            Check::fail("oracle-presence", "every criterion has an oracle", "AC-2 has none"),
        ]);
        let json = serde_json::to_string(&r).unwrap();
        let back: GateResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema, GATE_SCHEMA);
        assert_eq!(back.gate, "G0");
        assert_eq!(back.verdict, Verdict::Fail);
        assert_eq!(back.checks[0].expected.as_deref(), Some("every criterion has an oracle"));
    }

    #[test]
    fn long_lists_are_capped_in_gate_messages() {
        let items: Vec<String> = (1..=9).map(|n| format!("T-{n}")).collect();
        let out = join_capped(&items, 5);
        assert!(out.starts_with("T-1, T-2, T-3, T-4, T-5"), "{out}");
        assert!(out.ends_with("and 4 more"), "{out}");
        assert_eq!(join_capped(&items[..3], 5), "T-1, T-2, T-3");
    }

    #[test]
    fn run_ids_are_dated_and_distinct() {
        let a = run_id();
        assert!(a.starts_with(&crate::store::today()), "{a}");
        assert_eq!(a.len(), crate::store::today().len() + 4);
    }
}
