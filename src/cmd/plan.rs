//! `keel plan <slug>` — compute the blast radius and scaffold plan + tasks.

use crate::config::Config;
use crate::map::blast;
use crate::map::db::Index;
use crate::paths::Paths;
use crate::plan::{self, Plan};
use crate::spec::Spec;
use anyhow::{Result, bail};

pub fn run(slug: Option<String>, depth: Option<usize>) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    let spec = Spec::load(&paths, &slug)?;

    if spec.front.scope.is_empty() {
        bail!("spec `{slug}` declares no scope — G0 would reject it, and a blast radius cannot be computed without one");
    }
    let db = paths.index_db();
    if !db.exists() {
        bail!("no symbol index — run `keel map` first; the blast radius is computed from it, not guessed");
    }

    let index = Index::open(&db)?;
    let depth = depth.unwrap_or(cfg.plan.blast_depth);
    let radius = blast::compute(&index, &spec.front.scope, depth)?;

    // Re-running `keel plan` must refresh the computed radius without eating
    // the design prose a human wrote around it.
    let plan_path = Plan::path_for(&paths, &slug);
    let existing = Plan::load(&paths, &slug).ok();
    let rendered = plan::render_plan(&spec, &radius, existing.as_ref())?;
    std::fs::create_dir_all(plan_path.parent().unwrap())?;
    std::fs::write(&plan_path, rendered)?;
    println!(
        "  {} {}",
        if existing.is_some() { "updated" } else { "created" },
        paths.rel(&plan_path).display()
    );

    let tasks_path = crate::plan::Tasks::path_for(&paths, &slug);
    if tasks_path.exists() {
        println!("  kept    {} (edit it yourself; keel does not overwrite tasks)",
            paths.rel(&tasks_path).display());
    } else {
        std::fs::write(&tasks_path, plan::render_tasks(&spec)?)?;
        println!("  created {}", paths.rel(&tasks_path).display());
    }

    println!(
        "\n  blast radius at depth {}: {} file(s), {} lines",
        depth,
        radius.impact.len(),
        radius.impact_lines
    );
    let beyond = radius.beyond_scope();
    if !beyond.is_empty() {
        println!("  {} file(s) outside the declared scope depend on it:", beyond.len());
        for i in beyond.iter().take(8) {
            println!("    +{} {}", i.depth, i.path);
        }
        if beyond.len() > 8 {
            println!("    … {} more", beyond.len() - 8);
        }
    }
    if !radius.unmatched_globs.is_empty() {
        println!(
            "  scope globs matching no indexed file: {} (new files, or a typo)",
            radius.unmatched_globs.join(", ")
        );
    }

    println!("\n  next: fill in the approach, rollback and tasks, then `keel gate g1 {slug}`");
    Ok(0)
}
