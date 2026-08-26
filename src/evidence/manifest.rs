//! The bundle manifest: what is in the archive, and what it hashed to.
//!
//! A bundle without a manifest is a folder in a coat. The manifest is what lets
//! a reviewer who was not present establish that what they are reading is what
//! the run produced (PLAN.md G3).

use serde::{Deserialize, Serialize};

pub const MANIFEST_SCHEMA: &str = "keel.manifest/1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Member {
    /// Path inside the archive.
    pub path: String,
    pub bytes: u64,
    /// SHA-256 of the member's contents, lower-case hex.
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub run: String,
    pub spec: String,
    pub store_hash: String,
    pub keel_version: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    pub members: Vec<Member>,
}

impl Manifest {
    pub fn new(run: &crate::run::RunMeta, members: Vec<Member>) -> Self {
        Self {
            schema: MANIFEST_SCHEMA.to_string(),
            run: run.id.clone(),
            spec: run.spec.clone(),
            store_hash: run.store_hash.clone(),
            keel_version: run.keel_version.clone(),
            created_at: chrono::Local::now().to_rfc3339(),
            verdict: run.verdict.clone(),
            members,
        }
    }

    pub fn find(&self, path: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.path == path)
    }
}

/// The JSON Schema for a manifest, written to `.keel/schemas/manifest.json` so
/// the `schema` oracle kind has something real to validate against.
pub const JSON_SCHEMA: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "keel evidence bundle manifest",
  "type": "object",
  "required": ["schema", "run", "spec", "store_hash", "keel_version", "created_at", "members"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "keel.manifest/1" },
    "run": { "type": "string", "minLength": 1 },
    "spec": { "type": "string", "minLength": 1 },
    "store_hash": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
    "keel_version": { "type": "string", "minLength": 1 },
    "created_at": { "type": "string", "minLength": 1 },
    "verdict": { "type": "string" },
    "members": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "object",
        "required": ["path", "bytes", "sha256"],
        "additionalProperties": false,
        "properties": {
          "path": { "type": "string", "minLength": 1 },
          "bytes": { "type": "integer", "minimum": 0 },
          "sha256": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
        }
      }
    }
  }
}
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_schema_is_itself_valid_json_schema() {
        let v: serde_json::Value = serde_json::from_str(JSON_SCHEMA).expect("schema parses");
        jsonschema::validator_for(&v).expect("schema compiles");
    }

    #[test]
    fn a_real_manifest_validates_against_the_published_schema() {
        let schema: serde_json::Value = serde_json::from_str(JSON_SCHEMA).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let m = Manifest {
            schema: MANIFEST_SCHEMA.into(),
            run: "2026-08-21-abc".into(),
            spec: "demo".into(),
            store_hash: "a".repeat(64),
            keel_version: "0.1.0".into(),
            created_at: "2026-08-21T10:00:00Z".into(),
            verdict: Some("pass".into()),
            members: vec![Member { path: "run.json".into(), bytes: 12, sha256: "b".repeat(64) }],
        };
        let json = serde_json::to_value(&m).unwrap();
        let errors: Vec<String> = validator.iter_errors(&json).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn the_schema_rejects_a_truncated_hash() {
        let schema: serde_json::Value = serde_json::from_str(JSON_SCHEMA).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let bad = serde_json::json!({
            "schema": "keel.manifest/1", "run": "r", "spec": "s",
            "store_hash": "abc", "keel_version": "0.1.0", "created_at": "t",
            "members": [{ "path": "run.json", "bytes": 1, "sha256": "b" }]
        });
        assert!(validator.iter_errors(&bad).next().is_some(), "a short hash was accepted");
    }

    #[test]
    fn the_schema_rejects_an_empty_member_list() {
        let schema: serde_json::Value = serde_json::from_str(JSON_SCHEMA).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let bad = serde_json::json!({
            "schema": "keel.manifest/1", "run": "r", "spec": "s",
            "store_hash": "a".repeat(64), "keel_version": "0.1.0",
            "created_at": "t", "members": []
        });
        assert!(validator.iter_errors(&bad).next().is_some(), "an empty bundle was accepted");
    }
}
