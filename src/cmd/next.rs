//! `keel next` — tell the user what to do next.
//!
//! Inspects the pipeline state for the active spec and prints a single
//! actionable step. The goal is to eliminate the "what command was I
//! supposed to run?" question — keel has many commands, and a human
//! learning them should not need to memorise the pipeline by heart.

use crate::approval::{self, Standing};
use crate::config::Config;
use crate::gate::{self, Verdict};
use crate::paths::Paths;
use crate::plan::{Plan, Tasks};
use crate::spec::{self, Spec};
use anyhow::Result;

pub fn run(slug: Option<String>) -> Result<i32> {
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

    // --- resolve slug -----------------------------------------------------
    let slug = match slug {
        Some(s) => s,
        None if all_specs.len() == 1 => all_specs[0].clone(),
        None => {
            // Pick the most "in-progress" spec: prefer one without a passing G1.
            let active = all_specs.iter().find(|s| {
                gate::previous(&paths, s, "G1")
                    .is_none_or(|r| r.verdict != Verdict::Pass)
            });
            match active {
                Some(s) => s.clone(),
                None => all_specs.last().unwrap().clone(),
            }
        }
    };

    let _spec = Spec::load(&paths, &slug)?;

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

    // --- G0 ---------------------------------------------------------------
    let g0 = gate::previous(&paths, &slug, "G0");
    match g0 {
        None => {
            step(
                &format!("run G0 on `{slug}`"),
                &format!(
                    "The spec has never been gated. Run:\n\n  keel gate g0 {slug}\n\n\
                     G0 checks EARS form, oracles, no placeholders, and the store."
                ),
            );
            return Ok(0);
        }
        Some(ref r) if r.verdict != Verdict::Pass => {
            let (_, f, b) = r.counts();
            step(
                &format!("fix `{slug}` spec — G0 has {f} failure(s), {b} blocked"),
                &format!(
                    "Edit `.keel/specs/{slug}/spec.md` to fix the failing checks,\n\
                     then re-run:\n\n  keel gate g0 {slug}"
                ),
            );
            return Ok(0);
        }
        _ => {} // G0 passes, continue
    }

    // --- spec approval ----------------------------------------------------
    match approval::standing(&paths, &slug, "spec")? {
        Standing::Current { .. } => {} // approved, move on
        Standing::Absent => {
            step(
                &format!("approve the `{slug}` spec"),
                &format!(
                    "G0 passes. A human must sign off before planning begins:\n\n\
                     \x20 keel approve {slug} --stage spec"
                ),
            );
            return Ok(0);
        }
        Standing::Rejected { by, note } => {
            step(
                &format!("spec was rejected by {by}"),
                &format!(
                    "Revise the spec and re-run G0, then re-approve.{}",
                    note.map(|n| format!("\n\nReason: {n}")).unwrap_or_default()
                ),
            );
            return Ok(0);
        }
        Standing::Superseded { .. } => {
            step(
                &format!("re-approve the `{slug}` spec"),
                &format!(
                    "The spec changed after it was approved. Re-run G0 and re-approve:\n\n\
                     \x20 keel gate g0 {slug}\n\
                     \x20 keel approve {slug} --stage spec"
                ),
            );
            return Ok(0);
        }
    }

    // --- plan exists? -----------------------------------------------------
    let plan = Plan::load(&paths, &slug);
    if plan.is_err() {
        step(
            &format!("create a plan for `{slug}`"),
            &format!(
                "The spec is approved. Compute the blast radius and scaffold tasks:\n\n\
                 \x20 keel plan {slug}\n\n\
                 Then fill in the approach, rollback, and each task's files/budget/exit."
            ),
        );
        return Ok(0);
    }

    // --- G1 ---------------------------------------------------------------
    let tasks = Tasks::load(&paths, &slug);
    let g1 = gate::previous(&paths, &slug, "G1");
    match g1 {
        None => {
            step(
                &format!("run G1 on `{slug}`"),
                &format!(
                    "A plan and tasks exist. Gate them:\n\n  keel gate g1 {slug}\n\n\
                     G1 checks traceability, budgets, exit conditions, blast radius,\n\
                     and spec approval."
                ),
            );
            return Ok(0);
        }
        Some(ref r) if r.verdict != Verdict::Pass => {
            let (_, f, b) = r.counts();
            let hints = g1_failure_hints(r);
            step(
                &format!("fix `{slug}` plan — G1 has {f} failure(s), {b} blocked"),
                &format!(
                    "Edit the plan/tasks to fix the failing checks, then re-run:\n\n\
                     \x20 keel gate g1 {slug}\n\n\
                     Failing checks:{hints}"
                ),
            );
            return Ok(0);
        }
        _ => {} // G1 passes
    }

    // --- plan approval ----------------------------------------------------
    match approval::standing(&paths, &slug, "plan")? {
        Standing::Current { .. } => {} // approved
        Standing::Absent => {
            step(
                &format!("approve the `{slug}` plan"),
                &format!(
                    "G1 passes. Sign off the plan before running:\n\n\
                     \x20 keel approve {slug} --stage plan"
                ),
            );
            return Ok(0);
        }
        Standing::Rejected { by, note } => {
            step(
                &format!("plan was rejected by {by}"),
                &format!(
                    "Revise the plan and tasks, re-run G1, then re-approve.{}",
                    note.map(|n| format!("\n\nReason: {n}")).unwrap_or_default()
                ),
            );
            return Ok(0);
        }
        Standing::Superseded { .. } => {
            step(
                &format!("re-approve the `{slug}` plan"),
                &format!(
                    "The plan or tasks changed after approval. Re-run G1 and re-approve:\n\n\
                     \x20 keel gate g1 {slug}\n\
                     \x20 keel approve {slug} --stage plan"
                ),
            );
            return Ok(0);
        }
    }

    // --- ready to run -----------------------------------------------------
    // If we get here, spec approved, plan approved, G1 passes. The user
    // should do the work (or let an agent do it) and gate the result.
    let has_run = crate::run::latest(&paths)?.is_some();
    if !has_run || !has_passing_run(&paths, &slug) {
        let tasks_info = if let Ok(ref t) = tasks {
            match t.waves() {
                Ok(w) => format!(" ({} wave(s), {} task(s))", w.len(), t.tasks.len()),
                Err(_) => String::new(),
            }
        } else {
            String::new()
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
        return Ok(0);
    }

    // --- post-run: merge approval -----------------------------------------
    match approval::standing(&paths, &slug, "merge")? {
        Standing::Current { .. } => {} // approved
        Standing::Absent => {
            step(
                &format!("approve the merge for `{slug}`"),
                &format!(
                    "The run passed. Review it and approve the merge:\n\n\
                     \x20 keel approve {slug} --stage merge"
                ),
            );
            return Ok(0);
        }
        Standing::Superseded { .. } => {
            step(
                &format!("re-approve the merge for `{slug}`"),
                "The plan or tasks changed after the merge was approved. Re-approve.",
            );
            return Ok(0);
        }
        _ => {}
    }

    // --- export and learn -------------------------------------------------
    step(
        &format!("`{slug}` is complete"),
        "The pipeline has been gated and approved. Optional next steps:\n\n\
         \x20 keel export           # write an evidence bundle\n\
         \x20 keel learn            # extract failure episodes and propose lessons\n\
         \x20 keel spec new <slug>  # start the next change",
    );
    Ok(0)
}

/// Check whether there is a run for this slug that passed G2.
fn has_passing_run(paths: &Paths, slug: &str) -> bool {
    let runs_dir = paths.runs();
    let Ok(entries) = std::fs::read_dir(&runs_dir) else { return false };
    for entry in entries.filter_map(|e| e.ok()) {
        let Ok(run) = crate::run::Run::load(paths, &entry.file_name().to_string_lossy()) else {
            continue;
        };
        if run.meta.spec != slug {
            continue;
        }
        if let Ok(results) = run.gate_results() {
            // A run that passed G2 (or G3) counts as a successful run.
            if results.iter().any(|r| r.gate == "G2" && r.verdict == Verdict::Pass) {
                return true;
            }
        }
    }
    false
}

/// Summarise the failing G1 checks into hints for the user.
fn g1_failure_hints(result: &gate::GateResult) -> String {
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
    println!("▸ {title}\n");
    for line in detail.lines() {
        println!("  {line}");
    }
    println!();
}
