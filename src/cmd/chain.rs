//! `keel chain verify|head` — check the evidence chain, or print its head for
//! anchoring somewhere the chain's writer cannot reach.

use crate::chain::{self, Outcome};
use crate::paths::Paths;
use anyhow::Result;

pub fn verify(anchor: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let outcome = chain::verify(&chain::path_in(&paths.repo), anchor.as_deref())?;

    if json {
        let v = match &outcome {
            Outcome::Intact { entries, head, self_attested } => serde_json::json!({
                "verdict": "intact", "entries": entries, "head": head, "self_attested": self_attested,
            }),
            Outcome::Broken { line, seq, reason } => serde_json::json!({
                "verdict": "broken", "line": line, "seq": seq, "reason": reason,
            }),
            Outcome::HeadMismatch { expected, actual } => serde_json::json!({
                "verdict": "head_mismatch", "expected": expected, "actual": actual,
            }),
        };
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        match &outcome {
            Outcome::Intact { entries, head, self_attested } => {
                println!("chain intact — {entries} entries");
                println!("head: {head}");
                if *self_attested {
                    println!("self-attested: keel wrote these entries itself, not a runtime");
                }
            }
            Outcome::Broken { line, seq, reason } => {
                let which = seq.map(|s| format!("entry {s}")).unwrap_or_else(|| "entry".to_string());
                println!("chain broken at {which} (line {line}): {reason}");
            }
            Outcome::HeadMismatch { expected, actual } => {
                println!("chain head does not match the anchor");
                println!("  expected: {expected}");
                println!("  actual:   {actual}");
            }
        }
    }
    Ok(if matches!(outcome, Outcome::Intact { .. }) { 0 } else { 1 })
}

pub fn head() -> Result<i32> {
    let paths = Paths::require_init()?;
    println!("{}", chain::head(&chain::path_in(&paths.repo))?);
    Ok(0)
}
