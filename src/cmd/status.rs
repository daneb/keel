//! `keel status` — is the picture current, and is it within budget?

use crate::config::Config;
use crate::map::db::Index;
use crate::paths::Paths;
use crate::projection::drift;
use crate::store::{self, StoreDoc};
use anyhow::Result;

fn count_codemaps(dir: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let p = e.path();
            if p.is_dir() { count_codemaps(&p) }
            else if p.file_name().and_then(|n| n.to_str()) == Some("CODEMAP.md") { 1 }
            else { 0 }
        })
        .sum()
}

pub fn run() -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let store_hash = store::store_hash(&paths)?;

    println!("keel — {}\n", paths.repo.display());

    // --- index ------------------------------------------------------------
    let db = paths.index_db();
    if db.exists() {
        let index = Index::open(&db)?;
        let (files, symbols) = index.counts()?;
        let generated = index.meta("generated_at")?.unwrap_or_else(|| "unknown".into());
        println!("index      {files} files · {symbols} symbols · built {generated}");
    } else {
        println!("index      absent — run `keel map`");
    }

    // --- steering ---------------------------------------------------------
    // Curated docs and the generated map are budgeted separately — each budget
    // binds exactly the thing whose size it controls.
    let mut curated_lines = 0usize;
    let mut rows = Vec::new();
    for path in [paths.product(), paths.tech(), paths.conventions()] {
        if let Some(doc) = StoreDoc::read_optional(&path)? {
            let n = doc.line_count();
            curated_lines += n;
            let verified = doc.front.verified_at.clone().unwrap_or_else(|| "—".into());
            rows.push(format!(
                "  {:<16} {:>5} lines · verified {}",
                path.file_name().unwrap().to_string_lossy(), n, verified
            ));
        }
    }
    let budget = cfg.store.steering_budget_lines;
    let over_curated = curated_lines > budget;
    println!("steering   {curated_lines}/{budget} lines curated{}", if over_curated { "  OVER" } else { "" });
    for r in rows { println!("{r}"); }

    let mut over_map = false;
    if let Some(doc) = StoreDoc::read_optional(&paths.structure())? {
        let n = doc.line_count();
        over_map = n > cfg.map.budget_lines;
        println!(
            "map        {}/{} lines generated{}",
            n, cfg.map.budget_lines, if over_map { "  OVER" } else { "" }
        );
        println!(
            "  {:<16} {:>5} lines · verified {}",
            "structure.md", n, doc.front.verified_at.clone().unwrap_or_else(|| "—".into())
        );
    }

    let codemaps = count_codemaps(&paths.map_dir());
    if codemaps > 0 {
        println!("codemaps   {codemaps} per-directory map(s)");
    }

    // ADRs are deliberately outside the projection budget, so without a count
    // here they would be write-only.
    let decisions = std::fs::read_dir(paths.decisions())
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name().to_str().is_some_and(|n| n.starts_with("ADR-") && n.ends_with(".md"))
                })
                .count()
        })
        .unwrap_or(0);
    if decisions > 0 {
        println!("decisions  {} ADR(s) in {}", decisions, paths.rel(&paths.decisions()).display());
    }

    let lessons = store::lessons(&paths)?;
    println!("lessons    {} card{}", lessons.len(), if lessons.len() == 1 { "" } else { "s" });

    let inbox_count = std::fs::read_dir(paths.inbox())
        .map(|d| d.filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "md")).count())
        .unwrap_or(0);
    if inbox_count > 0 {
        println!("inbox      {inbox_count} unreconciled edit(s) in {}", paths.rel(&paths.inbox()).display());
    }

    // --- projections ------------------------------------------------------
    println!("\nprojections (store={})", crate::hashing::short(&store_hash));
    let reports = drift::check_all(&paths, &cfg, &store_hash)?;
    for r in &reports {
        let budget = match r.lines {
            Some(l) => format!("{}/{}", l, r.budget),
            None => "—".into(),
        };
        let over = if r.over_budget { " OVER BUDGET" } else { "" };
        println!("  {:<8} {:<10} {:>9}{}  {}", r.state.glyph(), r.adapter, budget, over, r.path);
    }

    let blocking = reports.iter().any(|r| r.state.is_blocking() || r.over_budget)
        || over_curated
        || over_map
        || !db.exists();
    if blocking {
        println!("\nnot current — see the states above");
    } else {
        println!("\nall current");
    }
    Ok(if blocking { 1 } else { 0 })
}
