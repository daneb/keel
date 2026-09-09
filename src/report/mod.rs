//! One assembled view of what happened to a feature, from G0 to G4.
//!
//! This is the model both `keel report --json` and the read-only web view are
//! rendered from. Having one assembly step rather than two is the point: a
//! browser and a terminal disagreeing about whether a spec is blocked would be
//! worse than having no browser at all.
//!
//! # What is frozen, and what is not
//!
//! `keel.report/1` joins the spine, so it is additive-only from here. It is
//! kept deliberately thin for that reason, and it earns most of its content by
//! **embedding types that are already frozen** — [`GateResult`] is
//! `keel.gate/1`, [`RunMeta`] is `keel.run/1`, [`Check`](crate::gate::Check)
//! travels inside the gate. Reusing them adds no new surface to defend, and it
//! means a reader that already understands a gate file understands this.
//!
//! What this model does *not* do is invent a summary of a run's trajectory.
//! Event-level detail is served separately and stays unversioned until the
//! shape has settled under real use.

pub mod insights;

use crate::approval;
use crate::gate::{self, GateResult};
use crate::paths::Paths;
use crate::pipeline::{self, Stage};
use crate::run::{Run, RunMeta};
use crate::trajectory::{self, Anomaly};
use anyhow::Result;
use serde::Serialize;

pub const SCHEMA: &str = "keel.report/1";

/// Every spec in the repository, and where each stands.
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub generated_at: String,
    pub keel_version: &'static str,
    pub specs: Vec<SpecReport>,
}

#[derive(Debug, Serialize)]
pub struct SpecReport {
    pub slug: String,
    /// The stable stage key, not the display label.
    pub stage: &'static str,
    pub complete: bool,
    /// G0 and G1, which belong to the spec rather than to any one attempt.
    pub gates: Vec<GateResult>,
    /// Stage name to its standing, in `approval::STAGES` order.
    pub approvals: serde_json::Map<String, serde_json::Value>,
    /// Every attempt at this spec, oldest first.
    pub runs: Vec<RunReport>,
}

/// One run, with the gates it produced and what its trajectory cost.
#[derive(Debug, Serialize)]
pub struct RunReport {
    /// `keel.run/1`, inlined so the field names stay the ones on disk.
    #[serde(flatten)]
    pub meta: RunMeta,
    /// G2, G2.5, G3 and G4 — the gates an attempt produces.
    pub gates: Vec<GateResult>,
    pub events: usize,
    pub tokens: usize,
    /// What the trajectory could not be read as. Empty is the normal case; a
    /// non-empty list is shown rather than swallowed, because a viewer that
    /// quietly drops a damaged record is the failure this whole tool argues
    /// against.
    pub anomalies: Vec<Anomaly>,
}

impl Report {
    /// Assemble the report, optionally narrowed to one spec.
    pub fn build(paths: &Paths, only: Option<&str>) -> Result<Self> {
        let slugs: Vec<String> = match only {
            // Load it purely to refuse a slug that does not exist: reporting a
            // phantom spec sitting at stage `spec` would read as a real answer.
            Some(s) => {
                crate::spec::Spec::load(paths, s)?;
                vec![s.to_string()]
            }
            None => crate::spec::list(paths)?,
        };
        let runs = crate::run::list(paths).unwrap_or_default();

        let specs = slugs
            .into_iter()
            .map(|slug| SpecReport::build(paths, &slug, &runs))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            schema: SCHEMA,
            generated_at: chrono::Local::now().to_rfc3339(),
            keel_version: env!("CARGO_PKG_VERSION"),
            specs,
        })
    }
}

impl SpecReport {
    fn build(paths: &Paths, slug: &str, all_runs: &[String]) -> Result<Self> {
        let pos = pipeline::position(paths, slug);

        let gates = ["G0", "G1"]
            .iter()
            .filter_map(|g| gate::previous(paths, slug, g))
            .collect();

        let mut approvals = serde_json::Map::new();
        for stage in approval::STAGES {
            let standing = approval::standing(paths, slug, stage).unwrap_or(approval::Standing::Absent);
            approvals.insert(stage.to_string(), approval::standing_json(&standing));
        }

        // `run::list` already skips a directory with no `run.json`, so a run
        // being created right now simply does not appear yet.
        let runs = all_runs
            .iter()
            .filter_map(|id| Run::load(paths, id).ok())
            .filter(|r| r.meta.spec == slug)
            .map(RunReport::build)
            .collect();

        Ok(Self {
            slug: slug.to_string(),
            stage: pos.stage.key(),
            complete: pos.stage == Stage::Complete,
            gates,
            approvals,
            runs,
        })
    }
}

impl RunReport {
    fn build(run: Run) -> Self {
        // Lenient on purpose: this may be a run in flight, whose trajectory is
        // being appended to as we read it.
        let (events, tokens, anomalies) = match trajectory::scan(&run.trajectory_path()) {
            Ok(s) => {
                let tokens = trajectory::token_total(&s.events);
                (s.events.len(), tokens, s.anomalies)
            }
            Err(_) => (0, 0, Vec::new()),
        };
        Self {
            gates: run.gate_results().unwrap_or_default(),
            meta: run.meta,
            events,
            tokens,
            anomalies,
        }
    }
}
