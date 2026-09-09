//! `keel next` — tell the user what to do next.
//!
//! Inspects the pipeline state and prints actionable steps. When multiple
//! specs exist and no slug is given, shows the status of each and the next
//! action for every incomplete one. With a slug, focuses on that spec alone.

use crate::approval::Standing;
use crate::config::Config;
use crate::gate::{self, Verdict};
use crate::paths::Paths;
use crate::pipeline::{self, Position, Stage};
use crate::plan::Tasks;
use crate::spec::{self, Spec};
use anyhow::Result;

/// The one command that moves this spec forward from where it stands.
fn command_for(slug: &str, stage: Stage) -> String {
    match stage {
        Stage::Spec => format!("keel gate g0 {slug}"),
        Stage::SpecApproval => format!("keel approve {slug} --stage spec"),
        Stage::Plan => format!("keel plan {slug}"),
        Stage::PlanGate => format!("keel gate g1 {slug}"),
        Stage::PlanApproval => format!("keel approve {slug} --stage plan"),
        Stage::Run => format!("keel run {slug}"),
        Stage::MergeApproval => format!("keel approve {slug} --stage merge"),
        Stage::Complete => "keel spec new <slug>".to_string(),
    }
}

/// `keel next --json`.
///
/// Repo-level obstacles land in `blockers` rather than replacing the answer, so
/// a caller always gets the same shape and can decide for itself whether a
/// drifted store is worth stopping for.
fn run_json(slug: Option<String>) -> Result<i32> {
    let mut blockers: Vec<serde_json::Value> = Vec::new();
    let mut specs: Vec<serde_json::Value> = Vec::new();

    if let Ok(paths) = Paths::require_init() {
        let cfg = Config::load(&paths.config())?;
        let store_hash = crate::store::store_hash_with_shared(&paths, &cfg)?;
        if crate::projection::drift::check_all(&paths, &cfg, &store_hash)?
            .iter()
            .any(|r| r.state.is_blocking())
        {
            blockers.push(serde_json::json!({
                "reason": "store drift",
                "command": "keel store render",
            }));
        }

        let slugs = match slug {
            Some(s) => vec![s],
            None => spec::list(&paths)?,
        };
        if slugs.is_empty() {
            blockers.push(serde_json::json!({
                "reason": "no specs",
                "command": "keel spec new <slug>",
            }));
        }
        for s in slugs {
            let stage = pipeline::stage(&paths, &s);
            specs.push(serde_json::json!({
                "slug": s,
                "stage": stage.key(),
                "command": command_for(&s, stage),
                "complete": stage == Stage::Complete,
            }));
        }
    } else {
        blockers.push(serde_json::json!({
            "reason": "not initialised",
            "command": "keel init",
        }));
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "keel.next/1",
            "blockers": blockers,
            "specs": specs,
        }))?
    );
    Ok(0)
}

pub fn run(slug: Option<String>, json: bool) -> Result<i32> {
    if json {
        return run_json(slug);
    }
    let paths = match Paths::require_init() {
        Ok(p) => p,
        Err(_) => {
            step(
                "initialise keel",
                "Run `keel init` to scaffold .keel/ and build the first map.",
            );
            return Ok(0);
        }
    };

    let cfg = Config::load(&paths.config())?;

    // --- no specs at all --------------------------------------------------
    let all_specs = spec::list(&paths)?;
    if all_specs.is_empty() {
        step(
            "write your first spec",
            "Run `keel spec new <slug>` to scaffold a spec. It will fail G0\n\
             immediately — that is the point. Fill in the criteria until it passes.",
        );
        return Ok(0);
    }

    // --- store drift? quick pre-flight ------------------------------------
    let store_hash = crate::store::store_hash_with_shared(&paths, &cfg)?;
    let drift_reports = crate::projection::drift::check_all(&paths, &cfg, &store_hash)?;
    let drifted = drift_reports.iter().any(|r| r.state.is_blocking());
    if drifted {
        step(
            "fix store drift",
            "Projections are out of date. Run `keel store render` to regenerate them.\n\
             If a generated file was hand-edited, run `keel store reconcile` first.",
        );
        return Ok(0);
    }

    // --- single slug: focused mode ----------------------------------------
    if let Some(s) = slug {
        return next_for_spec(&paths, &s);
    }

    // --- multiple specs: overview + guidance for each ----------------------
    if all_specs.len() == 1 {
        return next_for_spec(&paths, &all_specs[0]);
    }

    // Show a summary table, then detail for each incomplete spec.
    println!("specs:\n");
    let mut stages: Vec<(&str, Stage)> = Vec::new();
    for slug in &all_specs {
        let stage = pipeline::stage(&paths, slug);
        println!("  {:<28} {}", slug, stage.label());
        stages.push((slug, stage));
    }
    println!();

    let incomplete: Vec<&str> = stages.iter()
        .filter(|(_, s)| *s != Stage::Complete)
        .map(|(slug, _)| *slug)
        .collect();

    if incomplete.is_empty() {
        step(
            "all specs complete",
            "Every spec has been gated and approved. Start the next change:\n\n\
             \x20 keel spec new <slug>",
        );
        return Ok(0);
    }

    for slug in &incomplete {
        print_guidance(&paths, slug)?;
    }
    Ok(0)
}

/// Print the next action for a single spec (compact form for multi-spec view).
fn print_guidance(paths: &Paths, slug: &str) -> Result<()> {
    let pos = pipeline::position(paths, slug);
    match pos.stage {
        Stage::Spec => {
            match pos.g0 {
                None => step(
                    &format!("[{slug}] run G0"),
                    &format!("  keel gate g0 {slug}"),
                ),
                Some(r) => {
                    let (_, f, b) = r.counts();
                    let hints = failure_hints(&r);
                    step(
                        &format!("[{slug}] fix spec — G0 has {f} failure(s), {b} blocked"),
                        &format!("  Edit `.keel/specs/{slug}/spec.md`, then: keel gate g0 {slug}{hints}"),
                    );
                }
            }
        }
        Stage::SpecApproval => {
            step(
                &format!("[{slug}] approve spec"),
                &format!("  keel approve {slug} --stage spec"),
            );
        }
        Stage::Plan => {
            step(
                &format!("[{slug}] create a plan"),
                &format!("  keel plan {slug}"),
            );
        }
        Stage::PlanGate => {
            match pos.g1 {
                None => step(
                    &format!("[{slug}] run G1"),
                    &format!("  keel gate g1 {slug}"),
                ),
                Some(r) => {
                    let (_, f, b) = r.counts();
                    let hints = failure_hints(&r);
                    step(
                        &format!("[{slug}] fix plan — G1 has {f} failure(s), {b} blocked"),
                        &format!("  Edit plan/tasks, then: keel gate g1 {slug}{hints}"),
                    );
                }
            }
        }
        Stage::PlanApproval => {
            step(
                &format!("[{slug}] approve plan"),
                &format!("  keel approve {slug} --stage plan"),
            );
        }
        Stage::Run => {
            step(
                &format!("[{slug}] do the work"),
                &format!(
                    "  keel run {slug}              # drive an agent\n\
                     \x20 keel run {slug} --no-driver  # gate the tree as-is"
                ),
            );
        }
        Stage::MergeApproval => {
            step(
                &format!("[{slug}] approve merge"),
                &format!("  keel approve {slug} --stage merge"),
            );
        }
        Stage::Complete => {
            // Should not reach here in the incomplete list, but handle gracefully.
            step(&format!("[{slug}] complete"), "  Nothing to do.");
        }
    }
    Ok(())
}

/// Focused single-spec mode with full detail.
///
/// Every arm renders from the single [`Position`] evaluated up front, so the
/// stage named in the multi-spec listing and the guidance printed here cannot
/// disagree about what is blocking.
fn next_for_spec(paths: &Paths, slug: &str) -> Result<i32> {
    let _spec = Spec::load(paths, slug)?;
    let pos = pipeline::position(paths, slug);
    match pos.stage {
        Stage::Spec => spec_guidance(slug, &pos),
        Stage::SpecApproval => spec_approval_guidance(slug, &pos),
        Stage::Plan => plan_guidance(slug),
        Stage::PlanGate => plan_gate_guidance(slug, &pos),
        Stage::PlanApproval => plan_approval_guidance(slug, &pos),
        Stage::Run => run_guidance(paths, slug, &pos),
        Stage::MergeApproval => merge_approval_guidance(slug, &pos),
        Stage::Complete => complete_guidance(slug),
    }
    Ok(0)
}

fn spec_guidance(slug: &str, pos: &Position) {
    match &pos.g0 {
        None => {
            step(
                &format!("run G0 on `{slug}`"),
                &format!(
                    "The spec has never been gated. Run:\n\n  keel gate g0 {slug}\n\n\
                     G0 checks EARS form, oracles, no placeholders, and the store."
                ),
            );
        }
        Some(r) => {
            let (_, f, b) = r.counts();
            step(
                &format!("fix `{slug}` spec — G0 has {f} failure(s), {b} blocked"),
                &format!(
                    "Edit `.keel/specs/{slug}/spec.md` to fix the failing checks,\n\
                     then re-run:\n\n  keel gate g0 {slug}"
                ),
            );
        }
    }
}

fn spec_approval_guidance(slug: &str, pos: &Position) {
    match &pos.spec_approval {
        Standing::Rejected { by, note } => step(
            &format!("spec was rejected by {by}"),
            &format!(
                "Revise the spec and re-run G0, then re-approve.{}",
                note.as_ref().map(|n| format!("\n\nReason: {n}")).unwrap_or_default()
            ),
        ),
        Standing::Superseded { .. } => step(
            &format!("re-approve the `{slug}` spec"),
            &format!(
                "The spec changed after it was approved. Re-run G0 and re-approve:\n\n\
                 \x20 keel gate g0 {slug}\n\
                 \x20 keel approve {slug} --stage spec"
            ),
        ),
        _ => step(
            &format!("approve the `{slug}` spec"),
            &format!(
                "G0 passes. A human must sign off before planning begins:\n\n\
                 \x20 keel approve {slug} --stage spec"
            ),
        ),
    }
}

fn plan_guidance(slug: &str) {
    step(
        &format!("create a plan for `{slug}`"),
        &format!(
            "The spec is approved. Compute the blast radius and scaffold tasks:\n\n\
             \x20 keel plan {slug}\n\n\
             Then fill in the approach, rollback, and each task's files/budget/exit."
        ),
    );
}

fn plan_gate_guidance(slug: &str, pos: &Position) {
    match &pos.g1 {
        None => step(
            &format!("run G1 on `{slug}`"),
            &format!(
                "A plan and tasks exist. Gate them:\n\n  keel gate g1 {slug}\n\n\
                 G1 checks traceability, budgets, exit conditions, blast radius,\n\
                 and spec approval."
            ),
        ),
        // A G1 that passed but predates the spec's current approval judged a
        // spec that has since changed.
        Some(_) if pos.g1_stale => step(
            &format!("re-run G1 on `{slug}` — the spec changed since G1 last passed"),
            &format!(
                "The spec was re-approved after G1 ran. Update the plan and tasks\n\
                 if needed, then re-run:\n\n  keel gate g1 {slug}"
            ),
        ),
        Some(r) => {
            let (_, f, b) = r.counts();
            let hints = failure_hints(r);
            step(
                &format!("fix `{slug}` plan — G1 has {f} failure(s), {b} blocked"),
                &format!(
                    "Edit the plan/tasks to fix the failing checks, then re-run:\n\n\
                     \x20 keel gate g1 {slug}\n\n\
                     Failing checks:{hints}"
                ),
            );
        }
    }
}

fn plan_approval_guidance(slug: &str, pos: &Position) {
    match &pos.plan_approval {
        Standing::Rejected { by, note } => step(
            &format!("plan was rejected by {by}"),
            &format!(
                "Revise the plan and tasks, re-run G1, then re-approve.{}",
                note.as_ref().map(|n| format!("\n\nReason: {n}")).unwrap_or_default()
            ),
        ),
        Standing::Superseded { .. } => step(
            &format!("re-approve the `{slug}` plan"),
            &format!(
                "The plan or tasks changed after approval. Re-run G1 and re-approve:\n\n\
                 \x20 keel gate g1 {slug}\n\
                 \x20 keel approve {slug} --stage plan"
            ),
        ),
        _ => step(
            &format!("approve the `{slug}` plan"),
            &format!(
                "G1 passes. Sign off the plan before running:\n\n\
                 \x20 keel approve {slug} --stage plan"
            ),
        ),
    }
}

fn run_guidance(paths: &Paths, slug: &str, pos: &Position) {
    // A passing run already exists, so the only way to be back here is a merge
    // approval that went stale — the agreed shape of the work changed.
    if pos.passing_run.is_some() {
        step(
            &format!("re-run `{slug}` — the plan changed since the last run"),
            &format!(
                "The plan or tasks changed after the last passing run.\n\
                 Do the work again and gate it:\n\n\
                 \x20 keel run {slug}\n\
                 \x20 keel run {slug} --no-driver  # gate the tree as-is"
            ),
        );
        return;
    }

    let tasks_info = match Tasks::load(paths, slug) {
        Ok(t) => match t.waves() {
            Ok(w) => format!(" ({} wave(s), {} task(s))", w.len(), t.tasks.len()),
            Err(_) => String::new(),
        },
        Err(_) => String::new(),
    };
    step(
        &format!("do the work for `{slug}`"),
        &format!(
            "Everything is approved. Make the change, then gate it:\n\n\
             \x20 keel run {slug}              # drive an agent and gate the result\n\
             \x20 keel run {slug} --no-driver  # gate the working tree as-is\n\
             \x20 keel run {slug} --waves      # one worktree per task{tasks_info}\n\n\
             G2 will run build/test/lint and every oracle."
        ),
    );
}

fn merge_approval_guidance(slug: &str, pos: &Position) {
    match &pos.merge_approval {
        Standing::Rejected { by, note } => step(
            &format!("the merge was rejected by {by}"),
            &format!(
                "Address the reason, re-run `keel run {slug}`, then re-approve.{}",
                note.as_ref().map(|n| format!("\n\nReason: {n}")).unwrap_or_default()
            ),
        ),
        _ => step(
            &format!("approve the merge for `{slug}`"),
            &format!(
                "The run passed. Review it and approve the merge:\n\n\
                 \x20 keel approve {slug} --stage merge"
            ),
        ),
    }
}

fn complete_guidance(slug: &str) {
    step(
        &format!("`{slug}` is complete"),
        "The pipeline has been gated and approved. Optional next steps:\n\n\
         \x20 keel export           # write an evidence bundle\n\
         \x20 keel learn            # extract failure episodes and propose lessons\n\
         \x20 keel spec new <slug>  # start the next change",
    );
}

/// Summarise failing checks into hints.
fn failure_hints(result: &gate::GateResult) -> String {
    let mut hints = String::new();
    for check in &result.checks {
        if check.verdict == Verdict::Fail {
            let msg = check.actual.as_deref()
                .or(check.detail.as_deref())
                .unwrap_or("(no detail)");
            hints.push_str(&format!("\n  • {}: {msg}", check.id));
        }
    }
    hints
}

fn step(title: &str, detail: &str) {
    println!("{}\n", crate::ui::bold(&format!("▸ {title}")));
    for line in detail.lines() {
        println!("  {line}");
    }
    println!();
}
