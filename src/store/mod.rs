//! The canonical knowledge substrate (PLAN.md P2, §4.2).
//!
//! Two tiers: **steering** (durable, always projected, hard budget) and
//! **lessons** (scoped, injected — empty until Phase 3, but the store hash
//! already covers them so projections invalidate correctly when they arrive).

pub mod frontmatter;

use crate::hashing::SetHasher;
use crate::paths::Paths;
use anyhow::{Context, Result};
use frontmatter::FrontMatter;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct StoreDoc {
    pub path: PathBuf,
    pub front: FrontMatter,
    pub body: String,
}

impl StoreDoc {
    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let (front, body) = frontmatter::split(&raw)
            .with_context(|| format!("in {}", path.display()))?;
        Ok(Self { path: path.to_path_buf(), front, body })
    }

    pub fn read_optional(path: &Path) -> Result<Option<Self>> {
        if path.exists() { Ok(Some(Self::read(path)?)) } else { Ok(None) }
    }

    pub fn write(path: &Path, front: &FrontMatter, body: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, frontmatter::join(front, body)?)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Body with the leading `# Title` stripped — projections supply their own
    /// heading level, and a duplicated title wastes budget.
    pub fn body_without_title(&self) -> &str {
        let b = self.body.trim_start();
        if let Some(rest) = b.strip_prefix("# ") {
            match rest.find('\n') {
                Some(i) => rest[i + 1..].trim_start_matches('\n'),
                None => "",
            }
        } else {
            b
        }
    }

    pub fn line_count(&self) -> usize {
        self.body.lines().count()
    }
}

/// Every file that feeds a projection, in a stable order.
pub fn projection_inputs(paths: &Paths) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for p in [paths.product(), paths.tech(), paths.structure(), paths.conventions()] {
        if p.is_file() {
            out.push(p);
        }
    }
    let mut extra: Vec<PathBuf> = Vec::new();
    for dir in [paths.steering(), paths.lessons()] {
        if !dir.is_dir() { continue; }
        for entry in std::fs::read_dir(&dir)? {
            let p = entry?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("md") && !out.contains(&p) {
                extra.push(p);
            }
        }
    }
    extra.sort();
    out.extend(extra);
    Ok(out)
}

/// Hash of everything a projection is rendered from (see hashing.rs for why
/// this is separate from the projection's own body hash).
pub fn store_hash(paths: &Paths) -> Result<String> {
    let mut h = SetHasher::new();
    for p in projection_inputs(paths)? {
        let rel = paths.rel(&p).to_string_lossy().replace('\\', "/");
        let content = std::fs::read(&p)?;
        h.add(&rel, &content);
    }
    Ok(h.finish())
}

/// Lesson cards, sorted by id. Phase 3 fills this; Phase 0 renders whatever is
/// there so a hand-written lesson works today.
pub fn lessons(paths: &Paths) -> Result<Vec<StoreDoc>> {
    let dir = paths.lessons();
    if !dir.is_dir() { return Ok(vec![]); }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    files.sort();
    files.iter().map(|p| StoreDoc::read(p)).collect()
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
