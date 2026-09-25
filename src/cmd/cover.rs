//! `keel cover` — is the working tree's content covered by a verified bundle
//! of a passing run? Built for a pull request check (SPEC-0012).

use crate::config::Config;
use crate::paths::Paths;
use anyhow::Result;

pub fn run(exempt: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let report = crate::cover::check(&paths, &cfg, exempt)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(report.exit_code());
    }
    println!("head {}\n", report.head);
    if report.bundles.is_empty() {
        println!("  no bundles under .keel/bundles/");
    }
    for b in &report.bundles {
        let mark = if b.covers { "covers " } else { "does not" };
        println!("  {mark}  {} — {}", b.bundle, b.reason);
    }
    match report.verdict.as_str() {
        "covered" => println!("\ncovered"),
        "exempted" => println!(
            "\nexempted, not covered — {}",
            report.exempt_reason.as_deref().unwrap_or("no reason given")
        ),
        _ => println!("\nuncovered — commit the bundle of a passing run of exactly this tree"),
    }
    Ok(report.exit_code())
}
