//! `.keel/keel.toml` — budgets, adapters, exclusions.
//!
//! Budgets are the load-bearing part of this file (PLAN.md P4: "budgets are
//! declared per stage and enforced, not advised"). Everything else has a
//! defensible default so that `keel init` produces a file you can ignore.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CONFIG_SCHEMA: &str = "keel.config/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_schema")]
    pub schema: String,
    #[serde(default)]
    pub map: MapConfig,
    #[serde(default)]
    pub store: StoreConfig,
    #[serde(default = "default_adapters", rename = "adapter")]
    pub adapters: Vec<Adapter>,
    #[serde(default)]
    pub spec: SpecConfig,
    #[serde(default)]
    pub plan: PlanConfig,
    /// External gate checks, keyed by gate id ("G0", "G1", …). This is the P7
    /// extension point that Phase 3 uses to turn a lesson into a check.
    #[serde(default)]
    pub gate: std::collections::BTreeMap<String, GateConfig>,
    #[serde(default = "default_drivers", rename = "driver")]
    pub drivers: Vec<Driver>,
    #[serde(default)]
    pub verify: VerifyConfig,
    #[serde(default)]
    pub oracle: OracleConfig,
    #[serde(default)]
    pub learn: LearnConfig,
    #[serde(default)]
    pub retrieve: RetrieveConfig,
    /// Metrics that may improve and must not regress.
    #[serde(default = "default_ratchets", rename = "ratchet")]
    pub ratchets: Vec<Ratchet>,
}

/// The budget governor's limits (PLAN.md P4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetrieveConfig {
    /// Token ceiling on a single retrieval answer.
    pub query_tokens: usize,
    /// Token ceiling on a task slice, which is deliberately larger.
    pub slice_tokens: usize,
    /// Lines a body may be before reading it needs a recorded justification.
    pub max_unjustified_lines: usize,
}

impl Default for RetrieveConfig {
    fn default() -> Self {
        Self { query_tokens: 2_000, slice_tokens: 6_000, max_unjustified_lines: 300 }
    }
}

/// Thresholds for the learning gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LearnConfig {
    /// Share of episodes that may be UNATTRIBUTABLE before G4 fails.
    ///
    /// Set from the observed baseline: Peralta et al. found 33.1% of rejected
    /// agentic PRs had no observable rationale, so a third is normal and half
    /// means the classifier has stopped explaining anything.
    pub max_unattributable_rate: f64,
}

impl Default for LearnConfig {
    fn default() -> Self {
        Self { max_unattributable_rate: 0.5 }
    }
}

/// One ratchet every repository can measure, so a fresh `keel init` has a
/// working G2 rather than a permanently blocked one. Replace it with something
/// that matters to your project; the point is that the slot is never empty by
/// accident.
fn default_ratchets() -> Vec<Ratchet> {
    vec![Ratchet {
        id: "todo-markers".into(),
        cmd: "git grep -cE 'TODO|FIXME' -- . 2>/dev/null | awk -F: '{s+=$2} END {print s+0}'".into(),
        direction: crate::gate::ratchet::Direction::Down,
    }]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ratchet {
    pub id: String,
    /// A command printing a single integer.
    pub cmd: String,
    pub direction: crate::gate::ratchet::Direction,
}

/// An agent driver: a subprocess that takes a task on stdin and prints a result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Driver {
    pub id: String,
    pub cmd: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 { 900 }

fn default_drivers() -> Vec<Driver> {
    vec![Driver {
        id: "claude-code".into(),
        // A driver is a small adapter script, not keel reaching into a CLI.
        // Shipping the path rather than the invocation keeps drivers thin.
        cmd: ".keel/drivers/claude-code".into(),
        default: true,
        timeout_secs: default_timeout(),
    }]
}

/// The commands G2 runs to establish that the tree is healthy. These are the
/// "exact commands" `tech.md` asks for, in executable form.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VerifyConfig {
    pub build: Option<String>,
    pub test: Option<String>,
    pub lint: Option<String>,
}

/// How to execute each oracle kind. Templates take `{name}`, `{file}` and
/// `{id}`; keeping them in config is what makes oracles language-agnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OracleConfig {
    pub test_cmd: String,
    pub doctest_cmd: String,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            test_cmd: "cargo test --quiet -- --exact {name}".into(),
            doctest_cmd: "cargo test --doc --quiet".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpecConfig {
    /// Hard ceiling on acceptance criteria per spec. This is the anti-bloat
    /// control: agents will happily write forty criteria for a three-file
    /// change, and a spec nobody reads is not a spec.
    pub max_criteria: usize,
    /// Hard ceiling on the length of spec.md.
    pub max_lines: usize,
    /// Ambiguous phrases tolerated in criteria before G0 fails.
    pub max_ambiguities: usize,
    /// Optional command that authors a spec: receives the prompt on stdin and
    /// prints spec.md on stdout. Empty means "keel scaffolds, you write".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,
}

impl Default for SpecConfig {
    fn default() -> Self {
        Self { max_criteria: 12, max_lines: 250, max_ambiguities: 0, cmd: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanConfig {
    /// How far to walk the reverse-import graph when computing blast radius.
    pub blast_depth: usize,
    /// Hard ceiling on the line budget any single task may declare.
    pub max_task_lines: usize,
    /// Hard ceiling on the number of tasks in one plan.
    pub max_tasks: usize,
    /// Lines of diff a single human review can honestly cover (G3).
    pub max_reviewable_lines: usize,
}

impl Default for PlanConfig {
    fn default() -> Self {
        Self { blast_depth: 2, max_task_lines: 150, max_tasks: 15, max_reviewable_lines: 600 }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GateConfig {
    #[serde(default, rename = "check", skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckPlugin>,
}

/// An external check: a subprocess that prints one check result as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckPlugin {
    pub id: String,
    pub cmd: String,
    /// Provenance — e.g. `lesson:L-0012`. Answers "why does this check exist?".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

fn default_schema() -> String { CONFIG_SCHEMA.to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MapConfig {
    /// Hard line budget for the generated `store/steering/structure.md`.
    pub budget_lines: usize,
    /// Hard line budget for each generated per-directory `CODEMAP.md`.
    pub codemap_budget_lines: usize,
    /// Files larger than this are indexed as metadata only, never parsed.
    pub max_file_bytes: u64,
    /// Directories with fewer than this many indexed files get no CODEMAP.
    pub codemap_min_files: usize,
    /// Extra ignore globs, on top of .gitignore and the built-in defaults.
    pub exclude: Vec<String>,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            budget_lines: 400,
            codemap_budget_lines: 150,
            max_file_bytes: 1_048_576,
            codemap_min_files: 2,
            exclude: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    /// Ceiling on the **curated** steering docs — product, tech, conventions.
    ///
    /// Deliberately excludes the generated `structure.md`, which is bound by
    /// `map.budget_lines`. Counting it in both places made this ceiling fire on
    /// every repository large enough to fill the map, and a warning that is
    /// always on is a warning nobody reads.
    pub steering_budget_lines: usize,
}

impl Default for StoreConfig {
    fn default() -> Self { Self { steering_budget_lines: 150 } }
}

/// A projection target. `cmd` is the Phase 5 plugin escape hatch (P7); when it
/// is absent the built-in renderer for `id` is used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adapter {
    pub id: String,
    pub out: String,
    pub budget: usize,
    #[serde(default = "default_sections")]
    pub sections: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,
}

fn default_true() -> bool { true }

/// Section order is also priority order when trimming to budget.
fn default_sections() -> Vec<String> {
    ["conventions", "tech", "structure", "product", "lessons"]
        .iter().map(|s| s.to_string()).collect()
}

fn default_adapters() -> Vec<Adapter> {
    vec![
        Adapter { id: "claude".into(),  out: "CLAUDE.md".into(),                        budget: 180, sections: default_sections(), enabled: true, cmd: None },
        Adapter { id: "agents".into(),  out: "AGENTS.md".into(),                        budget: 180, sections: default_sections(), enabled: true, cmd: None },
        Adapter { id: "kiro".into(),    out: ".kiro/steering/keel.md".into(),           budget: 200, sections: default_sections(), enabled: true, cmd: None },
        Adapter { id: "copilot".into(), out: ".github/copilot-instructions.md".into(),  budget: 120, sections: default_sections(), enabled: true, cmd: None },
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: default_schema(),
            map: MapConfig::default(),
            store: StoreConfig::default(),
            adapters: default_adapters(),
            spec: SpecConfig::default(),
            plan: PlanConfig::default(),
            gate: Default::default(),
            drivers: default_drivers(),
            verify: VerifyConfig::default(),
            oracle: OracleConfig::default(),
            learn: LearnConfig::default(),
            retrieve: RetrieveConfig::default(),
            ratchets: default_ratchets(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let body = toml::to_string_pretty(self)?;
        let doc = format!("# keel configuration — schema {CONFIG_SCHEMA}\n\
                           # Budgets are enforced, not advised. Lower them until they hurt.\n\n{body}");
        std::fs::write(path, doc).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}
