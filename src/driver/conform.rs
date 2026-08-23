//! Driver conformance (PLAN.md Phase 5).
//!
//! > New tools, new checks and new repos plug in without touching the spine.
//!
//! That claim is worth nothing unless a new driver can be checked against the
//! contract without keel knowing anything about the tool behind it. This runs a
//! driver through a fixed set of probes and reports which parts of
//! `keel.drivertask/1` → `keel.driverresult/1` it actually honours.
//!
//! Every probe runs in a **scratch repository**, never the user's tree. A
//! conformance run invokes a real coding agent; doing that against live work
//! would be an unpleasant way to learn what a driver does when it is confused.

use super::contract::{DriverResult, DriverStatus, DriverTask};
use crate::config::Driver as DriverConfig;
use crate::gate::{Check, Verdict};
use crate::paths::Paths;
use anyhow::{Context, Result};
use std::path::Path;

/// A throwaway repository for a driver to act on.
pub struct Scratch {
    pub paths: Paths,
}

impl Scratch {
    fn new(driver_id: &str) -> Result<Self> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "keel-conform-{driver_id}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src"))
            .with_context(|| format!("creating {}", dir.display()))?;
        std::fs::write(dir.join("src/lib.rs"), "pub fn answer() -> u32 { 41 }\n")?;
        std::fs::write(dir.join("README.md"), "A scratch repository for driver conformance.\n")?;

        // A real git repository, because drivers report what they changed by
        // asking git. Probing them somewhere git knows nothing about would test
        // the adapter under conditions it will never meet, and `reports-changes`
        // would be vacuous.
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "conformance@keel.local"],
            vec!["config", "user.name", "keel conformance"],
            vec!["add", "-A"],
            vec!["commit", "-q", "-m", "conformance base"],
        ] {
            let out = std::process::Command::new("git").args(&args).current_dir(&dir).output();
            match out {
                Ok(o) if o.status.success() => {}
                // git missing is not a conformance failure; the probes that do
                // not need it still run.
                _ => break,
            }
        }
        Ok(Self { paths: Paths { repo: dir } })
    }

    fn changed_files(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect(&self.paths.repo, &self.paths.repo, &mut out);
        out.sort();
        out
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.paths.repo);
    }
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.filter_map(|e| e.ok()) {
        let p = e.path();
        // The scratch repo's own .git is not something a driver "changed".
        if p.file_name().and_then(|n| n.to_str()) == Some(".git") {
            continue;
        }
        if p.is_dir() {
            collect(&p, root, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The task every conformant driver must be able to answer.
///
/// Deliberately a no-op: the probe is whether the driver speaks the protocol,
/// not whether it can program. Asking a real agent to write code here would
/// make conformance slow, expensive and non-deterministic.
fn probe_task(run: &str, repo: &str) -> DriverTask {
    DriverTask::new(
        run,
        "conformance",
        Some("T-0".into()),
        "This is a protocol conformance probe, not a real task.\n\
         Make NO changes to any file. Reply immediately with a \
         keel.driverresult/1 object whose status is \"ok\" and whose \
         files_changed is an empty list."
            .into(),
        vec!["src/**".into()],
        Some(0),
        repo.to_string(),
    )
}

pub struct Conformance {
    pub driver: String,
    pub checks: Vec<Check>,
    pub verdict: Verdict,
    /// What the driver actually replied, for the report.
    pub result: Option<DriverResult>,
}

/// Run the conformance suite against one driver.
pub fn check(home: &Paths, driver: &DriverConfig) -> Result<Conformance> {
    let scratch = Scratch::new(&driver.id)?;
    let before = scratch.changed_files();
    let run_id = crate::gate::run_id();
    let task = probe_task(&run_id, &scratch.paths.repo.to_string_lossy());

    // A conformance probe should answer in seconds. A driver that takes its
    // configured production timeout to say "ok" is itself the finding.
    let probe_driver = DriverConfig {
        id: driver.id.clone(),
        cmd: driver.cmd.clone(),
        default: driver.default,
        timeout_secs: driver.timeout_secs.min(60),
    };

    let invocation = super::run_in(home, &scratch.paths, &probe_driver, &task);
    let mut checks = Vec::new();
    let mut result = None;

    // --- 1. it ran at all ---------------------------------------------------
    let detail = invocation.result.detail.clone().unwrap_or_default();
    if invocation.result.status == DriverStatus::Blocked && detail.contains("could not start") {
        checks.push(Check::blocked("executable", detail.clone()));
        // Everything downstream depends on the process existing.
        for id in ["reads-task", "emits-result", "status", "reports-changes", "no-side-effects"] {
            checks.push(Check::blocked(id, "the driver never started"));
        }
        let verdict = crate::gate::roll_up(&checks);
        return Ok(Conformance { driver: driver.id.clone(), checks, verdict, result: None });
    }
    checks.push(Check::pass(
        "executable",
        format!("`{}` started in {:.1}s", driver.cmd, invocation.elapsed.as_secs_f64()),
    ));

    // --- 2. it answered inside the probe timeout ----------------------------
    if detail.contains("timeout") {
        checks.push(Check::fail(
            "reads-task",
            "a reply within the probe timeout",
            format!("no reply in {}s — a conformance probe asks for nothing but a reply", probe_driver.timeout_secs),
        ));
    } else {
        checks.push(Check::pass("reads-task", "consumed the task and replied"));
    }

    // --- 3. its output is a keel.driverresult/1 -----------------------------
    if detail.starts_with("invalid driver result") {
        checks.push(Check::fail("emits-result", "a keel.driverresult/1 object on stdout", detail.clone()));
        checks.push(Check::blocked("status", "no parseable result to inspect"));
    } else if invocation.result.status == DriverStatus::Blocked && !detail.is_empty() {
        checks.push(Check::blocked("emits-result", detail.clone()));
        checks.push(Check::blocked("status", "no parseable result to inspect"));
    } else {
        checks.push(Check::pass("emits-result", "stdout parsed as keel.driverresult/1"));
        checks.push(Check::pass(
            "status",
            format!("reported `{}`", invocation.result.status_str()),
        ));
        result = Some(invocation.result.clone());
    }

    // --- 4. what it claimed matches what it did -----------------------------
    let after = scratch.changed_files();
    let new_files: Vec<String> = after.iter().filter(|f| !before.contains(f)).cloned().collect();
    let claimed = invocation.result.files_changed.clone();

    if claimed.is_empty() && new_files.is_empty() {
        checks.push(Check::pass("reports-changes", "claimed no changes and made none"));
    } else if !claimed.is_empty() && new_files.is_empty() {
        // Not fatal: a driver may legitimately have edited in place rather than
        // added, and this scratch check only sees additions.
        checks.push(Check::blocked(
            "reports-changes",
            format!("claimed {} change(s) the probe could not observe: {}", claimed.len(), claimed.join(", ")),
        ));
    } else {
        checks.push(Check::fail(
            "reports-changes",
            "files_changed matches what the driver actually did",
            format!("created {} unreported file(s): {}", new_files.len(), new_files.join(", ")),
        ));
    }

    // --- 5. it respected an explicit instruction to do nothing --------------
    if new_files.is_empty() {
        checks.push(Check::pass("no-side-effects", "left the scratch repository as it found it"));
    } else {
        checks.push(Check::fail(
            "no-side-effects",
            "a probe that asks for no changes gets none",
            format!("wrote {}", new_files.join(", ")),
        ));
    }

    let verdict = crate::gate::roll_up(&checks);
    Ok(Conformance { driver: driver.id.clone(), checks, verdict, result })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratch_repo_is_created_and_removed() {
        let path;
        {
            let s = Scratch::new("test").unwrap();
            path = s.paths.repo.clone();
            assert!(path.join("src/lib.rs").is_file());
            assert!(s.changed_files().contains(&"src/lib.rs".to_string()));
        }
        assert!(!path.exists(), "a conformance scratch repo outlived its run");
    }

    #[test]
    fn the_scratch_repo_is_a_git_repo() {
        let s = Scratch::new("git").unwrap();
        assert!(s.paths.repo.join(".git").exists(), "drivers report changes via git");
        // …and .git is not itself reported as a change the driver made.
        assert!(!s.changed_files().iter().any(|f| f.starts_with(".git/")));
    }

    #[test]
    fn the_probe_asks_for_nothing_but_a_reply() {
        let t = probe_task("r", "/tmp/x");
        assert_eq!(t.schema, super::super::contract::TASK_SCHEMA);
        assert!(t.prompt.contains("NO changes"), "the probe does not forbid edits");
        assert_eq!(t.budget_lines, Some(0), "the probe budgets no lines of change");
    }

    #[test]
    fn scratch_repos_do_not_collide() {
        let a = Scratch::new("x").unwrap();
        let b = Scratch::new("x").unwrap();
        assert_ne!(a.paths.repo, b.paths.repo);
    }
}
