//! The driver wire contract (`keel.drivertask/1` → `keel.driverresult/1`).
//!
//! keel delegates code generation (PLAN.md §1). The contract is deliberately
//! small: a task in on stdin, a result out on stdout. Drivers stay thin because
//! the moment a driver reimplements what the underlying CLI already does, keel
//! is competing with a product instead of conducting it.

use serde::{Deserialize, Serialize};

pub const TASK_SCHEMA: &str = "keel.drivertask/1";
pub const RESULT_SCHEMA: &str = "keel.driverresult/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverTask {
    pub schema: String,
    pub run: String,
    pub spec: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// The full instruction, including everything keel chose to inject.
    pub prompt: String,
    /// Globs the change is allowed to touch.
    pub scope: Vec<String>,
    /// Line budget for the diff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_lines: Option<usize>,
    pub repo: String,
}

impl DriverTask {
    pub fn new(run: &str, spec: &str, task: Option<String>, prompt: String, scope: Vec<String>, budget_lines: Option<usize>, repo: String) -> Self {
        Self {
            schema: TASK_SCHEMA.to_string(),
            run: run.to_string(),
            spec: spec.to_string(),
            task,
            prompt,
            scope,
            budget_lines,
            repo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriverStatus {
    /// The driver believes it completed the task.
    Ok,
    /// The driver ran and could not complete the task.
    Failed,
    /// The driver could not run at all. Never an agentic failure (P6).
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverResult {
    pub schema: String,
    pub status: DriverStatus,
    /// Paths the driver says it changed. keel verifies against the real diff
    /// rather than trusting this, but a mismatch is itself informative.
    #[serde(default)]
    pub files_changed: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<usize>,
}

impl DriverResult {
    pub fn blocked(detail: impl Into<String>) -> Self {
        Self {
            schema: RESULT_SCHEMA.to_string(),
            status: DriverStatus::Blocked,
            files_changed: vec![],
            detail: Some(detail.into()),
            tokens: None,
        }
    }

    pub fn status_str(&self) -> &'static str {
        match self.status {
            DriverStatus::Ok => "ok",
            DriverStatus::Failed => "failed",
            DriverStatus::Blocked => "blocked",
        }
    }
}

/// Parse a driver's stdout, naming the offending field on failure.
///
/// A driver that prints something almost-right is the common case, and "invalid
/// JSON" alone sends you reading the driver's source. serde already knows which
/// field it choked on; this just refuses to throw that away.
pub fn parse_result(stdout: &str) -> Result<DriverResult, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("driver printed nothing on stdout; expected a keel.driverresult/1 object".into());
    }
    // Drivers wrap their own logging around the payload often enough that
    // finding the object is worth doing rather than failing on a stray line.
    let candidate = last_json_object(trimmed).unwrap_or(trimmed);

    let value: serde_json::Value = serde_json::from_str(candidate)
        .map_err(|e| format!("stdout is not JSON: {e}"))?;

    match serde_json::from_value::<DriverResult>(value.clone()) {
        Ok(mut r) => {
            if r.schema != RESULT_SCHEMA {
                return Err(format!(
                    "field `schema`: expected `{RESULT_SCHEMA}`, found `{}`",
                    r.schema
                ));
            }
            r.schema = RESULT_SCHEMA.to_string();
            Ok(r)
        }
        Err(e) => {
            let field = missing_field(&e.to_string(), &value);
            Err(format!("field `{field}`: {e}"))
        }
    }
}

const FIELDS: &[&str] = &["schema", "status", "files_changed", "detail", "tokens"];

/// Work out which field serde choked on, so the caller can put it first — where
/// a human will actually read it.
///
/// serde names the field for a missing or mistyped one, but for a bad enum
/// value it names the *value* ("unknown variant `probably`") and leaves the
/// field implicit. Both cases have to be handled or the common one — a driver
/// inventing a status — reports `?`.
fn missing_field(msg: &str, value: &serde_json::Value) -> String {
    for key in FIELDS {
        if msg.contains(&format!("`{key}`")) {
            return key.to_string();
        }
    }
    // Bad enum value: find the field whose value serde is complaining about.
    if let Some(obj) = value.as_object() {
        for key in FIELDS {
            if let Some(serde_json::Value::String(v)) = obj.get(*key)
                && msg.contains(&format!("`{v}`"))
            {
                return key.to_string();
            }
        }
    }
    for key in FIELDS {
        if value.get(*key).is_none() {
            return key.to_string();
        }
    }
    "?".to_string()
}

/// The last `{...}` span in the text, so leading driver chatter is tolerated.
fn last_json_object(s: &str) -> Option<&str> {
    let start = s.rfind('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    // Scan forward from the earliest `{` that yields a balanced object ending
    // at the final `}`; the simple case (whole string is the object) hits first.
    for (i, c) in s.char_indices() {
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
                    let first = s[..=i].rfind('{').map(|_| s.find('{').unwrap_or(i));
                    let begin = first.unwrap_or(start);
                    return Some(&s[begin..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> String {
        serde_json::json!({
            "schema": RESULT_SCHEMA,
            "status": "ok",
            "files_changed": ["src/api.rs"],
            "tokens": 1200
        })
        .to_string()
    }

    #[test]
    fn parses_a_well_formed_result() {
        let r = parse_result(&good()).unwrap();
        assert_eq!(r.status, DriverStatus::Ok);
        assert_eq!(r.files_changed, vec!["src/api.rs"]);
        assert_eq!(r.tokens, Some(1200));
        assert_eq!(r.status_str(), "ok");
    }

    #[test]
    fn tolerates_driver_chatter_around_the_payload() {
        let noisy = format!("loading model…\nthinking\n{}\n", good());
        assert_eq!(parse_result(&noisy).unwrap().status, DriverStatus::Ok);
    }

    #[test]
    fn empty_stdout_is_rejected_clearly() {
        let err = parse_result("   ").unwrap_err();
        assert!(err.contains("printed nothing"), "{err}");
    }

    #[test]
    fn a_wrong_schema_names_the_schema_field() {
        let bad = good().replace(RESULT_SCHEMA, "keel.driverresult/99");
        let err = parse_result(&bad).unwrap_err();
        assert!(err.contains("field `schema`"), "{err}");
        assert!(err.contains("keel.driverresult/99"), "{err}");
    }

    #[test]
    fn a_missing_status_names_the_status_field() {
        let bad = serde_json::json!({ "schema": RESULT_SCHEMA }).to_string();
        let err = parse_result(&bad).unwrap_err();
        assert!(err.contains("field `status`"), "{err}");
    }

    #[test]
    fn an_unknown_status_value_names_the_status_field() {
        let bad = good().replace("\"ok\"", "\"probably\"");
        let err = parse_result(&bad).unwrap_err();
        assert!(err.contains("field `status`"), "{err}");
    }

    #[test]
    fn non_json_output_says_so() {
        let err = parse_result("Segmentation fault").unwrap_err();
        assert!(err.contains("not JSON"), "{err}");
    }

    #[test]
    fn a_task_round_trips() {
        let t = DriverTask::new("r1", "demo", Some("T-1".into()), "do it".into(),
            vec!["src/**".into()], Some(80), "/repo".into());
        let json = serde_json::to_string(&t).unwrap();
        let back: DriverTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema, TASK_SCHEMA);
        assert_eq!(back.task.as_deref(), Some("T-1"));
        assert_eq!(back.budget_lines, Some(80));
    }
}
