//! `keel report` — the whole life of a feature, in one place.

use crate::gate::Verdict;
use crate::paths::Paths;
use crate::report::Report;
use anyhow::Result;

pub fn run(slug: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let report = Report::build(&paths, slug.as_deref())?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(0);
    }

    if report.specs.is_empty() {
        println!("  no specs yet — `keel spec new <slug>`");
        return Ok(0);
    }

    for spec in &report.specs {
        println!(
            "{}  {}",
            crate::ui::bold(&spec.slug),
            crate::ui::dim(spec.stage)
        );

        for g in &spec.gates {
            let (p, f, b) = g.counts();
            println!(
                "  {:<6} {}  {}",
                g.gate,
                g.verdict.glyph_styled(),
                crate::ui::dim(&format!("{p} passed, {f} failed, {b} blocked"))
            );
        }

        for stage in crate::approval::STAGES {
            if let Some(s) = spec.approvals.get(*stage)
                && let Some(state) = s.get("state").and_then(|v| v.as_str())
                && state != "absent"
            {
                println!("  {:<6} {}", stage, styled_state(state));
            }
        }

        for r in &spec.runs {
            let verdict = match r.meta.verdict.as_deref() {
                Some("pass") => Verdict::Pass.glyph_styled(),
                Some("fail") => Verdict::Fail.glyph_styled(),
                Some("blocked") => Verdict::Blocked.glyph_styled(),
                _ => crate::ui::dim("…"),
            };
            println!(
                "  {:<20} {}  {}",
                r.meta.id,
                verdict,
                crate::ui::dim(&format!("{} events, {} tokens", r.events, r.tokens))
            );
            // Every failing check, with what it wanted and what it got.
            for g in &r.gates {
                for c in g.checks.iter().filter(|c| c.verdict != Verdict::Pass) {
                    println!("    {}", c.line().trim_start());
                }
            }
            if !r.anomalies.is_empty() {
                println!(
                    "    {}",
                    crate::ui::yellow(&format!(
                        "{} trajectory event(s) could not be read",
                        r.anomalies.len()
                    ))
                );
            }
        }
        println!();
    }
    Ok(0)
}

fn styled_state(state: &str) -> String {
    match state {
        "current" => crate::ui::green(state),
        "rejected" | "superseded" => crate::ui::red(state),
        _ => crate::ui::yellow(state),
    }
}
