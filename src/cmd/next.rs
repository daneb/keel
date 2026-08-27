//! `keel next` — tell the user what to do next.
//!
//! Inspects the pipeline state and prints actionable steps. When multiple
//! specs exist and no slug is given, shows the status of each and the next
//! action for every incomplete one. With a slug, focuses on that spec alone.

use crate::approval::{self, Standing};
use crate::config::Config;
use crate::gate::{self, Verdict};
use crate::paths::Paths;
use crate::plan::{Plan, Tasks};
use crate::spec::{self, Spec};
use anyhow::Result;

/// Where a spec is in the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    /// G0 has never run or is failing.
    Spec,
    /// G0 passes but spec is not approved.
    SpecApproval,
    /// Spec approved, no plan yet.
    Plan,
    /// Plan exists but G1 not passing.
    PlanGate,
    /// G1 passes but plan not approved.
    PlanApproval,
    /// Plan approved, work not done / G2 not passing.
    Run,
    /// Run passed, merge not approved.
    MergeApproval,
    /// All done.
    Complete,
}

impl Stage {
    fn label(&self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::SpecApproval => "approve spec",
            Self::Plan => "plan",
            Self::PlanGate => "G1",
            Self::PlanApproval => "approve plan",
            Self::Run => "run",
            Self::MergeApproval => "approve merge",
            Self::Complete => "done",
        }
    }
}

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
        let stage = pipeline_stage(&paths, slug);
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

/// Determine where a spec sits in the pipeline without printing anything.
fn pipeline_stage(paths: &Paths, slug: &str) -> Stage {
    // G0
    match gate::previous(paths, slug, "G0") {
        None | Some(gate::GateResult { verdict: Verdict::Fail, .. })
            | Some(gate::GateResult { verdict: Verdict::Blocked, .. }) => {
            return Stage::Spec;
        }
        _ => {}
    }

    // Spec approval
    if !matches!(approval::standing(paths, slug, "spec"), Ok(Standing::Current { .. })) {
        return Stage::SpecApproval;
    }

    // Plan exists
    if Plan::load(paths, slug).is_err() {
        return Stage::Plan;
    }

    // G1
    match gate::previous(paths, slug, "G1") {
        None | Some(gate::GateResult { verdict: Verdict::Fail, .. })
            | Some(gate::GateResult { verdict: Verdict::Blocked, .. }) => {
            return Stage::PlanGate;
        }
        _ => {}
    }

    // Plan approval
    if !matches!(approval::standing(paths, slug, "plan"), Ok(Standing::Current { .. })) {
        return Stage::PlanApproval;
    }

    // Run
    if !has_passing_run(paths, slug) {
        return Stage::Run;
    }

    // Merge approval — a superseded merge means the plan changed since the
    // last passing run, so the work needs to be redone, not just re-approved.
    match approval::standing(paths, slug, "merge") {
        Ok(Standing::Superseded { .. }) => return Stage::Run,
        Ok(Standing::Current { .. }) => {}
        _ => return Stage::MergeApproval,
    }

    Stage::Complete
}

/// Print the next action for a single spec (compact form for multi-spec view).
fn print_guidance(paths: &Paths, slug: &str) -> Result<()> {
    let stage = pipeline_stage(paths, slug);
    match stage {
        Stage::Spec => {
            let g0 = gate::previous(paths, slug, "G0");
            match g0 {
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
            let g1 = gate::previous(paths, slug, "G1");
            match g1 {
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
fn next_for_spec(paths: &Paths, slug: &str) -> Result<i32> {
    let _spec = Spec::load(paths, slug)?;

    // --- G0 ---------------------------------------------------------------
    let g0 = gate::previous(paths, slug, "G0");
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
    match approval::standing(paths, slug, "spec")? {
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
    if Plan::load(paths, slug).is_err() {
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
    let tasks = Tasks::load(paths, slug);
    let g1 = gate::previous(paths, slug, "G1");
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
            let hints = failure_hints(r);
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
    match approval::standing(paths, slug, "plan")? {
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
    if !has_passing_run(paths, slug) {
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
    match approval::standing(paths, slug, "merge")? {
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
                &format!("re-run `{slug}` — the plan changed since the last run"),
                &format!(
                    "The plan or tasks changed after the last passing run.\n\
                     Do the work again and gate it:\n\n\
                     \x20 keel run {slug}\n\
                     \x20 keel run {slug} --no-driver  # gate the tree as-is"
                ),
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
        if let Ok(results) = run.gate_results()
            && results.iter().any(|r| r.gate == "G2" && r.verdict == Verdict::Pass)
        {
            return true;
        }
    }
    false
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
    println!("▸ {title}\n");
    for line in detail.lines() {
        println!("  {line}");
    }
    println!();
}
