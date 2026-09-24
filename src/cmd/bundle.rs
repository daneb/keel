//! `keel bundle verify` — check a bundle using nothing but the bundle.
//!
//! Deliberately never discovers a repository or reads config: an auditor runs
//! this on a machine that has only the archive and keel's binary.

use crate::evidence::verify::check;
use anyhow::Result;

pub fn verify(archive: String, json: bool) -> Result<i32> {
    let report = check(std::path::Path::new(&archive))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "bundle {} — run {}, spec {}\n",
            report.archive,
            report.run.as_deref().unwrap_or("?"),
            report.spec.as_deref().unwrap_or("?"),
        );
        for c in &report.checks {
            println!("{}", c.line());
        }
        println!("\nbundle {}", report.verdict.glyph_styled());
    }
    Ok(report.verdict.exit_code())
}
