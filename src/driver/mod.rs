//! Running a task against somebody else's coding agent.
//!
//! One function owns the whole subprocess lifecycle — stdin, stdout, timeout,
//! termination — so adding a driver is a config entry, never new process code.
//!
//! The `blocked` discipline from P6 is enforced here: a driver that cannot
//! start, or that runs out of time, is `blocked`, not `failed`. Only a driver
//! that ran and could not do the job is an agentic failure. Getting this wrong
//! teaches the Phase 3 failure taxonomy to learn from noise.

pub mod builtin;
pub mod conform;
pub mod contract;

use crate::config::{Config, Driver as DriverConfig};
use crate::paths::Paths;
use anyhow::{Result, bail};
pub use contract::{DriverResult, DriverStatus, DriverTask};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How often to check whether the driver has exited. Short enough that a fast
/// driver is not held up, long enough not to spin.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long to keep draining a timed-out driver's pipes before giving up on
/// them. Killing the process group should already have closed both.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

pub struct Invocation {
    pub result: DriverResult,
    pub elapsed: Duration,
    pub stderr: String,
}

/// Execute a driver against a task.
///
/// Never returns `Err` for anything the driver did — a broken driver becomes a
/// `blocked` result so it lands in the trajectory and on the gate report rather
/// than aborting the run.
pub fn run(paths: &Paths, driver: &DriverConfig, task: &DriverTask) -> Invocation {
    run_in(paths, paths, driver, task)
}

/// Run a driver whose adapter lives in `home` against a working tree at `cwd`.
///
/// These are the same directory in normal use, and different exactly once: the
/// conformance suite runs a driver against a scratch repository. A driver
/// command like `.keel/drivers/codex` is relative to the repository that
/// *configured* it, not to wherever the child happens to be started — resolving
/// it against `cwd` made every driver unreachable under conformance.
pub fn run_in(
    home: &Paths,
    cwd: &Paths,
    driver: &DriverConfig,
    task: &DriverTask,
) -> Invocation {
    let started = Instant::now();
    let timeout = Duration::from_secs(driver.timeout_secs);

    let mut parts = driver.cmd.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>();
    if parts.is_empty() {
        return blocked(started, format!("driver `{}` has an empty cmd", driver.id));
    }
    let program = resolve_program(home, &parts.remove(0));

    let payload = match serde_json::to_string(task) {
        Ok(p) => p,
        Err(e) => return blocked(started, format!("could not serialise the task: {e}")),
    };

    let mut command = Command::new(&program);
    command
        .args(&parts)
        .current_dir(&cwd.repo)
        .env("KEEL_REPO", &cwd.repo)
        .env("KEEL_STORE", cwd.store())
        .env("KEEL_RUN", &task.run)
        .env("KEEL_SPEC", &task.spec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Give the driver its own process group so a timeout can kill the whole
    // tree. Without this, killing the driver leaves its children holding the
    // stdout pipe open, and the read below blocks until they finish anyway —
    // which is to say, the timeout would not be a timeout.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return blocked(started, format!("could not start `{}`: {e}", driver.cmd));
        }
    };
    let pid = child.id();

    if let Some(mut stdin) = child.stdin.take() {
        // A driver that closed stdin early is still worth waiting for; it may
        // have read enough already.
        let _ = stdin.write_all(payload.as_bytes());
    }

    // Drain both pipes on their own threads: a driver that fills the 64KB pipe
    // buffer deadlocks against a parent that waits before reading. They hand
    // the text back over a channel rather than a JoinHandle, because a join
    // cannot be given a deadline and the timeout path must not wait on a pipe
    // that something surviving still holds open.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let (err_tx, err_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = out_tx.send(read_all(&mut stdout_pipe));
    });
    std::thread::spawn(move || {
        let _ = err_tx.send(read_all(&mut stderr_pipe));
    });

    // Poll for exit against the deadline, keeping the Child here so it can be
    // killed — `wait_with_output` gives that handle away and cannot be interrupted.
    let deadline = started + timeout;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    kill_group(&mut child, pid);
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => return blocked(started, format!("could not wait on the driver: {e}")),
        }
    }

    // A driver that exited on its own closed both pipes, so these arrive at
    // once. After a timeout they may never arrive at all: killing the process
    // group is meant to close them, and a surviving grandchild would keep
    // stdout open regardless. Bounding the wait is what makes the timeout hold
    // even when killing the tree did not work — losing a driver's dying words
    // is a fair price for a deadline that is actually a deadline.
    let (stdout, stderr) = if timed_out {
        (
            out_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default(),
            err_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default(),
        )
    } else {
        (out_rx.recv().unwrap_or_default(), err_rx.recv().unwrap_or_default())
    };
    let stderr = stderr.trim().to_string();

    if timed_out {
        let mut inv = blocked(
            started,
            format!("driver `{}` exceeded its {}s timeout and was terminated", driver.id, driver.timeout_secs),
        );
        inv.stderr = stderr;
        return inv;
    }

    match contract::parse_result(&stdout) {
        Ok(result) => Invocation { result, elapsed: started.elapsed(), stderr },
        Err(why) => {
            // Invalid output is the driver's fault, not the environment's, but
            // it is still "keel could not learn anything", so it blocks.
            let mut inv = blocked(started, format!("invalid driver result — {why}"));
            inv.stderr = stderr;
            inv
        }
    }
}

fn read_all<R: std::io::Read>(pipe: &mut Option<R>) -> String {
    let Some(p) = pipe.as_mut() else { return String::new() };
    let mut buf = Vec::new();
    let _ = p.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).to_string()
}

/// Kill the driver and everything it started.
///
/// On Unix the child leads its own process group, so a negative pid signals the
/// whole group — which is the only way a `sleep` grandchild actually dies. The
/// earlier version of this shelled out to `pkill -f <name>`; that matched on a
/// substring of every command line on the machine, and could have killed
/// processes that had nothing to do with keel.
#[cfg(unix)]
fn kill_group(child: &mut std::process::Child, pid: u32) {
    // `libc::kill` rather than shelling out to `kill -KILL -<pgid>`: a negative
    // pid means "the whole process group" to the syscall, unambiguously, while
    // the *command* spells that differently across implementations — BSD kill
    // accepts `-KILL -123`, procps-ng reads the second argument as another
    // option and needs `-s KILL -- -123`. Shelling out therefore killed the
    // group on macOS and silently did nothing on Linux, where the surviving
    // grandchild held stdout open and the timeout stopped being a timeout.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_group(child: &mut std::process::Child, pid: u32) {
    // No process groups: taskkill's tree flag is the equivalent, scoped to this
    // pid rather than to a name.
    let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output();
    let _ = child.kill();
}

/// A relative adapter path is resolved against the configuring repository; a
/// bare name (`claude`, `npx`) is left alone for `PATH` to find.
fn resolve_program(home: &Paths, program: &str) -> String {
    let p = std::path::Path::new(program);
    if p.is_absolute() || !program.contains('/') {
        return program.to_string();
    }
    let candidate = home.repo.join(p);
    if candidate.exists() {
        candidate.to_string_lossy().to_string()
    } else {
        program.to_string()
    }
}

fn blocked(started: Instant, detail: String) -> Invocation {
    Invocation {
        result: DriverResult::blocked(detail),
        elapsed: started.elapsed(),
        stderr: String::new(),
    }
}

/// The driver to use: the named one, else the configured default.
/// Write every reference driver script into `.keel/drivers/`, and append a
/// `[[driver]]` entry for any that config does not already have.
///
/// Idempotent: an existing script is kept, not overwritten, unless `force` is
/// set — the same rule `keel init` already uses for steering docs, so re-running
/// this after hand-editing a driver does not clobber the edit by accident.
///
/// `keel.toml` is edited by *appending text*, never by loading it into
/// `Config` and writing the struct back out. A full round-trip through
/// `toml::to_string_pretty` has no memory of comments or section order —
/// `Config` does not store either — so it would silently strip every comment
/// in the file and shuffle every section on a command whose only job is to
/// add what is missing. `cfg` is read to know what already exists; it is
/// never the thing written.
///
/// Returns one line per file or config entry touched, for the caller to print.
pub fn scaffold(paths: &Paths, cfg: &Config, force: bool) -> Result<Vec<String>> {
    let dir = paths.keel().join("drivers");
    std::fs::create_dir_all(&dir)?;
    let mut lines = Vec::new();
    let mut to_register: Vec<(&'static str, &'static str, bool, u64)> = Vec::new();

    for b in builtin::ALL {
        let path = dir.join(b.filename);
        if path.exists() && !force {
            lines.push(format!("  kept    drivers/{}", b.filename));
        } else {
            std::fs::write(&path, b.content)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
            lines.push(format!("  created drivers/{}", b.filename));
        }

        if let Some(id) = b.driver_id
            && !cfg.drivers.iter().any(|d| d.id == id)
        {
            to_register.push((id, b.filename, b.default, b.timeout_secs));
            lines.push(format!("  added   [[driver]] {id} to keel.toml"));
        }
    }

    if !to_register.is_empty() {
        append_driver_entries(&paths.config(), &to_register)?;
    }

    // Non-driver assets ride along: they are scripts keel ships and the user
    // owns once written, exactly like a driver, but they are wired into config
    // by their own section rather than [[driver]].
    for a in builtin::ASSETS {
        let path = paths.keel().join(a.rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if path.exists() && !force {
            lines.push(format!("  kept    {}", a.rel));
        } else {
            std::fs::write(&path, a.content)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
            lines.push(format!("  created {}", a.rel));
        }
    }
    Ok(lines)
}

/// Append `[[driver]]` TOML blocks to the end of `keel.toml`'s raw text.
fn append_driver_entries(
    cfg_path: &std::path::Path,
    entries: &[(&'static str, &'static str, bool, u64)],
) -> Result<()> {
    let mut existing = std::fs::read_to_string(cfg_path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", cfg_path.display()))?;
    if !existing.ends_with('\n') {
        existing.push('\n');
    }
    for (id, filename, default, timeout_secs) in entries {
        existing.push_str(&format!(
            "\n[[driver]]\nid = \"{id}\"\ncmd = \".keel/drivers/{filename}\"\ndefault = {default}\ntimeout_secs = {timeout_secs}\n"
        ));
    }
    std::fs::write(cfg_path, existing)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", cfg_path.display()))
}

pub fn select<'a>(cfg: &'a Config, id: Option<&str>) -> Result<&'a DriverConfig> {
    if let Some(id) = id {
        return cfg
            .drivers
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow::anyhow!("no driver `{id}` in .keel/keel.toml"));
    }
    if let Some(d) = cfg.drivers.iter().find(|d| d.default) {
        return Ok(d);
    }
    match cfg.drivers.len() {
        0 => bail!("no drivers configured — add a [[driver]] block to .keel/keel.toml"),
        1 => Ok(&cfg.drivers[0]),
        _ => bail!("several drivers configured and none is default — pass --driver <id>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_writes_every_reference_script_and_registers_it() {
        let dir = std::env::temp_dir().join(format!("keel-scaffold-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".keel")).unwrap();
        let home = Paths { repo: dir.clone() };
        let cfg = Config { drivers: vec![], ..Default::default() };
        cfg.save(&home.config()).unwrap();

        let lines = scaffold(&home, &cfg, false).unwrap();
        // scaffold appends to keel.toml as text; re-load to see what landed,
        // the same way a real caller (init, `keel driver scaffold`) would.
        let cfg = Config::load(&home.config()).unwrap();
        assert!(lines.iter().any(|l| l.contains("created drivers/claude-code")));
        assert!(lines.iter().any(|l| l.contains("created drivers/kiro")));
        // _common.sh is sourced by the others, not a driver of its own.
        assert!(!cfg.drivers.iter().any(|d| d.id == "_common.sh"));

        for id in ["claude-code", "codex", "copilot", "kiro", "noop"] {
            let d = cfg.drivers.iter().find(|d| d.id == id).unwrap_or_else(|| panic!("no {id} entry"));
            let script = dir.join(&d.cmd);
            assert!(script.is_file(), "{id}'s script was not written: {}", script.display());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&script).unwrap().permissions().mode();
                assert_ne!(mode & 0o111, 0, "{id}'s script is not executable");
            }
        }
        assert!(cfg.drivers.iter().find(|d| d.id == "claude-code").unwrap().default);
        assert!(!cfg.drivers.iter().find(|d| d.id == "kiro").unwrap().default);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scaffold_neither_overwrites_a_hand_edit_nor_reregisters_it_twice() {
        let dir = std::env::temp_dir().join(format!("keel-scaffold-idem-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".keel/drivers")).unwrap();
        std::fs::write(dir.join(".keel/drivers/kiro"), "#!/bin/sh\necho mine\n").unwrap();
        let home = Paths { repo: dir.clone() };
        let cfg = Config {
            drivers: vec![DriverConfig {
                id: "kiro".into(),
                cmd: ".keel/drivers/kiro".into(),
                default: false,
                timeout_secs: 900,
            }],
            ..Default::default()
        };
        cfg.save(&home.config()).unwrap();
        let toml_before = std::fs::read_to_string(home.config()).unwrap();

        scaffold(&home, &cfg, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(".keel/drivers/kiro")).unwrap(),
            "#!/bin/sh\necho mine\n",
            "an existing script must survive a non-forced scaffold"
        );

        let toml_after = std::fs::read_to_string(home.config()).unwrap();
        // Append-only: the bytes already on disk -- including kiro's own
        // entry, in its original position -- must be an untouched prefix.
        // A full round-trip through Config would instead reformat and
        // reorder the whole file, which is the bug this guards against.
        assert!(
            toml_after.starts_with(&toml_before),
            "scaffold must only append; the original file content must survive as a prefix.\nbefore:\n{toml_before}\nafter:\n{toml_after}"
        );

        let cfg = Config::load(&home.config()).unwrap();
        assert_eq!(
            cfg.drivers.iter().filter(|d| d.id == "kiro").count(),
            1,
            "an already-configured driver must not gain a second [[driver]] entry"
        );
        // The other four were genuinely missing, so they are the ones scaffold
        // should have added.
        for id in ["claude-code", "codex", "copilot", "noop"] {
            assert_eq!(
                cfg.drivers.iter().filter(|d| d.id == id).count(),
                1,
                "{id} was missing and should have been added exactly once"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_relative_adapter_resolves_against_the_configuring_repo() {
        let dir = std::env::temp_dir().join(format!("keel-resolve-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".keel/drivers")).unwrap();
        std::fs::write(dir.join(".keel/drivers/x"), "#!/bin/sh\n").unwrap();
        let home = Paths { repo: dir.clone() };

        let resolved = resolve_program(&home, ".keel/drivers/x");
        assert!(std::path::Path::new(&resolved).is_absolute(), "{resolved}");
        assert!(resolved.ends_with(".keel/drivers/x"));

        // A bare name is PATH's business, not keel's.
        assert_eq!(resolve_program(&home, "claude"), "claude");
        // An absolute path is left alone.
        assert_eq!(resolve_program(&home, "/usr/bin/env"), "/usr/bin/env");
        // A relative path that does not exist is passed through, so the error
        // the caller sees is the shell's, not a rewritten one.
        assert_eq!(resolve_program(&home, "./nope/x"), "./nope/x");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
