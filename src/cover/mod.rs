//! Is this change covered by a verified bundle? (SPEC-0012)
//!
//! A run is linked to a pull request by the *content* it gated, not by commit
//! SHAs: a driver's work is not committed when G2 judges it. G2 records a hash
//! of the tree it judged; a PR's head is covered when a committed bundle
//! verifies, its run passed, and that hash equals the head's.

use crate::config::Config;
use crate::evidence;
use crate::gate::Verdict;
use crate::hashing::SetHasher;
use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSetBuilder};
use serde::Serialize;
use std::path::Path;

pub const REPORT_SCHEMA: &str = "keel.cover/1";

/// The content hash of the working tree: every path git would show as part
/// of the change, with its bytes as they are on disk now — so the same files
/// hash the same whether committed, staged or untracked.
///
/// Leaves out `.keel/` (committing a bundle and its run records changes it)
/// and keel's rendered projections (`store-drift` already checks those). Keeps
/// lockfiles, unlike G2's incidental set: a dependency change is exactly what
/// has to be covered. The file mode is not part of the hash.
pub fn tree_hash(paths: &Paths, cfg: &Config) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
        .current_dir(&paths.repo)
        .output()
        .context("running git ls-files")?;
    if !out.status.success() {
        bail!("git ls-files failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }

    let mut skip = GlobSetBuilder::new();
    skip.add(Glob::new(".keel/**")?);
    for a in cfg.adapters.iter().filter(|a| a.enabled) {
        if let Ok(g) = Glob::new(&a.out) {
            skip.add(g);
        }
    }
    let skip = skip.build()?;

    let mut files: Vec<String> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .filter(|p| !skip.is_match(p))
        .collect();
    files.sort();
    files.dedup();

    let mut h = SetHasher::new();
    for rel in &files {
        let p = paths.repo.join(rel);
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue; // tracked, but deleted from the working tree
        };
        let bytes = if meta.file_type().is_symlink() {
            std::fs::read_link(&p)?.to_string_lossy().into_owned().into_bytes()
        } else {
            std::fs::read(&p).with_context(|| format!("reading {rel}"))?
        };
        h.add(rel, &bytes);
    }
    Ok(h.finish())
}

#[derive(Debug, Clone, Serialize)]
pub struct Considered {
    pub bundle: String,
    pub covers: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: String,
    pub head: String,
    /// `covered`, `uncovered` or `exempted`.
    pub verdict: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exempt_reason: Option<String>,
    pub bundles: Vec<Considered>,
}

impl Report {
    pub fn exit_code(&self) -> i32 {
        if self.verdict == "uncovered" { 1 } else { 0 }
    }
}

/// Weigh every committed bundle against the head. The first reason that
/// applies is the one reported, in the order a reviewer would ask.
pub fn check(paths: &Paths, cfg: &Config, exempt: Option<String>) -> Result<Report> {
    let head = tree_hash(paths, cfg)?;
    let dir = paths.keel().join("bundles");
    let mut archives: Vec<_> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    archives.retain(|p: &std::path::PathBuf| p.to_string_lossy().ends_with(".tar.gz"));
    archives.sort();

    let bundles: Vec<Considered> = archives
        .iter()
        .map(|a| {
            let (covers, reason) = judge(a, &head);
            Considered { bundle: paths.rel(a).display().to_string(), covers, reason }
        })
        .collect();

    let covered = bundles.iter().any(|b| b.covers);
    let verdict = match (covered, &exempt) {
        (true, _) => "covered",
        (false, Some(_)) => "exempted",
        (false, None) => "uncovered",
    };
    Ok(Report {
        schema: REPORT_SCHEMA.to_string(),
        head,
        verdict: verdict.to_string(),
        exempt_reason: if covered { None } else { exempt },
        bundles,
    })
}

fn judge(archive: &Path, head: &str) -> (bool, String) {
    let report = match evidence::verify::check(archive) {
        Ok(r) => r,
        Err(e) => return (false, format!("failed verification: {e:#}")),
    };
    if report.verdict != Verdict::Pass {
        let failing: Vec<&str> =
            report.checks.iter().filter(|c| c.verdict != Verdict::Pass).map(|c| c.id.as_str()).collect();
        // Blocked means verification could not be completed (a bundle with no
        // chain, say) — not that something failed it.
        let how = if report.verdict == Verdict::Blocked { "verification blocked" } else { "failed verification" };
        return (false, format!("{how} ({})", failing.join(", ")));
    }
    let members = match evidence::read_members(archive) {
        Ok(m) => m,
        Err(e) => return (false, format!("failed verification: {e:#}")),
    };
    let member = |name: &str| members.iter().find(|(p, _)| p == name).map(|(_, b)| b.clone());
    let verdict = member("manifest.json")
        .and_then(|b| serde_json::from_slice::<evidence::Manifest>(&b).ok())
        .and_then(|m| m.verdict);
    if verdict.as_deref() != Some("pass") {
        return (false, format!("run verdict {}", verdict.as_deref().unwrap_or("unknown")));
    }
    let Some(tree) = member("evidence/tree.txt") else {
        return (false, "predates tree hashes (no evidence/tree.txt)".to_string());
    };
    let tree = String::from_utf8_lossy(&tree).trim().to_string();
    if tree != head {
        return (false, format!("a different tree ({})", &tree[..12.min(tree.len())]));
    }
    (true, "covers the head".to_string())
}
