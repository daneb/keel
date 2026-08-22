//! `keel map` — rebuild the structural index and the generated maps.

use crate::config::Config;
use crate::map::MapReport;
use crate::paths::Paths;
use anyhow::Result;

pub fn run(budget: Option<usize>, json: bool) -> Result<()> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let report = crate::map::build(&paths, &cfg, budget)?;

    if json {
        println!("{}", serde_json::json!({
            "files": report.files,
            "symbols": report.symbols,
            "edges": report.edges,
            "unparsed": report.unparsed,
            "elapsed_ms": report.elapsed_ms,
            "structure_lines": report.structure_lines,
            "structure_budget": report.structure_budget,
            "codemaps": report.codemaps,
            "degraded": report.degraded.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
        }));
    } else {
        print_report(&report);
    }
    Ok(())
}

pub fn print_report(r: &MapReport) {
    let langs = r.languages.iter()
        .map(|(l, n)| format!("{l} {n}"))
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "  {} files · {} symbols · {} import edges  ({} ms)",
        r.files, r.symbols, r.edges, r.elapsed_ms
    );
    if !langs.is_empty() {
        println!("  {langs}");
    }
    println!(
        "  structure.md {}/{} lines · {} CODEMAP{}",
        r.structure_lines,
        r.structure_budget,
        r.codemaps,
        if r.codemaps == 1 { "" } else { "s" }
    );
    for (lang, err) in &r.degraded {
        println!("  ! {lang}: symbol queries did not compile — indexed without symbols ({err})");
    }
    if r.unparsed > 0 {
        println!(
            "  {} file{} indexed as metadata only (parse errors or over size limit)",
            r.unparsed,
            if r.unparsed == 1 { "" } else { "s" }
        );
    }
}
