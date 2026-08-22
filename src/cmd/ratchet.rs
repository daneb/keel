//! `keel ratchet` — show the metrics that must not regress, or set them.

use crate::config::Config;
use crate::gate::ratchet::{Baseline, measure};
use crate::paths::Paths;
use anyhow::Result;
use std::collections::BTreeMap;

pub fn run(accept: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    if cfg.ratchets.is_empty() {
        println!("  no ratchets configured — add a [[ratchet]] block to .keel/keel.toml, e.g.\n");
        println!("    [[ratchet]]");
        println!("    id = \"clippy-warnings\"");
        println!("    cmd = \"cargo clippy --all-targets -q 2>&1 | grep -c '^warning' || true\"");
        println!("    direction = \"down\"");
        return Ok(0);
    }

    let measurements = measure(&paths, &cfg)?;
    let mut regressed = false;
    for m in &measurements {
        let marker = if m.regressed() {
            regressed = true;
            "REGRESSED"
        } else if m.improved() {
            "improved"
        } else if m.value.is_none() {
            "BLOCKED"
        } else {
            "held"
        };
        println!("  {marker:<10} {}", m.describe());
        if let Some(e) = &m.error {
            println!("             {e}");
        }
    }

    if accept {
        let metrics: BTreeMap<String, i64> = measurements
            .iter()
            .filter_map(|m| m.value.map(|v| (m.id.clone(), v)))
            .collect();
        let unmeasured = measurements.len() - metrics.len();
        Baseline::save(&paths, metrics)?;
        println!("\n  baseline recorded in .keel/baseline.json");
        if unmeasured > 0 {
            println!("  {unmeasured} metric(s) could not be measured and were not recorded");
        }
        return Ok(0);
    }

    if regressed {
        println!("\n  a metric moved the wrong way — fix it, or `keel ratchet --accept` to move the baseline deliberately");
        return Ok(1);
    }
    Ok(0)
}
