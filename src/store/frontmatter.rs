//! YAML front matter on store documents (PLAN.md §4.2).
//!
//! The machine fields are a fixed, small set; anything else a human writes is
//! preserved verbatim in `extra` so keel never eats a field it does not know.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrontMatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// `human` or `agent` — who is allowed to rewrite this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// True for files keel regenerates (structure.md, CODEMAPs).
    #[serde(default, skip_serializing_if = "is_false")]
    pub generated: bool,
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

fn is_false(b: &bool) -> bool { !*b }

/// Split a document into front matter of a caller-chosen shape, and body.
/// Specs and plans carry structured machine fields that the loose `FrontMatter`
/// above would swallow into `extra`.
pub fn split_typed<T: serde::de::DeserializeOwned>(raw: &str) -> Result<(T, String)> {
    let trimmed = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let Some(rest) = trimmed.strip_prefix("---\n") else {
        anyhow::bail!("missing front matter (the file must start with `---`)");
    };
    let Some(end) = find_terminator(rest) else {
        anyhow::bail!("front matter is never terminated (no closing `---`)");
    };
    let (yaml, body) = rest.split_at(end);
    let body = body.strip_prefix("---\n").or_else(|| body.strip_prefix("---")).unwrap_or(body);
    let front: T = serde_yaml::from_str(yaml).context("parsing front matter")?;
    Ok((front, body.trim_start_matches('\n').to_string()))
}

/// Split a document into front matter and body. A file with no front matter is
/// legal — it simply has default (empty) metadata.
pub fn split(raw: &str) -> Result<(FrontMatter, String)> {
    let trimmed = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let Some(rest) = trimmed.strip_prefix("---\n") else {
        return Ok((FrontMatter::default(), trimmed.to_string()));
    };
    let Some(end) = find_terminator(rest) else {
        return Ok((FrontMatter::default(), trimmed.to_string()));
    };
    let (yaml, body) = rest.split_at(end);
    let body = body
        .strip_prefix("---\n").or_else(|| body.strip_prefix("---"))
        .unwrap_or(body);
    let front: FrontMatter = if yaml.trim().is_empty() {
        FrontMatter::default()
    } else {
        serde_yaml::from_str(yaml).context("parsing front matter")?
    };
    Ok((front, body.trim_start_matches('\n').to_string()))
}

fn find_terminator(s: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in s.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Re-join typed front matter and body into a writable document.
pub fn join_typed<T: Serialize>(front: &T, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(front)?;
    Ok(format!("---\n{}---\n\n{}", yaml, body.trim_start_matches('\n')))
}

/// Re-join front matter and body into a writable document.
pub fn join(front: &FrontMatter, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(front)?;
    let yaml = if yaml.trim() == "{}" { String::new() } else { yaml };
    if yaml.trim().is_empty() {
        return Ok(body.to_string());
    }
    Ok(format!("---\n{}---\n\n{}", yaml, body.trim_start_matches('\n')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_known_fields() {
        let raw = "---\nid: CONV-0007\nscope: repo\nowner: human\nsources:\n  - runs/a3f\n---\n\n# Body\ntext\n";
        let (front, body) = split(raw).unwrap();
        assert_eq!(front.id.as_deref(), Some("CONV-0007"));
        assert_eq!(front.sources, vec!["runs/a3f".to_string()]);
        assert!(body.starts_with("# Body"));
        let again = join(&front, &body).unwrap();
        let (front2, body2) = split(&again).unwrap();
        assert_eq!(front2.id, front.id);
        assert_eq!(body2, body);
    }

    #[test]
    fn preserves_unknown_fields() {
        let (front, _) = split("---\nid: X\nweird_field: 3\n---\nbody\n").unwrap();
        assert!(front.extra.contains_key(serde_yaml::Value::String("weird_field".into())));
        let out = join(&front, "body\n").unwrap();
        assert!(out.contains("weird_field"));
    }

    #[test]
    fn missing_front_matter_is_legal() {
        let (front, body) = split("# Just markdown\n").unwrap();
        assert!(front.id.is_none());
        assert_eq!(body, "# Just markdown\n");
    }

    #[test]
    fn unterminated_front_matter_is_body() {
        let (front, body) = split("---\nid: X\nno terminator\n").unwrap();
        assert!(front.id.is_none());
        assert!(body.starts_with("---"));
    }
}
