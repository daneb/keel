//! Executing oracles (PLAN.md P3).
//!
//! > Do not write a beautiful spec and expect the agent to honour it. Compile
//! > the spec down into something that runs.
//!
//! This is where that happens. Each oracle kind maps to something executable;
//! `human` maps to nothing, on purpose, and is reported as an outstanding human
//! obligation rather than quietly counted as a pass.

use crate::config::Config;
use crate::paths::Paths;
use crate::spec::oracle::Oracle;
use crate::spec::{Criterion, Spec};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Pass,
    Fail,
    /// Could not be executed — a missing tool, an unreadable file.
    Blocked,
    /// Awaits a person. Never a pass, never an agentic failure.
    Human,
}

impl Outcome {
    pub fn glyph(&self) -> &'static str {
        match self {
            Outcome::Pass => "pass",
            Outcome::Fail => "FAIL",
            Outcome::Blocked => "BLOCKED",
            Outcome::Human => "human",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OracleRun {
    pub criterion: String,
    pub kind: &'static str,
    pub oracle: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Trimmed output, kept as evidence.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Coverage {
    pub runs: Vec<OracleRun>,
    pub criteria: usize,
    pub executed: usize,
    pub passed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub human: usize,
}

impl Coverage {
    /// Criteria with no passing oracle and no outstanding human judgement.
    pub fn unsatisfied(&self) -> Vec<&OracleRun> {
        self.runs.iter().filter(|r| r.outcome == Outcome::Fail).collect()
    }
}

/// Execute every oracle in a spec.
pub fn run_all(paths: &Paths, cfg: &Config, spec: &Spec) -> Coverage {
    let mut runs = Vec::new();
    for c in &spec.criteria {
        if c.oracles.is_empty() {
            runs.push(OracleRun {
                criterion: c.id.clone(),
                kind: "none",
                oracle: "(no oracle)".into(),
                outcome: Outcome::Fail,
                exit_code: None,
                detail: Some("criterion has no oracle — G0 should have caught this".into()),
                output: String::new(),
            });
            continue;
        }
        for o in &c.oracles {
            runs.push(execute(paths, cfg, c, o));
        }
    }

    let count = |o: Outcome| runs.iter().filter(|r| r.outcome == o).count();
    Coverage {
        criteria: spec.criteria.len(),
        executed: runs.iter().filter(|r| r.outcome != Outcome::Human).count(),
        passed: count(Outcome::Pass),
        failed: count(Outcome::Fail),
        blocked: count(Outcome::Blocked),
        human: count(Outcome::Human),
        runs,
    }
}

/// Execute an oracle that belongs to a lesson rather than a criterion.
pub fn execute_standalone(paths: &Paths, cfg: &Config, o: &Oracle) -> OracleRun {
    let synthetic = Criterion {
        id: "lesson".into(),
        title: String::new(),
        statement: String::new(),
        oracles: vec![],
        bad_oracles: vec![],
        line: 0,
    };
    execute(paths, cfg, &synthetic, o)
}

fn execute(paths: &Paths, cfg: &Config, c: &Criterion, o: &Oracle) -> OracleRun {
    let base = OracleRun {
        criterion: c.id.clone(),
        kind: o.kind(),
        oracle: o.summary(),
        outcome: Outcome::Blocked,
        exit_code: None,
        detail: None,
        output: String::new(),
    };

    match o {
        Oracle::Human { judgement } => OracleRun {
            outcome: Outcome::Human,
            detail: Some(judgement.clone()),
            ..base
        },
        Oracle::Cmd { cmd, exit } => {
            let (code, output) = shell(paths, cmd);
            match code {
                Some(actual) if actual == *exit => OracleRun {
                    outcome: Outcome::Pass, exit_code: Some(actual), output, ..base
                },
                Some(actual) => OracleRun {
                    outcome: Outcome::Fail,
                    exit_code: Some(actual),
                    detail: Some(format!("expected exit {exit}, got {actual}")),
                    output,
                    ..base
                },
                None => OracleRun {
                    outcome: Outcome::Blocked,
                    detail: Some("command could not be executed".into()),
                    output,
                    ..base
                },
            }
        }
        Oracle::Test { id } => {
            let cmd = expand_test_template(&cfg.oracle.test_cmd, id);
            let (code, output) = shell(paths, &cmd);
            classify_zero(base, code, output, &cmd)
        }
        Oracle::Doctest { path } => {
            let cmd = cfg.oracle.doctest_cmd.replace("{path}", path);
            let (code, output) = shell(paths, &cmd);
            classify_zero(base, code, output, &cmd)
        }
        Oracle::Schema { schema, target } => validate_schema(paths, base, schema, target),
    }
}

fn classify_zero(base: OracleRun, code: Option<i32>, output: String, cmd: &str) -> OracleRun {
    match code {
        Some(0) => OracleRun { outcome: Outcome::Pass, exit_code: Some(0), output, ..base },
        Some(n) => OracleRun {
            outcome: Outcome::Fail,
            exit_code: Some(n),
            detail: Some(format!("`{cmd}` exited {n}")),
            output,
            ..base
        },
        None => OracleRun {
            outcome: Outcome::Blocked,
            detail: Some(format!("`{cmd}` could not be executed")),
            output,
            ..base
        },
    }
}

/// `tests/trajectory.rs::one_json_object_per_line` → `{file}` = `trajectory`,
/// `{name}` = `one_json_object_per_line`.
pub fn expand_test_template(template: &str, id: &str) -> String {
    let (path, name) = id.split_once("::").unwrap_or(("", id));
    let file = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    template
        .replace("{id}", id)
        .replace("{file}", &file)
        .replace("{name}", name)
        .replace("{path}", path)
}

/// Run a command through the shell so oracles can use pipes and substitution,
/// which is how anybody actually writes a check.
fn shell(paths: &Paths, cmd: &str) -> (Option<i32>, String) {
    let shell_bin = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };
    let out = std::process::Command::new(shell_bin)
        .arg(flag)
        .arg(cmd)
        .current_dir(&paths.repo)
        .output();
    match out {
        Ok(o) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            (o.status.code(), tail(&text, 4000))
        }
        Err(e) => (None, e.to_string()),
    }
}

fn validate_schema(paths: &Paths, base: OracleRun, schema: &str, target: &str) -> OracleRun {
    let schema_path = paths.repo.join(schema);
    let target_path = paths.repo.join(target);

    let read = |p: &Path| -> Result<serde_json::Value, String> {
        let raw = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", p.display()))
    };

    let schema_json = match read(&schema_path) {
        Ok(v) => v,
        Err(e) => return OracleRun { outcome: Outcome::Blocked, detail: Some(format!("schema unreadable — {e}")), ..base },
    };
    let target_json = match read(&target_path) {
        Ok(v) => v,
        Err(e) => return OracleRun { outcome: Outcome::Blocked, detail: Some(format!("target unreadable — {e}")), ..base },
    };

    let validator = match jsonschema::validator_for(&schema_json) {
        Ok(v) => v,
        Err(e) => return OracleRun { outcome: Outcome::Blocked, detail: Some(format!("schema is invalid — {e}")), ..base },
    };

    let errors: Vec<String> = validator
        .iter_errors(&target_json)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .take(10)
        .collect();

    if errors.is_empty() {
        OracleRun { outcome: Outcome::Pass, detail: Some(format!("{target} validates")), ..base }
    } else {
        OracleRun {
            outcome: Outcome::Fail,
            detail: Some(format!("{} did not validate", target)),
            output: errors.join("\n"),
            ..base
        }
    }
}

fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.trim_end().to_string();
    }
    let start = s.len() - max;
    format!("…\n{}", &s[start..].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_templates_expand_file_and_name() {
        let out = expand_test_template(
            "cargo test --test {file} -- --exact {name}",
            "tests/trajectory.rs::one_json_object_per_line",
        );
        assert_eq!(out, "cargo test --test trajectory -- --exact one_json_object_per_line");
    }

    #[test]
    fn a_bare_test_name_still_expands() {
        assert_eq!(expand_test_template("cargo test {name}", "my_test"), "cargo test my_test");
    }

    #[test]
    fn outcomes_are_distinguishable() {
        assert_ne!(Outcome::Blocked.glyph(), Outcome::Fail.glyph());
        assert_ne!(Outcome::Human.glyph(), Outcome::Pass.glyph());
    }

    #[test]
    fn tail_keeps_the_end_of_long_output() {
        let long = "x".repeat(5000) + "IMPORTANT";
        let t = tail(&long, 100);
        assert!(t.contains("IMPORTANT"), "the informative end was trimmed");
        assert!(t.len() < 200);
    }
}
