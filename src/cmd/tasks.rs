//! `keel tasks` — the plan as an execution order.
//!
//! Waves say what *could* proceed together, which is not the same as what keel
//! will run together. Two agents editing one working tree concurrently is a bug
//! factory, so keel reports the waves and runs tasks one at a time; genuine
//! parallelism needs a worktree per task, which is not built.
//!
//! Saying that plainly is better than a `--parallel` flag that quietly
//! serialises, or one that does not and corrupts the tree.

use crate::paths::Paths;
use crate::plan::Tasks;
use anyhow::Result;

pub fn run(slug: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    let tasks = Tasks::load(&paths, &slug)?;

    let dangling = tasks.dangling_dependencies();
    let waves = tasks.waves();

    if json {
        let payload = match &waves {
            Ok(w) => serde_json::json!({
                "spec": slug,
                "waves": w.iter().map(|wave| {
                    wave.iter().map(|t| serde_json::json!({
                        "id": t.id,
                        "title": t.title,
                        "criteria": t.criteria,
                        "budget": t.budget,
                        "depends_on": t.depends_on,
                    })).collect::<Vec<_>>()
                }).collect::<Vec<_>>(),
                "dangling": dangling,
            }),
            Err(stuck) => serde_json::json!({ "spec": slug, "cycle": stuck }),
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(if waves.is_err() { 1 } else { 0 });
    }

    let waves = match waves {
        Ok(w) => w,
        Err(stuck) => {
            println!("  dependency cycle among: {}", stuck.join(", "));
            println!("  a plan that cannot be ordered is not a plan — break the cycle");
            return Ok(1);
        }
    };

    println!("{slug} — {} task(s) in {} wave(s)\n", tasks.tasks.len(), waves.len());
    for (n, wave) in waves.iter().enumerate() {
        println!("wave {} — {} task(s) with no dependency on each other", n + 1, wave.len());
        for t in wave {
            println!(
                "  {:<6} {:<40} {:>4} lines  {}",
                t.id,
                crate::gate::truncate(&t.title, 38),
                t.budget.unwrap_or(0),
                if t.depends_on.is_empty() {
                    String::new()
                } else {
                    format!("after {}", t.depends_on.join(", "))
                }
            );
        }
        println!();
    }

    if !dangling.is_empty() {
        println!("  dependencies naming no such task: {}", dangling.join(", "));
    }
    let widest = waves.iter().map(|w| w.len()).max().unwrap_or(0);
    if widest > 1 {
        println!(
            "  up to {widest} task(s) could proceed together. keel runs them one at a time:\n  \
             concurrent agents on one working tree corrupt it. Give each a git worktree\n  \
             if you want the parallelism."
        );
    }
    Ok(0)
}
