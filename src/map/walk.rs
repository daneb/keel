//! Gitignore-aware repository walk.
//!
//! Anything the VCS ignores, keel ignores. On top of that there is a small set
//! of directories that are never source-of-truth even when committed (vendored
//! dependencies, build output), because indexing them buys nothing and blows
//! the map budget on code nobody edits.

use crate::config::MapConfig;
use crate::map::lang::Lang;
use anyhow::Result;
use ignore::{WalkBuilder, overrides::OverrideBuilder};
use std::path::{Path, PathBuf};

const NEVER_INDEX: &[&str] = &[
    "!.git/**", "!.keel/runs/**", "!.keel/store/map/**",
    "!target/**", "!node_modules/**", "!vendor/**", "!.venv/**", "!venv/**",
    "!__pycache__/**", "!dist/**", "!build/**", "!.next/**", "!.svelte-kit/**",
    "!coverage/**", "!*.min.js", "!*.bundle.js", "!*.generated.*", "!*_pb2.py",
];

#[derive(Debug, Clone)]
pub struct Candidate {
    pub abs: PathBuf,
    pub rel: String,
    pub lang: Lang,
    pub bytes: u64,
}

/// Every indexable source file under `root`, in a deterministic order.
pub fn candidates(root: &Path, cfg: &MapConfig) -> Result<Vec<Candidate>> {
    let mut ovr = OverrideBuilder::new(root);
    for pat in NEVER_INDEX {
        ovr.add(pat)?;
    }
    for pat in &cfg.exclude {
        // Config excludes are written positively ("target/**"); invert them so
        // the user never has to think about override syntax.
        let pat = if pat.starts_with('!') { pat.clone() } else { format!("!{pat}") };
        ovr.add(&pat)?;
    }

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .overrides(ovr.build()?)
        .build();

    let mut out = Vec::new();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // unreadable path: not fatal, just not indexed
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let abs = entry.path().to_path_buf();
        let Some(lang) = Lang::from_path(&abs) else { continue };
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let rel = abs.strip_prefix(root).unwrap_or(&abs)
            .to_string_lossy().replace('\\', "/");
        out.push(Candidate { abs, rel, lang, bytes });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}
