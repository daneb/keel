//! The runtime contract, v0: posture attestation (`keel.posture/1`).
//!
//! Under a runtime such as Moor, the runtime launches keel inside the sandbox,
//! so keel cannot provision or inspect the box it runs in. What it can do is
//! hold the runtime to what it claims. The runtime writes an attestation before
//! starting keel and names it in `KEEL_RUNTIME_ATTESTATION`; keel records the
//! claim in the evidence chain and checks it against `[runtime] require` before
//! any agent runs.
//!
//! An attestation is a claim, not proof. A property the runtime cannot vouch
//! for is `blocked`, never `pass` — the rule every gate follows.

pub mod host;

use crate::config::Config;
use crate::gate::{Check, GateResult};
use crate::paths::Paths;
use crate::run::Run;
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

pub const POSTURE_SCHEMA: &str = "keel.posture/1";
pub const ATTESTATION_ENV: &str = "KEEL_RUNTIME_ATTESTATION";
pub const GATE: &str = "posture";

const SCHEMA_JSON: &str = include_str!("../../schemas/posture.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The runtime checked the property, from outside the sandbox, and it holds.
    Proven,
    /// The runtime cannot vouch for it either way.
    Unproven,
    /// The runtime checked, and it does not hold.
    Violated,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Property {
    pub status: Status,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Attestation {
    pub runtime: String,
    pub properties: BTreeMap<String, Property>,
}

/// Validate raw attestation JSON against the published schema, naming the
/// first offending field.
pub fn parse(raw: &str) -> std::result::Result<Attestation, String> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| format!("not JSON: {e}"))?;
    let schema: serde_json::Value = serde_json::from_str(SCHEMA_JSON).map_err(|e| e.to_string())?;
    let validator = jsonschema::validator_for(&schema).map_err(|e| e.to_string())?;
    if let Some(err) = validator.iter_errors(&value).next() {
        let at = err.instance_path().to_string();
        let at = if at.is_empty() { "/".to_string() } else { at };
        return Err(format!("{at}: {err}"));
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// What the runtime handed keel, if anything.
enum Supplied {
    None,
    Unreadable(String),
    Malformed(String),
    Valid(Attestation),
}

/// Record the runtime's claim and, when `[runtime] require` names anything,
/// judge it. Runs before the agent; `None` means nothing was required.
pub fn preflight(paths: &Paths, cfg: &Config, slug: &str, run: &Run) -> Result<Option<GateResult>> {
    let supplied = match std::env::var_os(ATTESTATION_ENV) {
        None => Supplied::None,
        Some(p) => read_and_record(Path::new(&p), run)?,
    };
    if cfg.runtime.require.is_empty() {
        return Ok(None);
    }

    let mut checks = Vec::new();
    let attestation = match supplied {
        Supplied::Valid(a) => {
            checks.push(Check::pass("attestation", format!("{POSTURE_SCHEMA} from runtime `{}`", a.runtime)));
            Some(a)
        }
        Supplied::None => {
            checks.push(Check::blocked("attestation", format!("{ATTESTATION_ENV} is unset — no runtime vouched for this run")));
            None
        }
        Supplied::Unreadable(why) => {
            checks.push(Check::blocked("attestation", format!("cannot read the attestation — {why}")));
            None
        }
        Supplied::Malformed(why) => {
            checks.push(Check::blocked("attestation", format!("not a valid {POSTURE_SCHEMA} — {why}")));
            None
        }
    };
    for name in &cfg.runtime.require {
        checks.push(judge(name, attestation.as_ref()));
    }

    let mut result = GateResult::new(paths, GATE, Some(slug.to_string()), checks);
    result.run = run.meta.id.clone();
    result.write(&run.gates_dir())?;
    Ok(Some(result))
}

fn judge(name: &str, attestation: Option<&Attestation>) -> Check {
    let id = format!("posture:{name}");
    let Some(a) = attestation else {
        return Check::blocked(&id, "no valid attestation to judge it by");
    };
    match a.properties.get(name) {
        None => Check::blocked(&id, format!("runtime `{}` did not attest it", a.runtime)),
        Some(p) => {
            let evidence = p.evidence.as_deref().map(|e| format!(" ({e})")).unwrap_or_default();
            match p.status {
                Status::Proven => Check::pass(&id, format!("proven{evidence}")),
                Status::Unproven => Check::blocked(&id, format!("runtime `{}` cannot prove it{evidence}", a.runtime)),
                Status::Violated => Check::fail(&id, "proven", format!("violated{evidence}")),
            }
        }
    }
}

/// Put the claim on the record before anything is judged, so a runtime's
/// attestation is in the chain whether or not this repository requires one.
fn read_and_record(path: &Path, run: &Run) -> Result<Supplied> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => return Ok(Supplied::Unreadable(format!("{}: {e}", path.display()))),
    };
    let sha256 = crate::hashing::sha256_hex(raw.as_bytes());
    let parsed = parse(&raw);
    let entry = match &parsed {
        Ok(_) => {
            let v: serde_json::Value = serde_json::from_str(&raw)?;
            serde_json::json!({
                "run": run.meta.id, "sha256": sha256,
                "runtime": v["runtime"], "properties": v["properties"],
            })
        }
        Err(_) => serde_json::json!({ "run": run.meta.id, "sha256": sha256, "valid": false }),
    };
    crate::chain::record(&run.gates_dir(), "attest", entry)?;
    Ok(match parsed {
        Ok(a) => Supplied::Valid(a),
        Err(why) => Supplied::Malformed(why),
    })
}
