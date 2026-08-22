//! Running a task against somebody else's coding agent.
//!
//! One function owns the whole subprocess lifecycle — stdin, stdout, timeout,
//! termination — so adding a driver is a config entry, never new process code.
//!
//! The `blocked` discipline from P6 is enforced here: a driver that cannot
//! start, or that runs out of time, is `blocked`, not `failed`. Only a driver
//! that ran and could not do the job is an agentic failure. Getting this wrong
//! teaches the Phase 3 failure taxonomy to learn from noise.

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
    let started = Instant::now();
    let timeout = Duration::from_secs(driver.timeout_secs);

    let mut parts = driver.cmd.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>();
    if parts.is_empty() {
        return blocked(started, format!("driver `{}` has an empty cmd", driver.id));
    }
    let program = parts.remove(0);

    let payload = match serde_json::to_string(task) {
        Ok(p) => p,
        Err(e) => return blocked(started, format!("could not serialise the task: {e}")),
    };

    let mut command = Command::new(&program);
    command
        .args(&parts)
        .current_dir(&paths.repo)
        .env("KEEL_REPO", &paths.repo)
        .env("KEEL_STORE", paths.store())
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

    // Drain both pipes on their own threads. A driver that fills the 64KB pipe
    // buffer deadlocks against a parent that waits before reading.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_all(&mut stdout_pipe));
    let stderr_reader = std::thread::spawn(move || read_all(&mut stderr_pipe));

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

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default().trim().to_string();

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
    let _ = Command::new("kill").args(["-KILL", &format!("-{pid}")]).output();
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_group(child: &mut std::process::Child, pid: u32) {
    // No process groups: taskkill's tree flag is the equivalent, scoped to this
    // pid rather than to a name.
    let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output();
    let _ = child.kill();
}

fn blocked(started: Instant, detail: String) -> Invocation {
    Invocation {
        result: DriverResult::blocked(detail),
        elapsed: started.elapsed(),
        stderr: String::new(),
    }
}

/// The driver to use: the named one, else the configured default.
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
