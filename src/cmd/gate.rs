//! `keel gate g0|g1` — run a gate, record the verdict, exit with it.

use crate::config::Config;
use crate::gate::{self, GateResult};
use crate::paths::Paths;
use crate::plan::{Plan, Tasks};
use crate::spec::Spec;
use anyhow::Result;

pub fn g0(slug: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let slug = resolve_slug(&paths, slug)?;
    let spec = Spec::load(&paths, &slug)?;
    let result = gate::g0::run(&paths, &cfg, &spec)?;
    report(&paths, &slug, &result, json)
}

pub fn g1(slug: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let slug = resolve_slug(&paths, slug)?;
    let spec = Spec::load(&paths, &slug)?;
    let plan = Plan::load(&paths, &slug)?;
    let tasks = Tasks::load(&paths, &slug)?;
    let result = gate::g1::run(&paths, &cfg, &spec, &plan, &tasks)?;
    report(&paths, &slug, &result, json)
}

fn report(paths: &Paths, slug: &str, result: &GateResult, json: bool) -> Result<i32> {
    let dir = gate::dir_for(paths, slug);
    let written = result.write(&dir)?;

    if json {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(result.verdict.exit_code());
    }

    println!("{} — {} (run {})\n", result.gate, slug, result.run);
    for check in &result.checks {
        println!("{}", check.line());
    }
    let (p, f, b) = result.counts();
    println!(
        "\n{} {} — {p} passed, {f} failed, {b} blocked",
        result.gate,
        result.verdict.glyph()
    );
    println!("evidence: {}", paths.rel(&written).display());

    if result.verdict == gate::Verdict::Blocked {
        println!("\nblocked is not failed: a check could not run. Fix the environment, not the spec.");
    }
    Ok(result.verdict.exit_code())
}

/// Fall back to the only spec present, so the common case needs no argument.
pub fn resolve_slug(paths: &Paths, slug: Option<String>) -> Result<String> {
    if let Some(s) = slug {
        return Ok(s);
    }
    let all = crate::spec::list(paths)?;
    match all.len() {
        0 => anyhow::bail!("no specs yet — run `keel spec new <slug>`"),
        1 => Ok(all[0].clone()),
        _ => anyhow::bail!(
            "several specs exist ({}) — name the one you mean",
            all.join(", ")
        ),
    }
}
