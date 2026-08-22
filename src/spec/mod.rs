//! The spec artefact: `.keel/specs/<slug>/spec.md`.
//!
//! A spec is markdown a human can read and a parser can check. The machine
//! fields live in front matter; the acceptance criteria live in `###` blocks
//! that each carry an EARS statement and at least one `oracle:` line.

pub mod ears;
pub mod oracle;
pub mod placeholder;

use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use oracle::Oracle;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SPEC_SCHEMA: &str = "keel.spec/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecFront {
    pub id: String,
    pub slug: String,
    #[serde(default = "default_spec_schema")]
    pub schema: String,
    #[serde(default = "default_status")]
    pub status: String,
    /// Declared blast-radius intent: the globs this change is allowed to touch.
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub budget: SpecBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

fn default_spec_schema() -> String { SPEC_SCHEMA.to_string() }
fn default_status() -> String { "draft".to_string() }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecBudget {
    /// Max criteria the author intends. Checked against config's hard ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<usize>,
    /// Max lines of production diff this change should cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Criterion {
    pub id: String,
    pub title: String,
    /// The EARS sentence, whitespace-normalised.
    pub statement: String,
    pub oracles: Vec<Oracle>,
    /// Oracle lines that failed to parse, with the reason.
    pub bad_oracles: Vec<(String, String)>,
    /// 1-indexed line of the criterion heading, for locatable gate failures.
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Spec {
    pub front: SpecFront,
    pub criteria: Vec<Criterion>,
    pub lines: usize,
}

impl Spec {
    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        Self::parse(path, &raw)
    }

    pub fn parse(path: &Path, raw: &str) -> Result<Self> {
        let (front, body): (SpecFront, String) = crate::store::frontmatter::split_typed(raw)
            .with_context(|| format!("in {}", path.display()))?;
        let front_lines = raw.lines().count() - body.lines().count();
        let criteria = parse_criteria(&body, front_lines);
        let _ = path;
        Ok(Self { front, criteria, lines: raw.lines().count() })
    }

    pub fn dir(paths: &Paths, slug: &str) -> PathBuf {
        paths.specs().join(slug)
    }

    pub fn path_for(paths: &Paths, slug: &str) -> PathBuf {
        Self::dir(paths, slug).join("spec.md")
    }

    pub fn load(paths: &Paths, slug: &str) -> Result<Self> {
        let p = Self::path_for(paths, slug);
        if !p.exists() {
            bail!("no spec at {} — run `keel spec new {slug}`", paths.rel(&p).display());
        }
        Self::read(&p)
    }

    /// Criteria whose only oracle is human judgement — the visible human cost.
    pub fn human_only_criteria(&self) -> Vec<&Criterion> {
        self.criteria
            .iter()
            .filter(|c| !c.oracles.is_empty() && c.oracles.iter().all(|o| o.is_human()))
            .collect()
    }
}

/// Every spec slug present on disk, sorted.
pub fn list(paths: &Paths) -> Result<Vec<String>> {
    let dir = paths.specs();
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut out: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("spec.md").is_file())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect();
    out.sort();
    Ok(out)
}

/// A criterion block is a `###` heading whose text begins with an identifier
/// like `AC-3`. Everything until the next heading belongs to it.
fn parse_criteria(body: &str, line_offset: usize) -> Vec<Criterion> {
    let mut out: Vec<Criterion> = Vec::new();
    let mut current: Option<Criterion> = None;
    let mut statement_lines: Vec<String> = Vec::new();
    let mut in_fence = false;

    let flush = |cur: &mut Option<Criterion>, stmt: &mut Vec<String>, out: &mut Vec<Criterion>| {
        if let Some(mut c) = cur.take() {
            c.statement = normalise(&stmt.join(" "));
            out.push(c);
        }
        stmt.clear();
    };

    for (i, line) in body.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        if let Some(heading) = trimmed.strip_prefix("### ") {
            flush(&mut current, &mut statement_lines, &mut out);
            if let Some((id, title)) = split_criterion_heading(heading) {
                current = Some(Criterion {
                    id,
                    title,
                    statement: String::new(),
                    oracles: Vec::new(),
                    bad_oracles: Vec::new(),
                    line: line_offset + i + 1,
                });
            }
            continue;
        }
        // Any other heading closes the criteria region.
        if trimmed.starts_with("# ") || trimmed.starts_with("## ") {
            flush(&mut current, &mut statement_lines, &mut out);
            continue;
        }

        let Some(c) = current.as_mut() else { continue };

        let oracle_text = trimmed
            .strip_prefix("oracle:")
            .or_else(|| trimmed.strip_prefix("- oracle:"))
            .or_else(|| trimmed.strip_prefix("* oracle:"));
        if let Some(text) = oracle_text {
            match oracle::parse(text) {
                Ok(o) => c.oracles.push(o),
                Err(e) => c.bad_oracles.push((text.trim().to_string(), e.to_string())),
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        statement_lines.push(trimmed.to_string());
    }
    flush(&mut current, &mut statement_lines, &mut out);
    out
}

/// `AC-3 Requests over the limit are rejected` → (`AC-3`, the title).
fn split_criterion_heading(heading: &str) -> Option<(String, String)> {
    let h = heading.trim();
    let (id, rest) = h.split_once(char::is_whitespace).unwrap_or((h, ""));
    let id = id.trim_end_matches([':', '.']);
    let looks_like_id = id.len() >= 3
        && id.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && id.contains('-')
        && id.chars().last().is_some_and(|c| c.is_ascii_digit());
    if !looks_like_id {
        return None;
    }
    Some((id.to_string(), rest.trim().to_string()))
}

fn normalise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        let space = c.is_whitespace();
        if space {
            if !prev_space { out.push(' '); }
        } else {
            out.push(c);
        }
        prev_space = space;
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"---
id: SPEC-0001
slug: rate-limit
scope:
  - src/api/**
budget:
  criteria: 6
  lines: 200
---

# Rate limiting

## Context

Some prose that mentions `handle` and should not be parsed as a criterion.

## Acceptance criteria

### AC-1 Requests over the limit are rejected

WHEN a client exceeds 100 requests per minute
THE SYSTEM SHALL respond with HTTP 429.

oracle: cmd `cargo test --test rate_limit over_limit` exit 0

### AC-2 The limit is configurable

WHERE `rate_limit.rpm` is set THE SYSTEM SHALL use that value.
oracle: test tests/rate_limit.rs::respects_config
oracle: human reviewer confirms the default is documented

## Notes

Trailing prose after the criteria.
"#;

    fn spec() -> Spec {
        Spec::parse(Path::new("spec.md"), SPEC).unwrap()
    }

    #[test]
    fn parses_front_matter() {
        let s = spec();
        assert_eq!(s.front.id, "SPEC-0001");
        assert_eq!(s.front.slug, "rate-limit");
        assert_eq!(s.front.schema, SPEC_SCHEMA, "schema should default");
        assert_eq!(s.front.status, "draft");
        assert_eq!(s.front.scope, vec!["src/api/**".to_string()]);
        assert_eq!(s.front.budget.criteria, Some(6));
    }

    #[test]
    fn finds_exactly_the_criteria() {
        let s = spec();
        let ids: Vec<&str> = s.criteria.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["AC-1", "AC-2"]);
    }

    #[test]
    fn joins_multiline_statements() {
        let s = spec();
        assert_eq!(
            s.criteria[0].statement,
            "WHEN a client exceeds 100 requests per minute THE SYSTEM SHALL respond with HTTP 429."
        );
    }

    #[test]
    fn collects_all_oracles_for_a_criterion() {
        let s = spec();
        assert_eq!(s.criteria[0].oracles.len(), 1);
        assert_eq!(s.criteria[1].oracles.len(), 2);
        assert!(s.criteria[1].oracles.iter().any(|o| o.is_human()));
    }

    #[test]
    fn prose_sections_are_not_criteria() {
        let s = spec();
        assert!(!s.criteria.iter().any(|c| c.title.contains("Context")));
        assert!(!s.criteria.iter().any(|c| c.statement.contains("Trailing prose")));
    }

    #[test]
    fn a_criterion_with_only_human_oracles_is_flagged_as_human_cost() {
        let raw = SPEC.replace("oracle: test tests/rate_limit.rs::respects_config\n", "");
        let s = Spec::parse(Path::new("spec.md"), &raw).unwrap();
        let human = s.human_only_criteria();
        assert_eq!(human.len(), 1);
        assert_eq!(human[0].id, "AC-2");
    }

    #[test]
    fn malformed_oracles_are_captured_not_dropped() {
        let raw = SPEC.replace("oracle: cmd `cargo test --test rate_limit over_limit` exit 0",
                               "oracle: vibes it feels right");
        let s = Spec::parse(Path::new("spec.md"), &raw).unwrap();
        assert!(s.criteria[0].oracles.is_empty());
        assert_eq!(s.criteria[0].bad_oracles.len(), 1);
        assert!(s.criteria[0].bad_oracles[0].1.contains("unknown oracle kind"));
    }

    #[test]
    fn criteria_carry_locatable_line_numbers() {
        let s = spec();
        let at = s.criteria[0].line;
        let line = SPEC.lines().nth(at - 1).unwrap();
        assert!(line.contains("AC-1"), "line {at} was `{line}`");
    }

    #[test]
    fn fenced_code_is_not_scanned_for_criteria() {
        let raw = SPEC.replace(
            "## Notes",
            "## Notes\n\n```md\n### AC-9 Not a real criterion\n```\n",
        );
        let s = Spec::parse(Path::new("spec.md"), &raw).unwrap();
        assert!(!s.criteria.iter().any(|c| c.id == "AC-9"));
    }

    #[test]
    fn a_spec_without_front_matter_is_an_error_not_a_silent_pass() {
        let err = format!("{:#}", Spec::parse(Path::new("spec.md"), "# Just markdown\n").unwrap_err());
        assert!(err.contains("front matter"), "{err}");
    }
}
