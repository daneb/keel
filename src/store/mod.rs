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
    pub front: FrontMatter,
    pub body: String,
}

impl StoreDoc {
    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let (front, body) = frontmatter::split(&raw)
            .with_context(|| format!("in {}", path.display()))?;
        let _ = path;
        Ok(Self { front, body })
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

/// A shared store resolved to a directory on disk.
#[derive(Debug, Clone)]
pub struct Shared {
    pub id: String,
    pub root: PathBuf,
    pub required: bool,
    /// Present when the path does not resolve.
    pub missing: bool,
}

impl Shared {
    pub fn conventions(&self) -> PathBuf { self.root.join("steering").join("conventions.md") }
    pub fn lessons_dir(&self) -> PathBuf { self.root.join("lessons") }
}

/// Resolve every configured shared store, relative to the repository root.
///
/// A store that does not resolve is returned marked `missing` rather than
/// dropped: silently skipping it is exactly how a platform rule stops applying
/// while everyone still believes it is in force.
pub fn shared(paths: &Paths, cfg: &crate::config::Config) -> Vec<Shared> {
    cfg.shared
        .iter()
        .map(|s| {
            let raw = Path::new(&s.path);
            let root = if raw.is_absolute() { raw.to_path_buf() } else { paths.repo.join(raw) };
            Shared {
                id: s.id.clone(),
                missing: !root.is_dir(),
                root,
                required: s.required,
            }
        })
        .collect()
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

/// Hash of everything a projection is rendered from — this repository's store
/// and every shared store beneath it (see hashing.rs for why this is separate
/// from a projection's own body hash).
///
/// Shared stores are hashed too: a platform convention changing must mark this
/// repository's projections stale, or the rule reaches nobody.
pub fn store_hash_with_shared(paths: &Paths, cfg: &crate::config::Config) -> Result<String> {
    let mut h = SetHasher::new();
    for p in projection_inputs(paths)? {
        let rel = paths.rel(&p).to_string_lossy().replace('\\', "/");
        h.add(&rel, &std::fs::read(&p)?);
    }
    for s in shared(paths, cfg) {
        if s.missing {
            // The absence is itself part of the state: a projection rendered
            // without a required store must not look identical to one rendered
            // with it.
            h.add(&format!("shared:{}:missing", s.id), b"");
            continue;
        }
        let mut files: Vec<PathBuf> = Vec::new();
        if s.conventions().is_file() {
            files.push(s.conventions());
        }
        if let Ok(entries) = std::fs::read_dir(s.lessons_dir()) {
            let mut ls: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
                .collect();
            ls.sort();
            files.extend(ls);
        }
        for f in files {
            let rel = f.strip_prefix(&s.root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            h.add(&format!("shared:{}:{rel}", s.id), &std::fs::read(&f).unwrap_or_default());
        }
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
