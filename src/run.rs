//! A run: one attempt at one task, and everything it produced.
//!
//! `.keel/runs/<run-id>/` is the unit of evidence (PLAN.md §4.1). Everything a
//! gate needs to justify its verdict lives inside one directory, so exporting a
//! run is a directory walk rather than a scavenger hunt.

use crate::gate::GateResult;
use crate::paths::Paths;
use crate::trajectory::Trajectory;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const RUN_SCHEMA: &str = "keel.run/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub schema: String,
    pub id: String,
    pub spec: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    pub keel_version: String,
    pub store_hash: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// Commit the working tree was at when the run started, for reproducibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
}

pub struct Run {
    pub dir: PathBuf,
    pub meta: RunMeta,
}

impl Run {
    /// Create a new run directory and its metadata.
    pub fn create(
        paths: &Paths,
        spec: &str,
        task: Option<String>,
        driver: Option<String>,
        store_hash: &str,
    ) -> Result<Self> {
        let id = crate::gate::run_id();
        let dir = paths.runs().join(&id);
        if dir.exists() {
            bail!("run {id} already exists");
        }
        std::fs::create_dir_all(dir.join("evidence"))?;
        std::fs::create_dir_all(dir.join("gates"))?;

        let meta = RunMeta {
            schema: RUN_SCHEMA.to_string(),
            id,
            spec: spec.to_string(),
            task,
            driver,
            keel_version: env!("CARGO_PKG_VERSION").to_string(),
            store_hash: store_hash.to_string(),
            started_at: chrono::Local::now().to_rfc3339(),
            finished_at: None,
            verdict: None,
            base_commit: head_commit(paths),
        };
        let run = Self { dir, meta };
        run.save()?;
        Ok(run)
    }

    pub fn load(paths: &Paths, id: &str) -> Result<Self> {
        let dir = paths.runs().join(id);
        let meta_path = dir.join("run.json");
        if !meta_path.exists() {
            bail!("no run `{id}` at {}", paths.rel(&dir).display());
        }
        let raw = std::fs::read_to_string(&meta_path)?;
        let meta: RunMeta = serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", meta_path.display()))?;
        Ok(Self { dir, meta })
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.meta)?;
        std::fs::write(self.dir.join("run.json"), format!("{json}\n"))?;
        Ok(())
    }

    pub fn trajectory_path(&self) -> PathBuf {
        self.dir.join("trajectory.jsonl")
    }

    pub fn open_trajectory(&self) -> Result<Trajectory> {
        Trajectory::open(&self.trajectory_path())
    }

    pub fn gates_dir(&self) -> PathBuf {
        self.dir.join("gates")
    }

    pub fn evidence_dir(&self) -> PathBuf {
        self.dir.join("evidence")
    }

    /// Write an evidence file and return its path relative to the run root,
    /// which is the form a gate check records.
    pub fn write_evidence(&self, name: &str, content: &str) -> Result<String> {
        let path = self.evidence_dir().join(name);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&path, content)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(format!("evidence/{name}"))
    }

    pub fn finish(&mut self, verdict: &str) -> Result<()> {
        self.meta.finished_at = Some(chrono::Local::now().to_rfc3339());
        self.meta.verdict = Some(verdict.to_string());
        self.save()
    }

    /// Gate results recorded in this run, sorted by gate name.
    pub fn gate_results(&self) -> Result<Vec<GateResult>> {
        let dir = self.gates_dir();
        if !dir.is_dir() {
            return Ok(vec![]);
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        files.sort();
        files.iter().map(|p| GateResult::read(p)).collect()
    }
}

/// Every run id on disk, oldest first.
pub fn list(paths: &Paths) -> Result<Vec<String>> {
    let dir = paths.runs();
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut out: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("run.json").is_file())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect();
    // Run ids are `YYYY-MM-DD-xxx`, so lexicographic order is chronological
    // within a day and correct across days.
    out.sort();
    Ok(out)
}

pub fn latest(paths: &Paths) -> Result<Option<String>> {
    Ok(list(paths)?.pop())
}

fn head_commit(paths: &Paths) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&paths.repo)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Resolve a run reference: an explicit id, or the most recent run.
pub fn resolve(paths: &Paths, id: Option<String>) -> Result<String> {
    match id {
        Some(i) => Ok(i),
        None => latest(paths)?
            .ok_or_else(|| anyhow::anyhow!("no runs yet — `keel run <spec>` first")),
    }
}

/// Files a run directory must contain to be a complete record.
pub fn required_members() -> &'static [&'static str] {
    &["run.json", "trajectory.jsonl"]
}
