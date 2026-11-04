//! `keel review <slug>` — the brief a reviewer decides from, kept as run evidence.

use crate::approval;
use crate::config::Config;
use crate::gate::{self, diff};
use crate::paths::Paths;
use crate::run::Run;
use crate::spec::Spec;
use anyhow::{Context, Result};
use serde_json::Value;

pub fn run(slug: Option<String>) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    let id = approval::latest_run_for(&paths, &slug)
        .with_context(|| format!("no run for `{slug}` — `keel run` first, so there is something to review"))?;
    let run = Run::load(&paths, &id)?;
    let text = brief(&paths, &cfg, &run, &Spec::load(&paths, &slug)?)?;
    run.write_evidence(approval::BRIEF, &text)?;
    print!("{text}");
    Ok(0)
}

fn evidence_json(run: &Run, name: &str) -> Value {
    let raw = std::fs::read_to_string(run.evidence_dir().join(name)).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or(Value::Null)
}
fn brief(paths: &Paths, cfg: &Config, run: &Run, spec: &Spec) -> Result<String> {
    let reviewable = gate::g3::reviewable(paths, cfg, run);
    let lines = reviewable.map_or(String::new(), |(_, l)| format!("reviewable_lines: {l}\n"));
    // `generated:` and `reviewable_lines:` are read back by `approval::brief`.
    let mut s = format!("# Review brief — {} (run {})\n\ngenerated: {}\n{lines}", spec.front.slug, run.meta.id, chrono::Local::now().to_rfc3339());

    let (oracles, sensitivity) = (evidence_json(run, "oracles.json"), evidence_json(run, "oracle-sensitivity.json"));
    s.push_str("\n## Criteria\n\n");
    for r in oracles["runs"].as_array().into_iter().flatten() {
        let sens = sensitivity["runs"].as_array().into_iter().flatten()
            .find(|x| x["criterion"] == r["criterion"] && x["oracle"] == r["oracle"])
            .and_then(|x| x["assessment"].as_str())
            .unwrap_or("not assessed");
        let [c, o, v] = ["criterion", "oracle", "outcome"].map(|k| r[k].as_str().unwrap_or("?"));
        s.push_str(&format!("- {c} {v} — sensitivity: {sens} — {o}\n"));
    }

    let base = run.meta.base_commit.clone().unwrap_or_else(|| diff::default_base(paths));
    let files: Vec<String> = diff::against(paths, &base)?.files.into_iter()
        .filter(|f| !gate::g2::is_incidental_for(cfg, &f.path))
        .map(|f| f.path)
        .collect();
    s.push_str(&format!("\n## Blast radius\n\ndeclared: {}\ncomputed: {}\n", spec.front.scope.join(", "), files.join(", ")));

    s.push_str("\n## G2.5 findings\n\n");
    let findings: Vec<String> = run.gate_results()?.iter().filter(|g| g.gate == "G2.5")
        .flat_map(|g| g.checks.iter().filter(|c| c.verdict != gate::Verdict::Pass))
        .map(|c| format!("- {} ({}) {}\n", c.id, c.verdict.glyph(), c.detail.clone().or(c.actual.clone()).unwrap_or_default()))
        .collect();
    s.push_str(&if findings.is_empty() { "none raised\n".into() } else { findings.concat() });

    let events = crate::trajectory::read(&run.trajectory_path()).unwrap_or_default();
    s.push_str(&format!(
        "\n## Size and spend\n\nreviewable lines: {} of {}\ntokens spent (reported by drivers and reviewers): {}\n",
        reviewable.map_or("unavailable".into(), |(_, l)| l.to_string()),
        cfg.plan.max_reviewable_lines,
        crate::trajectory::usage_total(&events).tokens
    ));
    Ok(s)
}
