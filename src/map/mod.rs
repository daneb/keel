//! `keel map` — walk, parse, rank, fit to budget, write.
//!
//! The output is three things: a SQLite symbol index (machine), a budget-fitted
//! `structure.md` (every agent, every session), and per-directory `CODEMAP.md`
//! files (local detail, pulled only when work touches that directory).

pub mod blast;
pub mod db;
pub mod extract;
pub mod lang;
pub mod rank;
pub mod render;
pub mod resolve;
pub mod walk;

use crate::config::Config;
use crate::paths::Paths;
use anyhow::Result;
use extract::{Extractor, FileFacts};
use rayon::prelude::*;
use std::time::Instant;

pub struct MapReport {
    pub files: usize,
    pub symbols: usize,
    pub edges: usize,
    pub unparsed: usize,
    pub elapsed_ms: u128,
    pub structure_lines: usize,
    pub structure_budget: usize,
    pub codemaps: usize,
    pub languages: Vec<(String, usize)>,
    /// Grammars whose queries failed to compile this run — see `lang::unavailable`.
    pub degraded: Vec<(&'static str, String)>,
}

pub fn build(paths: &Paths, cfg: &Config, budget_override: Option<usize>) -> Result<MapReport> {
    let started = Instant::now();
    let mut map_cfg = cfg.map.clone();
    if let Some(b) = budget_override {
        map_cfg.budget_lines = b;
    }

    let degraded = lang::unavailable();
    let candidates = walk::candidates(&paths.repo, &map_cfg)?;

    // Parsing is the only expensive step and it is embarrassingly parallel.
    // One Extractor per rayon worker keeps query compilation off the hot path.
    let facts: Vec<FileFacts> = candidates
        .par_iter()
        .map_init(Extractor::new, |ex, c| {
            let source = match std::fs::read(&c.abs) {
                Ok(s) => s,
                Err(_) => return extract::unparsed(&c.rel, c.lang, 0, String::new(), 0),
            };
            if c.bytes > map_cfg.max_file_bytes {
                let sha = crate::hashing::sha256_hex(&source);
                let lines = source.iter().filter(|b| **b == b'\n').count();
                return extract::unparsed(&c.rel, c.lang, c.bytes, sha, lines);
            }
            ex.extract(&c.rel, c.lang, &source)
        })
        .collect();

    // --- graph --------------------------------------------------------------
    let rel_paths: Vec<String> = facts.iter().map(|f| f.rel.clone()).collect();
    let resolver = resolve::Resolver::new(&rel_paths);
    let mut graph = rank::Graph::new(facts.len());
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (i, f) in facts.iter().enumerate() {
        for raw in &f.imports {
            if let Some(j) = resolver.resolve(&f.rel, f.lang, raw)
                && i != j
            {
                graph.add_edge(i, j);
                edges.push((i, j));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();

    let priors: Vec<f64> = facts
        .iter()
        .map(|f| rank::prior(&f.rel, f.symbols.len(), f.lines))
        .collect();
    let ranks = graph.pagerank(&priors);
    let in_degrees = graph.in_degree();

    // --- index --------------------------------------------------------------
    let index_path = paths.index_db();
    let (mut index, tmp) = db::Index::create(&index_path)?;
    let meta = vec![
        ("generated_at", crate::store::today()),
        ("repo", paths.repo.to_string_lossy().to_string()),
        ("keel_version", env!("CARGO_PKG_VERSION").to_string()),
    ];
    index.write_all(&facts, &ranks, &in_degrees, &edges, &meta)?;
    drop(index);
    db::promote(&tmp, &index_path)?;

    // --- rendered maps ------------------------------------------------------
    let view = render::View::new(&facts, &ranks, &in_degrees);
    let structure = render::structure_md(paths, &view, map_cfg.budget_lines);
    let structure_lines = structure.lines().count();
    render::write_structure(paths, &structure)?;
    let codemaps = render::write_codemaps(paths, &view, &map_cfg)?;

    let symbols = facts.iter().map(|f| f.symbols.len()).sum();
    let unparsed = facts.iter().filter(|f| !f.parse_ok).count();

    let mut langs: std::collections::HashMap<&str, usize> = Default::default();
    for f in &facts {
        *langs.entry(f.lang.name()).or_default() += 1;
    }
    let mut languages: Vec<(String, usize)> =
        langs.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    languages.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    Ok(MapReport {
        files: facts.len(),
        symbols,
        edges: edges.len(),
        unparsed,
        elapsed_ms: started.elapsed().as_millis(),
        structure_lines,
        structure_budget: map_cfg.budget_lines,
        codemaps,
        languages,
        degraded,
    })
}
