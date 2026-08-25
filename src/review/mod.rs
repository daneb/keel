//! The adversarial reviewer (PLAN.md G2.5).
//!
//! G2.5 shipped with substring heuristics: a list of mocking vocabulary and a
//! count of which files changed. That catches the obvious cases and misses
//! everything that requires reading the diff — which is most of what an
//! adversarial pass is for.
//!
//! This is the intended implementation: a second agent pass, run in critique
//! mode against the conventions and the lessons in force, returning structured
//! findings that become gate checks. It reuses the driver subprocess pattern so
//! a reviewer is a config entry, not new process code.
//!
//! The heuristics stay. They are cheap, they need no agent, and a reviewer that
//! is not configured must not silently remove the only check there was.

use crate::config::Reviewer;
use crate::paths::Paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const REQUEST_SCHEMA: &str = "keel.reviewrequest/1";
pub const RESULT_SCHEMA: &str = "keel.reviewresult/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub schema: String,
    pub run: String,
    pub spec: String,
    /// The unified diff under review.
    pub diff: String,
    /// House rules the change must not breach.
    pub conventions: String,
    /// Lessons in force, as rules.
    pub lessons: Vec<String>,
    /// What the change was supposed to do.
    pub criteria: Vec<String>,
    /// What the reviewer is being asked to look for.
    pub prompt: String,
    pub repo: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A defect: this should not merge.
    Fail,
    /// Worth a human look, not a refusal.
    Concern,
}

/// How bad a finding is, separately from whether it blocks.
///
/// `severity` is policy — does this stop the gate. `grade` is assessment — how
/// dangerous is it. They are different axes and conflating them is why security
/// findings are usually either all-blocking or all-advisory: a hardcoded
/// credential and a missing `# nosec` comment are not the same thing, and a
/// single fail/concern flag cannot say so.
///
/// Added rather than folded into `Severity` because `keel.reviewresult/1` is
/// frozen and additive-only. An optional field a reviewer may omit is additive;
/// new variants on an existing enum would break every reviewer that does not
/// know them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Low,
    Medium,
    High,
    Critical,
}

impl Grade {
    /// Grades that fail a gate rather than merely being recorded.
    pub fn blocks(&self) -> bool {
        matches!(self, Grade::High | Grade::Critical)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Grade::Low => "LOW",
            Grade::Medium => "MEDIUM",
            Grade::High => "HIGH",
            Grade::Critical => "CRITICAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Short kebab-case category, e.g. `test-invalidation`.
    pub id: String,
    pub severity: Severity,
    /// How dangerous, where the reviewer graded it. Absent on findings that are
    /// not security-relevant, which is most of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<Grade>,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

impl Finding {
    pub fn where_(&self) -> String {
        match (&self.file, self.line) {
            (Some(f), Some(l)) => format!("{f}:{l}"),
            (Some(f), None) => f.clone(),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub schema: String,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// What the reviewer is asked to do.
///
/// Named categories on purpose: an open-ended "review this" produces style
/// opinions, and PLAN.md is specific about what G2.5 is for — test-invalidation
/// and scope creep, the two things a green G2 cannot see.
pub const REVIEW_PROMPT: &str = "\
You are reviewing a diff adversarially. Report only defects you can point at in \
the diff; do not report style preferences, and do not restate what the change \
does.

Look specifically for:
  test-invalidation — a test weakened, mocked, skipped or deleted so that it \
passes without exercising the behaviour it names. This is the most dangerous \
finding: the suite is green and the code is wrong.
  scope-creep — changes that are not needed for the stated criteria.
  convention-breach — a violation of the house rules or a lesson listed below.
  missing-coverage — a stated criterion with no corresponding test change.

Then review the same diff for SECURITY defects, which are graded separately. \
Judge only the added and modified lines — pre-existing weaknesses are not this \
change's findings, and reporting them buries the ones that are. Categories:
  injection — untrusted input reaching a shell, SQL, a template, a path, or a \
deserialiser without validation or parameterisation.
  authz — a check that is missing, applied after the effect, or trivially \
bypassable.
  crypto — home-rolled cryptography, a weak or fixed algorithm, a predictable \
seed, a hardcoded key or IV.
  secret-exposure — a credential in source, in a log line, in an error message, \
or written to a file the repository commits.
  unsafe-input — a parser or decoder handed untrusted bytes with no bound on \
size, depth, or recursion.
  resource-exhaustion — an unbounded allocation, read, or loop driven by input \
the caller controls.

Reply with one JSON object and nothing else:
{\"schema\":\"keel.reviewresult/1\",\"findings\":[{\"id\":\"injection\",\
\"severity\":\"fail\",\"grade\":\"high\",\"detail\":\"<what and why>\",\
\"file\":\"<path>\",\"line\":<n>}],\"summary\":\"<one sentence>\"}

severity is \"fail\" for a defect that should block, \"concern\" for something a \
human should look at.

grade is required on every security finding and omitted on the others. Use \
\"critical\" for a defect that is exploitable as written, \"high\" for one \
exploitable given a reachable caller, \"medium\" where exploitation needs a \
condition not shown in the diff, \"low\" for hardening. Grade what the diff \
shows, not what you imagine around it: an inflated grade spends a human's \
attention, and spending it twice on nothing is how the check gets turned off.

An empty findings list is a valid and common answer.";

pub struct Review {
    pub result: ReviewResult,
    pub elapsed: Duration,
    /// Present when the reviewer could not run — a blocked check, not findings.
    pub blocked: Option<String>,
}

impl Review {
    fn blocked(started: Instant, why: String) -> Self {
        Self {
            result: ReviewResult { schema: RESULT_SCHEMA.into(), findings: vec![], summary: None },
            elapsed: started.elapsed(),
            blocked: Some(why),
        }
    }
}

/// Run the configured reviewer over a diff.
///
/// Like the driver, this never returns `Err` for anything the reviewer did: a
/// reviewer that cannot run is `blocked`, which is a fact about the
/// environment, not a finding about the change.
pub fn run(paths: &Paths, reviewer: &Reviewer, request: &ReviewRequest) -> Review {
    let started = Instant::now();
    let mut parts = reviewer.cmd.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>();
    if parts.is_empty() {
        return Review::blocked(started, "reviewer has an empty cmd".into());
    }
    let program = parts.remove(0);

    let payload = match serde_json::to_string(request) {
        Ok(p) => p,
        Err(e) => return Review::blocked(started, format!("could not serialise the request: {e}")),
    };

    let mut command = Command::new(&program);
    command
        .args(&parts)
        .current_dir(&paths.repo)
        .env("KEEL_REPO", &paths.repo)
        .env("KEEL_RUN", &request.run)
        .env("KEEL_SPEC", &request.spec)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return Review::blocked(started, format!("could not start `{}`: {e}", reviewer.cmd)),
    };
    let pid = child.id();

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.as_bytes());
    }

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || read_all(&mut stdout_pipe));
    let err_reader = std::thread::spawn(move || read_all(&mut stderr_pipe));

    let deadline = started + Duration::from_secs(reviewer.timeout_secs);
    let mut timed_out = false;
    let mut status = None;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => {
                status = Some(s);
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    kill_group(&mut child, pid);
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Review::blocked(started, format!("could not wait on the reviewer: {e}")),
        }
    }

    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if timed_out {
        return Review::blocked(
            started,
            format!("reviewer exceeded its {}s timeout", reviewer.timeout_secs),
        );
    }

    // A reviewer that exits non-zero is saying it could not do its job, and
    // that is `blocked` — the same discipline the driver contract has. This was
    // previously ignored, which meant an adapter reporting "the scanner is not
    // installed" alongside an empty findings list read as a clean pass. A check
    // that cannot run must never be indistinguishable from one that ran and
    // found nothing.
    //
    // A reviewer with findings reports them and exits 0; findings are not an
    // error condition for the reviewer, they are its output.
    if let Some(s) = status
        && !s.success()
    {
        let code = s.code().map(|c| c.to_string()).unwrap_or_else(|| "a signal".into());
        let why = parse_result(&stdout)
            .ok()
            .and_then(|r| r.summary)
            .unwrap_or_else(|| format!("exited with {code}"));
        return Review::blocked(started, match stderr.trim() {
            "" => why,
            e => format!("{why}; stderr: {}", truncate(e, 200)),
        });
    }

    match parse_result(&stdout) {
        Ok(result) => Review { result, elapsed: started.elapsed(), blocked: None },
        Err(why) => Review::blocked(
            started,
            format!("{why}{}", if stderr.trim().is_empty() {
                String::new()
            } else {
                format!("; stderr: {}", truncate(stderr.trim(), 200))
            }),
        ),
    }
}

/// Parse a reviewer's stdout, tolerating chatter around the payload.
pub fn parse_result(stdout: &str) -> Result<ReviewResult, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("reviewer printed nothing on stdout".into());
    }
    let candidate = json_object(trimmed).unwrap_or(trimmed);
    let value: serde_json::Value =
        serde_json::from_str(candidate).map_err(|e| format!("stdout is not JSON: {e}"))?;

    let mut result: ReviewResult = serde_json::from_value(value)
        .map_err(|e| format!("not a keel.reviewresult/1 object: {e}"))?;
    if result.schema != RESULT_SCHEMA {
        return Err(format!(
            "field `schema`: expected `{RESULT_SCHEMA}`, found `{}`",
            result.schema
        ));
    }
    // A finding with no detail is unactionable; drop it rather than fail a gate
    // on a blank.
    result.findings.retain(|f| !f.detail.trim().is_empty() && !f.id.trim().is_empty());
    Ok(result)
}

/// The outermost balanced `{...}` span.
fn json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < start) {
        if in_string {
            if escaped { escaped = false; }
            else if c == '\\' { escaped = true; }
            else if c == '"' { in_string = false; }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn read_all<R: std::io::Read>(pipe: &mut Option<R>) -> String {
    let Some(p) = pipe.as_mut() else { return String::new() };
    let mut buf = Vec::new();
    let _ = p.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).to_string()
}

#[cfg(unix)]
fn kill_group(child: &mut std::process::Child, pid: u32) {
    let _ = Command::new("kill").args(["-KILL", &format!("-{pid}")]).output();
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_group(child: &mut std::process::Child, pid: u32) {
    let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()]).output();
    let _ = child.kill();
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    s.chars().take(max - 1).chain(['…']).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> String {
        serde_json::json!({
            "schema": RESULT_SCHEMA,
            "findings": [{
                "id": "test-invalidation",
                "severity": "fail",
                "detail": "respects_config now asserts true instead of the limit",
                "file": "tests/limit.rs",
                "line": 4
            }],
            "summary": "one weakened test"
        })
        .to_string()
    }

    /// A scanner that cannot run must not be mistaken for a clean scan.
    #[test]
    fn a_reviewer_that_exits_non_zero_is_blocked_not_clean() {
        let dir = std::env::temp_dir().join(format!("keel-rev-exit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("cannot-run");
        std::fs::write(
            &script,
            "#!/bin/sh\ncat > /dev/null\n\
             echo '{\"schema\":\"keel.reviewresult/1\",\"findings\":[],\
             \"summary\":\"semgrep is not installed\"}'\nexit 3\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let paths = Paths { repo: dir.clone() };
        let reviewer = Reviewer {
            id: "sast".into(),
            cmd: script.to_string_lossy().to_string(),
            timeout_secs: 30,
            advisory: false,
        };
        let req = ReviewRequest {
            schema: REQUEST_SCHEMA.into(),
            run: "r".into(),
            spec: "s".into(),
            diff: "--- a/x\n+++ b/x\n".into(),
            conventions: String::new(),
            lessons: vec![],
            criteria: vec![],
            prompt: String::new(),
            repo: dir.to_string_lossy().to_string(),
        };

        let r = run(&paths, &reviewer, &req);
        let why = r.blocked.expect("a non-zero exit must block, not read as a clean scan");
        // The adapter's own summary explains why, rather than a bare exit code.
        assert!(why.contains("not installed"), "{why}");
        assert!(r.result.findings.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_graded_security_finding_round_trips() {
        let raw = serde_json::json!({
            "schema": RESULT_SCHEMA,
            "findings": [{
                "id": "injection",
                "severity": "fail",
                "grade": "high",
                "detail": "task.repo is interpolated into a shell string",
                "file": "src/driver/mod.rs",
                "line": 73
            }],
            "summary": "one injection"
        })
        .to_string();
        let r = parse_result(&raw).unwrap();
        assert_eq!(r.findings[0].grade, Some(Grade::High));
        assert!(r.findings[0].grade.unwrap().blocks());
    }

    #[test]
    fn grade_is_optional_so_older_reviewers_still_parse() {
        // `grade` was added to a frozen schema. A reviewer that has never heard
        // of it must keep working, or the freeze meant nothing.
        let r = parse_result(&good()).unwrap();
        assert_eq!(r.findings[0].grade, None);
    }

    #[test]
    fn only_high_and_critical_block() {
        assert!(!Grade::Low.blocks());
        assert!(!Grade::Medium.blocks());
        assert!(Grade::High.blocks());
        assert!(Grade::Critical.blocks());
        // Ordered, so a caller can ask for "at least high".
        assert!(Grade::Critical > Grade::High);
        assert!(Grade::High > Grade::Medium);
    }

    #[test]
    fn parses_a_well_formed_review() {
        let r = parse_result(&good()).unwrap();
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].severity, Severity::Fail);
        assert_eq!(r.findings[0].where_(), "tests/limit.rs:4");
    }

    #[test]
    fn an_empty_findings_list_is_a_valid_answer() {
        let clean = serde_json::json!({ "schema": RESULT_SCHEMA, "findings": [] }).to_string();
        assert!(parse_result(&clean).unwrap().findings.is_empty());
    }

    #[test]
    fn tolerates_chatter_around_the_payload() {
        let noisy = format!("thinking…\n{}\ndone\n", good());
        assert_eq!(parse_result(&noisy).unwrap().findings.len(), 1);
    }

    #[test]
    fn a_wrong_schema_names_the_field() {
        let bad = good().replace(RESULT_SCHEMA, "keel.reviewresult/99");
        let err = parse_result(&bad).unwrap_err();
        assert!(err.contains("field `schema`"), "{err}");
    }

    #[test]
    fn findings_with_no_detail_are_dropped_not_fatal() {
        let blank = serde_json::json!({
            "schema": RESULT_SCHEMA,
            "findings": [
                { "id": "x", "severity": "fail", "detail": "   " },
                { "id": "real", "severity": "concern", "detail": "something" }
            ]
        })
        .to_string();
        let r = parse_result(&blank).unwrap();
        assert_eq!(r.findings.len(), 1, "a blank finding survived");
        assert_eq!(r.findings[0].id, "real");
    }

    #[test]
    fn empty_stdout_is_rejected() {
        assert!(parse_result("  ").unwrap_err().contains("printed nothing"));
    }

    #[test]
    fn the_prompt_names_the_categories_g25_exists_for() {
        for c in ["test-invalidation", "scope-creep", "convention-breach", "missing-coverage"] {
            assert!(REVIEW_PROMPT.contains(c), "the prompt does not ask for {c}");
        }
        assert!(REVIEW_PROMPT.contains(RESULT_SCHEMA));
    }
}
