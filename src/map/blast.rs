//! Blast radius: what else does this change touch?
//!
//! PLAN.md P4 exposes `blast_radius(sym, d=2)` as a depth-weighted impact set.
//! G1 uses it to make the phrase "blast radius declared" mean something: the
//! plan states a radius, keel computes one from the import graph, and a plan
//! whose declaration is smaller than the computation does not pass.
//!
//! The walk is over *reverse* import edges — from a file to the files that
//! import it — because that is the direction breakage travels.

use crate::map::db::Index;
use anyhow::{Context, Result};
use globset::{Glob, GlobSetBuilder};
use rusqlite::params;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Serialize)]
pub struct Impact {
    pub path: String,
    /// 0 = matched the declared scope directly; n = n imports away from it.
    pub depth: usize,
    pub lines: usize,
    pub rank: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlastRadius {
    pub scope: Vec<String>,
    pub depth: usize,
    /// Files matching the declared scope globs.
    pub seed: Vec<String>,
    /// Every affected file, seed included, nearest first.
    pub impact: Vec<Impact>,
    /// Total lines across the impact set — the honest size of the surface.
    pub impact_lines: usize,
    /// Scope globs that matched no indexed file (new files, or a typo).
    pub unmatched_globs: Vec<String>,
}

impl BlastRadius {
    /// Paths beyond the declared scope, i.e. what the author did not ask for.
    pub fn beyond_scope(&self) -> Vec<&Impact> {
        self.impact.iter().filter(|i| i.depth > 0).collect()
    }
}

/// Compute the impact set for a set of scope globs.
pub fn compute(index: &Index, scope: &[String], depth: usize) -> Result<BlastRadius> {
    let mut builder = GlobSetBuilder::new();
    let mut valid: Vec<String> = Vec::new();
    for pattern in scope {
        let p = pattern.trim();
        if p.is_empty() { continue; }
        match Glob::new(p) {
            Ok(g) => { builder.add(g); valid.push(p.to_string()); }
            Err(e) => anyhow::bail!("scope glob `{p}` is invalid: {e}"),
        }
    }
    let set = builder.build().context("building scope matcher")?;

    // --- load the file table -------------------------------------------------
    let mut stmt = index.conn.prepare("SELECT id, path, lines, rank FROM files")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)? as usize, r.get::<_, String>(1)?, r.get::<_, i64>(2)? as usize, r.get::<_, f64>(3)?))
    })?;
    let mut files: BTreeMap<usize, (String, usize, f64)> = BTreeMap::new();
    for row in rows {
        let (id, path, lines, rank) = row?;
        files.insert(id, (path, lines, rank));
    }

    // --- reverse edges: dst -> importers -------------------------------------
    let mut importers: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut stmt = index.conn.prepare("SELECT src, dst FROM edges")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)? as usize, r.get::<_, i64>(1)? as usize)))?;
    for row in rows {
        let (src, dst) = row?;
        importers.entry(dst).or_default().push(src);
    }

    // --- seed ----------------------------------------------------------------
    let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
    let mut matched_globs: Vec<bool> = vec![false; valid.len()];

    for (id, (path, _, _)) in &files {
        let matches = set.matches(path.as_str());
        if matches.is_empty() { continue; }
        for m in matches {
            if let Some(flag) = matched_globs.get_mut(m) { *flag = true; }
        }
        seen.insert(*id, 0);
        queue.push_back((*id, 0));
    }

    // --- breadth-first over reverse edges ------------------------------------
    while let Some((id, d)) = queue.pop_front() {
        if d >= depth { continue; }
        for &up in importers.get(&id).map(|v| v.as_slice()).unwrap_or(&[]) {
            // First arrival wins: BFS order guarantees it is the shortest path.
            if let std::collections::btree_map::Entry::Vacant(slot) = seen.entry(up) {
                slot.insert(d + 1);
                queue.push_back((up, d + 1));
            }
        }
    }

    let mut impact: Vec<Impact> = seen
        .iter()
        .filter_map(|(id, d)| {
            files.get(id).map(|(path, lines, rank)| Impact {
                path: path.clone(),
                depth: *d,
                lines: *lines,
                rank: *rank,
            })
        })
        .collect();
    impact.sort_by(|a, b| {
        a.depth.cmp(&b.depth)
            .then(b.rank.partial_cmp(&a.rank).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.path.cmp(&b.path))
    });

    let seed: Vec<String> = impact.iter().filter(|i| i.depth == 0).map(|i| i.path.clone()).collect();
    let impact_lines = impact.iter().map(|i| i.lines).sum();
    let unmatched_globs = valid
        .iter()
        .zip(matched_globs.iter())
        .filter(|(_, m)| !**m)
        .map(|(g, _)| g.clone())
        .collect();

    Ok(BlastRadius {
        scope: valid,
        depth,
        seed,
        impact,
        impact_lines,
        unmatched_globs,
    })
}

/// Files that define a symbol by name — the entry point for
/// `keel blast --symbol AuthGuard`.
pub fn files_defining(index: &Index, symbol: &str) -> Result<Vec<String>> {
    let mut stmt = index.conn.prepare(
        "SELECT DISTINCT f.path FROM symbols s JOIN files f ON f.id = s.file_id
         WHERE s.name = ?1 ORDER BY f.rank DESC",
    )?;
    let rows = stmt.query_map(params![symbol], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::extract::{Extractor, FileFacts};
    use crate::map::lang::Lang;

    /// Unique per call, not merely per nanosecond: `SystemTime::now()` is coarse
    /// enough on some platforms that two threads starting together get the same
    /// value, and two tests sharing a directory is a flake that costs an
    /// afternoon to diagnose.
    fn unique_dir(prefix: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// A tiny repo: main → api → core, plus an unrelated leaf.
    fn index() -> Index {
        let dir = unique_dir("keel-blast");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.sqlite");
        let (mut idx, tmp) = Index::create(&path).unwrap();

        let mut ex = Extractor::new();
        let facts: Vec<FileFacts> = vec![
            ex.extract("src/main.rs", Lang::Rust, b"fn main() {}\n"),
            ex.extract("src/api.rs", Lang::Rust, b"pub fn serve() {}\n"),
            ex.extract("src/core.rs", Lang::Rust, b"pub struct Guard;\n"),
            ex.extract("src/lonely.rs", Lang::Rust, b"pub fn nobody() {}\n"),
        ];
        // main(0) -> api(1) -> core(2); lonely(3) is isolated.
        let edges = vec![(0usize, 1usize), (1, 2)];
        let ranks = vec![0.4, 0.3, 0.2, 0.1];
        let degs = vec![0usize, 1, 1, 0];
        idx.write_all(&facts, &ranks, &degs, &edges, &[]).unwrap();
        drop(idx);
        crate::map::db::promote(&tmp, &path).unwrap();
        Index::open(&path).unwrap()
    }

    #[test]
    fn depth_zero_is_exactly_the_declared_scope() {
        let idx = index();
        let b = compute(&idx, &["src/core.rs".into()], 0).unwrap();
        assert_eq!(b.seed, vec!["src/core.rs".to_string()]);
        assert_eq!(b.impact.len(), 1);
        assert!(b.beyond_scope().is_empty());
    }

    #[test]
    fn one_hop_finds_direct_importers() {
        let idx = index();
        let b = compute(&idx, &["src/core.rs".into()], 1).unwrap();
        let paths: Vec<&str> = b.impact.iter().map(|i| i.path.as_str()).collect();
        assert!(paths.contains(&"src/api.rs"), "{paths:?}");
        assert!(!paths.contains(&"src/main.rs"), "main is two hops away: {paths:?}");
    }

    #[test]
    fn two_hops_reach_the_transitive_importer() {
        let idx = index();
        let b = compute(&idx, &["src/core.rs".into()], 2).unwrap();
        let by_path: BTreeMap<&str, usize> =
            b.impact.iter().map(|i| (i.path.as_str(), i.depth)).collect();
        assert_eq!(by_path.get("src/core.rs"), Some(&0));
        assert_eq!(by_path.get("src/api.rs"), Some(&1));
        assert_eq!(by_path.get("src/main.rs"), Some(&2));
        assert!(!by_path.contains_key("src/lonely.rs"), "unrelated file pulled in");
    }

    #[test]
    fn globs_expand_across_the_tree() {
        let idx = index();
        let b = compute(&idx, &["src/**".into()], 0).unwrap();
        assert_eq!(b.seed.len(), 4);
        assert!(b.unmatched_globs.is_empty());
    }

    #[test]
    fn a_glob_matching_nothing_is_reported_not_ignored() {
        let idx = index();
        let b = compute(&idx, &["src/core.rs".into(), "web/**".into()], 1).unwrap();
        assert_eq!(b.unmatched_globs, vec!["web/**".to_string()]);
    }

    #[test]
    fn impact_is_ordered_nearest_first_and_deterministic() {
        let idx = index();
        let a = compute(&idx, &["src/core.rs".into()], 2).unwrap();
        let b = compute(&idx, &["src/core.rs".into()], 2).unwrap();
        let pa: Vec<&str> = a.impact.iter().map(|i| i.path.as_str()).collect();
        let pb: Vec<&str> = b.impact.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(pa, pb);
        assert!(a.impact.windows(2).all(|w| w[0].depth <= w[1].depth));
    }

    #[test]
    fn an_invalid_glob_is_an_error_not_an_empty_radius() {
        let idx = index();
        assert!(compute(&idx, &["src/[".into()], 1).is_err());
    }

    #[test]
    fn symbols_can_be_located_by_name() {
        let idx = index();
        assert_eq!(files_defining(&idx, "Guard").unwrap(), vec!["src/core.rs".to_string()]);
        assert!(files_defining(&idx, "NoSuchThing").unwrap().is_empty());
    }
}
