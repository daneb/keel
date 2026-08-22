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
    pub reused: usize,
    pub refs: usize,
    pub languages: Vec<(String, usize)>,
    /// Grammars whose queries failed to compile this run — see `lang::unavailable`.
    pub degraded: Vec<(&'static str, String)>,
}

/// Facts keel can reuse because the file's content hash is unchanged.
fn previous_facts(
    paths: &Paths,
    candidates: &[walk::Candidate],
) -> Result<std::collections::HashMap<String, FileFacts>> {
    let db_path = paths.index_db();
    if !db_path.exists() {
        return Ok(Default::default());
    }
    let index = match db::Index::open(&db_path) {
        Ok(i) => i,
        Err(_) => return Ok(Default::default()),
    };
    // A schema change invalidates everything: reusing rows written by an older
    // shape is how an index quietly starts lying.
    if index.meta("schema").ok().flatten().as_deref() != Some(db::SCHEMA_VERSION) {
        return Ok(Default::default());
    }
    let Ok(shas) = index.shas() else { return Ok(Default::default()) };

    let mut out = std::collections::HashMap::new();
    for c in candidates {
        let Some(old_sha) = shas.get(&c.rel) else { continue };
        let Ok(bytes) = std::fs::read(&c.abs) else { continue };
        if crate::hashing::sha256_hex(&bytes) != *old_sha {
            continue;
        }
        if let Ok(Some(f)) = index.facts_for(&c.rel) {
            out.insert(c.rel.clone(), f);
        }
    }
    Ok(out)
}

pub fn build(paths: &Paths, cfg: &Config, budget_override: Option<usize>) -> Result<MapReport> {
    build_with(paths, cfg, budget_override, false)
}

pub fn build_with(
    paths: &Paths,
    cfg: &Config,
    budget_override: Option<usize>,
    full: bool,
) -> Result<MapReport> {
    let started = Instant::now();
    let mut map_cfg = cfg.map.clone();
    if let Some(b) = budget_override {
        map_cfg.budget_lines = b;
    }

    let degraded = lang::unavailable();
    let candidates = walk::candidates(&paths.repo, &map_cfg)?;

    // Reuse what has not changed. tree-sitter is fast, but not parsing a file
    // at all is faster, and on a large repo most files are untouched between
    // two runs.
    let reusable = if full {
        Default::default()
    } else {
        previous_facts(paths, &candidates)?
    };
    let reused = reusable.len();

    // Parsing is the only expensive step and it is embarrassingly parallel.
    // One Extractor per rayon worker keeps query compilation off the hot path.
    let facts: Vec<FileFacts> = candidates
        .par_iter()
        .map_init(Extractor::new, |ex, c| {
            if let Some(f) = reusable.get(&c.rel) {
                return f.clone();
            }
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

    // References are only interesting when they point at something this repo
    // defines. Filtering here rather than at extraction keeps the table small
    // and makes `keel refs` answer about code, not about the standard library.
    let defined: std::collections::HashSet<String> =
        facts.iter().flat_map(|f| f.symbols.iter().map(|s| s.name.clone())).collect();
    let mut facts = facts;
    for f in &mut facts {
        f.refs.retain(|r| defined.contains(&r.name));
    }
    let facts = facts;

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
        reused,
        refs: facts.iter().map(|f| f.refs.len()).sum(),
        languages,
        degraded,
    })
}
