//! `keel blast` — the impact set for a glob or a symbol, on its own.

use crate::config::Config;
use crate::map::blast;
use crate::map::db::Index;
use crate::paths::Paths;
use anyhow::{Result, bail};

pub fn run(targets: Vec<String>, symbol: Option<String>, depth: Option<usize>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let db = paths.index_db();
    if !db.exists() {
        bail!("no symbol index — run `keel map` first");
    }
    let index = Index::open(&db)?;
    let depth = depth.unwrap_or(cfg.plan.blast_depth);

    let scope = match &symbol {
        Some(name) => {
            let files = blast::files_defining(&index, name)?;
            if files.is_empty() {
                bail!("no indexed symbol named `{name}`");
            }
            println!("  `{name}` is defined in: {}\n", files.join(", "));
            files
        }
        None => {
            if targets.is_empty() {
                bail!("name a path glob, or pass --symbol <name>");
            }
            targets
        }
    };

    let radius = blast::compute(&index, &scope, depth)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&radius)?);
        return Ok(0);
    }

    println!(
        "  {} file(s), {} lines at depth {}",
        radius.impact.len(),
        radius.impact_lines,
        depth
    );
    for i in &radius.impact {
        let marker = if i.depth == 0 { "scope".to_string() } else { format!("   +{}", i.depth) };
        println!("  {marker}  {:<50} {:>5} lines", i.path, i.lines);
    }
    if !radius.unmatched_globs.is_empty() {
        println!("\n  matched no indexed file: {}", radius.unmatched_globs.join(", "));
    }
    Ok(0)
}
