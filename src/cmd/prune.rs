//! `keel runs prune` — bound the audit trail without breaking provenance.

use crate::paths::Paths;
use crate::run as runs;
use anyhow::Result;

pub fn prune(keep: usize, apply: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let candidates = runs::prune_plan(&paths, keep)?;
    let total = runs::list(&paths)?.len();

    if candidates.is_empty() {
        println!("  {total} run(s), all within the most recent {keep} — nothing to prune");
        return Ok(0);
    }

    let (removable, protected): (Vec<_>, Vec<_>) =
        candidates.iter().partition(|c| c.protected_by.is_none());

    for c in &removable {
        println!("  {} {:<8} {:>8}", c.id, c.verdict, human(c.bytes));
    }
    for c in &protected {
        println!(
            "  {} {:<8} {:>8}  KEPT — {}",
            c.id,
            c.verdict,
            human(c.bytes),
            c.protected_by.clone().unwrap_or_default()
        );
    }

    let freed: u64 = removable.iter().map(|c| c.bytes).sum();
    if !apply {
        println!(
            "\n  {} of {total} run(s) would be removed, freeing {}; {} kept for provenance",
            removable.len(),
            human(freed),
            protected.len()
        );
        println!("  re-run with --apply to do it");
        return Ok(0);
    }

    for c in &removable {
        let dir = paths.runs().join(&c.id);
        std::fs::remove_dir_all(&dir)?;
    }
    println!(
        "\n  removed {} run(s), freed {}; {} kept because a lesson cites them",
        removable.len(),
        human(freed),
        protected.len()
    );
    Ok(0)
}

fn human(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1}M", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0}K", b as f64 / (1u64 << 10) as f64),
        b => format!("{b}B"),
    }
}
