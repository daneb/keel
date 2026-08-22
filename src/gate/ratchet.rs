//! The baseline ratchet (PLAN.md Phase 2, G2).
//!
//! A ratchet is a metric that may improve and must not regress. Each is a
//! command that prints a number; the baseline is committed, and G2 fails when a
//! metric moves the wrong way.
//!
//! This is the cheapest possible defence against the slow rot that no single
//! review catches — one more warning, one fewer test — because no individual
//! change ever looks like the problem.

use crate::config::Config;
use crate::paths::Paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BASELINE_SCHEMA: &str = "keel.baseline/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Lower is better: warnings, TODOs, dead code.
    Down,
    /// Higher is better: tests, coverage.
    Up,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub schema: String,
    pub recorded_at: String,
    pub metrics: BTreeMap<String, i64>,
}

impl Baseline {
    pub fn load(paths: &Paths) -> Result<Option<Self>> {
        let p = paths.keel().join("baseline.json");
        if !p.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&p)?;
        Ok(Some(serde_json::from_str(&raw).with_context(|| format!("parsing {}", p.display()))?))
    }

    pub fn save(paths: &Paths, metrics: BTreeMap<String, i64>) -> Result<Self> {
        let b = Self {
            schema: BASELINE_SCHEMA.to_string(),
            recorded_at: chrono::Local::now().to_rfc3339(),
            metrics,
        };
        let p = paths.keel().join("baseline.json");
        std::fs::write(&p, format!("{}\n", serde_json::to_string_pretty(&b)?))?;
        Ok(b)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Measurement {
    pub id: String,
    pub direction: Direction,
    pub value: Option<i64>,
    pub baseline: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Measurement {
    /// A metric that moved the wrong way.
    pub fn regressed(&self) -> bool {
        match (self.value, self.baseline) {
            (Some(v), Some(b)) => match self.direction {
                Direction::Down => v > b,
                Direction::Up => v < b,
            },
            _ => false,
        }
    }

    pub fn improved(&self) -> bool {
        match (self.value, self.baseline) {
            (Some(v), Some(b)) => match self.direction {
                Direction::Down => v < b,
                Direction::Up => v > b,
            },
            _ => false,
        }
    }

    pub fn describe(&self) -> String {
        match (self.value, self.baseline) {
            (Some(v), Some(b)) if v == b => format!("{} {v} (unchanged)", self.id),
            (Some(v), Some(b)) => format!("{} {b} → {v}", self.id),
            (Some(v), None) => format!("{} {v} (no baseline)", self.id),
            (None, _) => format!("{} could not be measured", self.id),
        }
    }
}

/// Measure every configured ratchet against the recorded baseline.
pub fn measure(paths: &Paths, cfg: &Config) -> Result<Vec<Measurement>> {
    let baseline = Baseline::load(paths)?;
    let mut out = Vec::new();
    for r in &cfg.ratchets {
        let (value, error) = match run_metric(paths, &r.cmd) {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e.to_string())),
        };
        out.push(Measurement {
            id: r.id.clone(),
            direction: r.direction,
            value,
            baseline: baseline.as_ref().and_then(|b| b.metrics.get(&r.id).copied()),
            error,
        });
    }
    Ok(out)
}

/// Run a metric command and read a single integer from its output.
fn run_metric(paths: &Paths, cmd: &str) -> Result<i64> {
    let shell_bin = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };
    let out = std::process::Command::new(shell_bin)
        .arg(flag)
        .arg(cmd)
        .current_dir(&paths.repo)
        .output()
        .with_context(|| format!("running `{cmd}`"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_metric(&text).ok_or_else(|| {
        anyhow::anyhow!(
            "`{cmd}` printed no integer (got `{}`)",
            text.trim().chars().take(60).collect::<String>()
        )
    })
}

/// The last integer in the output, so `grep -c` and chattier tools both work.
fn parse_metric(text: &str) -> Option<i64> {
    text.split_whitespace()
        .filter_map(|t| t.trim().parse::<i64>().ok())
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(direction: Direction, value: i64, baseline: i64) -> Measurement {
        Measurement {
            id: "x".into(),
            direction,
            value: Some(value),
            baseline: Some(baseline),
            error: None,
        }
    }

    #[test]
    fn down_metrics_regress_when_they_rise() {
        assert!(m(Direction::Down, 5, 3).regressed());
        assert!(!m(Direction::Down, 3, 3).regressed());
        assert!(m(Direction::Down, 1, 3).improved());
    }

    #[test]
    fn up_metrics_regress_when_they_fall() {
        assert!(m(Direction::Up, 90, 100).regressed());
        assert!(!m(Direction::Up, 110, 100).regressed());
        assert!(m(Direction::Up, 110, 100).improved());
    }

    #[test]
    fn a_missing_baseline_is_not_a_regression() {
        let mut x = m(Direction::Down, 99, 0);
        x.baseline = None;
        assert!(!x.regressed(), "the first measurement cannot regress");
        assert!(x.describe().contains("no baseline"));
    }

    #[test]
    fn an_unmeasurable_metric_is_not_a_regression() {
        let x = Measurement {
            id: "x".into(), direction: Direction::Down,
            value: None, baseline: Some(3), error: Some("no such tool".into()),
        };
        assert!(!x.regressed(), "an unmeasurable metric must block, not fail");
    }

    #[test]
    fn metric_parsing_takes_the_last_integer() {
        assert_eq!(parse_metric("42\n"), Some(42));
        assert_eq!(parse_metric("warning: x\n7\n"), Some(7));
        assert_eq!(parse_metric("test result: ok. 140 passed"), Some(140));
        assert_eq!(parse_metric("no numbers here"), None);
    }
}
