//! Exportable evidence bundles (PLAN.md G3, §5).
//!
//! A bundle is one `.tar.gz` a reviewer who was not present can open and
//! understand: the trajectory, every gate verdict, the evidence those verdicts
//! were reached from, the spec that was agreed, and a README that says what
//! happened. Plus a manifest, so they can tell it has not been edited since.

pub mod manifest;

use crate::hashing::sha256_hex;
use crate::paths::Paths;
use crate::run::Run;
use anyhow::{Context, Result};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
pub use manifest::{Manifest, Member};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Build a bundle for a run. Returns the archive path.
pub fn export(paths: &Paths, run: &Run, out_dir: Option<&Path>) -> Result<PathBuf> {
    let mut members: Vec<(String, Vec<u8>)> = Vec::new();

    // The run directory, in full.
    collect_dir(&run.dir, &run.dir, &mut members)?;

    // The artefacts the run was judged against. A bundle without the spec
    // cannot answer "was this the right thing to build?".
    let spec_dir = crate::spec::Spec::dir(paths, &run.meta.spec);
    if spec_dir.is_dir() {
        collect_dir(&spec_dir, spec_dir.parent().unwrap_or(&spec_dir), &mut members)?;
    }
    // The steering the agent actually saw.
    let steering = paths.steering();
    if steering.is_dir() {
        collect_dir(&steering, &paths.store(), &mut members)?;
    }

    members.sort_by(|a, b| a.0.cmp(&b.0));
    members.dedup_by(|a, b| a.0 == b.0);

    // README first, so it is what a reader meets.
    let readme = readme(paths, run)?;
    members.insert(0, ("README.md".to_string(), readme.into_bytes()));

    // The manifest hashes the same bytes that are written, never a re-read.
    let entries: Vec<Member> = members
        .iter()
        .map(|(path, bytes)| Member {
            path: path.clone(),
            bytes: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        })
        .collect();
    let manifest = Manifest::new(&run.meta, entries);
    let manifest_bytes = format!("{}\n", serde_json::to_string_pretty(&manifest)?).into_bytes();
    members.insert(1, ("manifest.json".to_string(), manifest_bytes));

    let dir = out_dir.map(|d| d.to_path_buf()).unwrap_or_else(|| paths.keel().join("bundles"));
    std::fs::create_dir_all(&dir)?;
    let archive_path = dir.join(format!("keel-{}.tar.gz", run.meta.id));

    let file = std::fs::File::create(&archive_path)
        .with_context(|| format!("creating {}", archive_path.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for (path, bytes) in &members {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        tar.append_data(&mut header, path, bytes.as_slice())
            .with_context(|| format!("adding {path} to the archive"))?;
    }
    tar.into_inner()?.finish()?;

    Ok(archive_path)
}

/// Read every member of a bundle back into memory.
pub fn read_members(archive: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let file = std::fs::File::open(archive)
        .with_context(|| format!("opening {}", archive.display()))?;
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    let mut out = Vec::new();
    for entry in tar.entries()? {
        let mut entry = entry?;
        let raw = entry.path()?.to_string_lossy().to_string();
        // `tar -czf x.tar.gz -C dir .` writes every member as `./name`. That is
        // the same archive by any reasonable reading, so normalise rather than
        // report a bundle with no manifest.
        let path = raw.trim_start_matches("./").to_string();
        if path.is_empty() || path == "." {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        out.push((path, bytes));
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct Verification {
    pub manifest: Manifest,
    /// Members whose bytes no longer match the manifest.
    pub tampered: Vec<String>,
    /// Members listed in the manifest but absent from the archive.
    pub missing: Vec<String>,
    /// Members present in the archive but not listed.
    pub unlisted: Vec<String>,
}

impl Verification {
    pub fn is_intact(&self) -> bool {
        self.tampered.is_empty() && self.missing.is_empty() && self.unlisted.is_empty()
    }
}

/// Check an archive against its own manifest.
pub fn verify(archive: &Path) -> Result<Verification> {
    let members = read_members(archive)?;
    let manifest_bytes = members
        .iter()
        .find(|(p, _)| p == "manifest.json")
        .map(|(_, b)| b.clone())
        .ok_or_else(|| anyhow::anyhow!("{} has no manifest.json", archive.display()))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("parsing the manifest in {}", archive.display()))?;

    let mut tampered = Vec::new();
    let mut missing = Vec::new();
    let mut unlisted = Vec::new();

    for m in &manifest.members {
        match members.iter().find(|(p, _)| *p == m.path) {
            None => missing.push(m.path.clone()),
            Some((_, bytes)) => {
                if sha256_hex(bytes) != m.sha256 {
                    tampered.push(m.path.clone());
                }
            }
        }
    }
    for (path, _) in &members {
        // The manifest cannot list itself without a hash of its own hash.
        if path == "manifest.json" {
            continue;
        }
        if manifest.find(path).is_none() {
            unlisted.push(path.clone());
        }
    }

    Ok(Verification { manifest, tampered, missing, unlisted })
}

/// The part a person reads first.
fn readme(paths: &Paths, run: &Run) -> Result<String> {
    let meta = &run.meta;
    let mut s = String::new();
    s.push_str(&format!("# keel evidence bundle — run {}\n\n", meta.id));
    s.push_str(&format!(
        "Spec `{}`{}, driver `{}`, keel {}.\nStarted {}{}.\n\n",
        meta.spec,
        meta.task.as_ref().map(|t| format!(" task `{t}`")).unwrap_or_default(),
        meta.driver.clone().unwrap_or_else(|| "none".into()),
        meta.keel_version,
        meta.started_at,
        meta.finished_at.as_ref().map(|f| format!(", finished {f}")).unwrap_or_default()
    ));
    if let Some(c) = &meta.base_commit {
        s.push_str(&format!("Base commit `{c}`.\n\n"));
    }

    // --- what changed ------------------------------------------------------
    s.push_str("## What changed\n\n");
    let diff_stat = run.evidence_dir().join("diff-stat.txt");
    match std::fs::read_to_string(&diff_stat) {
        Ok(t) => s.push_str(&format!("```\n{}\n```\n\n", t.trim_end())),
        Err(_) => s.push_str("_No diff was recorded for this run._\n\n"),
    }

    // --- gates -------------------------------------------------------------
    s.push_str("## Gates\n\n");
    let results = run.gate_results()?;
    if results.is_empty() {
        s.push_str("_No gate ran._\n\n");
    } else {
        s.push_str("| gate | verdict | passed | failed | blocked |\n| --- | --- | --- | --- | --- |\n");
        for r in &results {
            let (p, f, b) = r.counts();
            s.push_str(&format!("| {} | **{}** | {p} | {f} | {b} |\n", r.gate, r.verdict.glyph()));
        }
        s.push('\n');
        // Name every failure explicitly: a reviewer must not have to open JSON
        // to find out what went wrong.
        for r in &results {
            let bad: Vec<&crate::gate::Check> = r
                .checks
                .iter()
                .filter(|c| c.verdict != crate::gate::Verdict::Pass)
                .collect();
            if bad.is_empty() {
                continue;
            }
            s.push_str(&format!("### {} — what did not pass\n\n", r.gate));
            for c in bad {
                s.push_str(&format!("- **{}** ({})", c.id, c.verdict.glyph()));
                if let Some(d) = &c.detail {
                    s.push_str(&format!(" — {d}"));
                } else if let (Some(e), Some(a)) = (&c.expected, &c.actual) {
                    s.push_str(&format!("\n  - expected: {e}\n  - actual: {a}"));
                }
                if let Some(ev) = &c.evidence {
                    s.push_str(&format!("\n  - evidence: `{ev}`"));
                }
                s.push('\n');
            }
            s.push('\n');
        }
    }

    // --- the record --------------------------------------------------------
    let events = crate::trajectory::read(&run.trajectory_path()).unwrap_or_default();
    s.push_str("## The record\n\n");
    s.push_str(&format!(
        "`trajectory.jsonl` holds {} events, one JSON object per line, in sequence order.\n\
         {} tokens were put in front of the model across {} injection(s).\n\n",
        events.len(),
        crate::trajectory::token_total(&events),
        events.iter().filter(|e| e.payload.kind() == "inject").count()
    ));
    s.push_str(
        "Verify this bundle has not been edited since it was written:\n\n\
         ```sh\n\
         keel export --verify keel-<run>.tar.gz\n\
         ```\n\n\
         Every member's SHA-256 is recorded in `manifest.json`.\n\n",
    );

    s.push_str("## Contents\n\n");
    s.push_str("- `README.md` — this file\n- `manifest.json` — member hashes\n");
    s.push_str(&format!("- `{}/` — run metadata, trajectory, gate results, evidence\n", meta.id));
    s.push_str(&format!("- `{}/` — the spec, plan and tasks as agreed\n", meta.spec));
    s.push_str("- `steering/` — the durable context the agent was given\n");
    let _ = paths;
    Ok(s)
}

/// Walk `dir`, adding files with paths relative to `relative_to`.
fn collect_dir(dir: &Path, relative_to: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_dir(&path, relative_to, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(relative_to)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        out.push((rel, bytes));
    }
    Ok(())
}

/// Write the manifest JSON Schema to disk so the `schema` oracle can use it.
pub fn write_schema(paths: &Paths) -> Result<PathBuf> {
    let dir = paths.keel().join("schemas");
    std::fs::create_dir_all(&dir)?;
    let p = dir.join("manifest.json");
    std::fs::write(&p, manifest::JSON_SCHEMA)?;
    Ok(p)
}
