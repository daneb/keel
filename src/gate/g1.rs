//! **G1 — is this plan bounded?**
//!
//! Checks (PLAN.md §4.4): every task ↦ ≥1 criterion; blast radius computed
//! from the map and declared; per-task line budget set; rollback stated; no
//! task without an exit condition.
//!
//! Two checks here are load-bearing beyond the letter of the plan:
//!
//! * `blast-radius-current` recomputes the radius and fails if the recorded one
//!   has gone stale. A radius computed against last week's import graph is a
//!   guess wearing a computation's clothes.
//! * `spec-approved` fails when the spec changed after it was approved, so a
//!   sign-off cannot be inherited by a spec nobody agreed to.

use super::{Check, GateResult, run_plugins};
use crate::approval::{self, Standing};
use crate::config::Config;
use crate::map::blast;
use crate::map::db::Index;
use crate::paths::Paths;
use crate::plan::{Plan, Tasks};
use crate::spec::Spec;
use crate::spec::placeholder::is_placeholder_value as is_placeholder;
use anyhow::Result;
use globset::{Glob, GlobSetBuilder};

pub fn run(
    paths: &Paths,
    cfg: &Config,
    spec: &Spec,
    plan: &Plan,
    tasks: &Tasks,
) -> Result<GateResult> {
    let mut checks = vec![
        schema(plan, tasks),
        g0_passed(paths, &spec.front.slug),
        spec_approved(paths, &spec.front.slug)?,
        tasks_present(tasks, cfg),
        task_ids_unique(tasks),
        unknown_task_fields(tasks),
        task_criterion_traceability(spec, tasks),
        criteria_covered(spec, tasks),
        task_budgets(tasks, cfg),
        total_budget(spec, tasks),
        task_exit_conditions(tasks),
        task_dependencies(tasks),
        wave_isolation(tasks),
        task_files_in_scope(spec, tasks),
        rollback_stated(plan),
        blast_radius_declared(plan),
        blast_radius_current(paths, cfg, spec, plan)?,
    ];
    checks.extend(run_plugins(paths, cfg, "G1", Some(&spec.front.slug)));
    Ok(GateResult::new("G1", Some(spec.front.slug.clone()), checks))
}

fn schema(plan: &Plan, tasks: &Tasks) -> Check {
    if plan.front.schema != crate::plan::PLAN_SCHEMA {
        return Check::fail("schema", crate::plan::PLAN_SCHEMA, plan.front.schema.clone());
    }
    if tasks.front.schema != crate::plan::TASKS_SCHEMA {
        return Check::fail("schema", crate::plan::TASKS_SCHEMA, tasks.front.schema.clone());
    }
    Check::pass("schema", format!("{} + {}", plan.front.id, tasks.front.id))
}

/// Stage ordering: a plan cannot be sound if its spec was never buildable.
fn g0_passed(paths: &Paths, slug: &str) -> Check {
    match super::previous(paths, slug, "G0") {
        None => Check::fail("g0-passed", "a recorded G0 verdict", "G0 has never been run — run `keel gate g0`"),
        Some(r) if r.verdict == super::Verdict::Pass => {
            Check::pass("g0-passed", format!("G0 passed in run {}", r.run))
        }
        Some(r) => Check::fail(
            "g0-passed",
            "G0 verdict is pass",
            format!("G0 is {} (run {})", r.verdict.glyph(), r.run),
        ),
    }
}

fn spec_approved(paths: &Paths, slug: &str) -> Result<Check> {
    Ok(match approval::standing(paths, slug, "spec")? {
        Standing::Current { by, .. } => Check::pass("spec-approved", format!("approved by {by}")),
        Standing::Absent => Check::fail(
            "spec-approved",
            "a recorded human approval of the spec",
            format!("none — run `keel approve {slug} --stage spec`"),
        ),
        Standing::Rejected { by, note } => Check::fail(
            "spec-approved",
            "the spec is approved",
            format!("rejected by {by}{}", note.map(|n| format!(": {n}")).unwrap_or_default()),
        ),
        Standing::Superseded { approved_hash, current_hash } => Check::fail(
            "spec-approved",
            "the approved spec is the current spec",
            format!("spec changed after approval ({approved_hash} → {current_hash}) — re-approve it"),
        ),
    })
}

fn tasks_present(tasks: &Tasks, cfg: &Config) -> Check {
    if tasks.tasks.is_empty() {
        return Check::fail("tasks-present", "at least one `### T-n` task", "none found");
    }
    if tasks.tasks.len() > cfg.plan.max_tasks {
        return Check::fail(
            "tasks-present",
            format!("at most {} tasks", cfg.plan.max_tasks),
            format!("{} tasks — split the spec", tasks.tasks.len()),
        );
    }
    Check::pass("tasks-present", format!("{}/{} tasks", tasks.tasks.len(), cfg.plan.max_tasks))
}

fn task_ids_unique(tasks: &Tasks) -> Check {
    let mut seen: Vec<&str> = Vec::new();
    let mut dupes: Vec<&str> = Vec::new();
    for t in &tasks.tasks {
        if seen.contains(&t.id.as_str()) {
            if !dupes.contains(&t.id.as_str()) { dupes.push(&t.id); }
        } else {
            seen.push(&t.id);
        }
    }
    if dupes.is_empty() {
        Check::pass("task-ids-unique", "no duplicate task ids")
    } else {
        Check::fail("task-ids-unique", "unique task ids", format!("duplicated: {}", dupes.join(", ")))
    }
}

/// A misspelled field parses as "no field at all", which would otherwise show
/// up as a confusing failure in a different check.
fn unknown_task_fields(tasks: &Tasks) -> Check {
    let bad: Vec<String> = tasks
        .tasks
        .iter()
        .flat_map(|t| t.unknown_fields.iter().map(move |f| format!("{}: `{f}`", t.id)))
        .collect();
    if bad.is_empty() {
        Check::pass("task-fields", "all task fields recognised")
    } else {
        Check::fail(
            "task-fields",
            "task fields are criteria, files, budget, exit",
            format!("unrecognised: {}", bad.join(", ")),
        )
    }
}

fn task_criterion_traceability(spec: &Spec, tasks: &Tasks) -> Check {
    let known: Vec<&str> = spec.criteria.iter().map(|c| c.id.as_str()).collect();
    let mut orphans: Vec<String> = Vec::new();
    let mut dangling: Vec<String> = Vec::new();
    for t in &tasks.tasks {
        if t.criteria.is_empty() {
            orphans.push(format!("{} \u{201c}{}\u{201d} (line {})", t.id, t.title, t.line));
        }
        for c in &t.criteria {
            if !known.contains(&c.as_str()) {
                dangling.push(format!("{} → {c}", t.id));
            }
        }
    }
    if !orphans.is_empty() {
        return Check::fail(
            "task-criterion-traceability",
            "every task names at least one criterion",
            format!("no criteria on: {}", super::join_capped(&orphans, 5)),
        );
    }
    if !dangling.is_empty() {
        return Check::fail(
            "task-criterion-traceability",
            "every referenced criterion exists in the spec",
            format!("unknown: {}", super::join_capped(&dangling, 5)),
        );
    }
    Check::pass("task-criterion-traceability", format!("{} tasks all traced", tasks.tasks.len()))
}

fn criteria_covered(spec: &Spec, tasks: &Tasks) -> Check {
    let covered: Vec<&String> = tasks.tasks.iter().flat_map(|t| t.criteria.iter()).collect();
    let missing: Vec<&str> = spec
        .criteria
        .iter()
        .filter(|c| !covered.iter().any(|x| **x == c.id))
        .map(|c| c.id.as_str())
        .collect();
    if missing.is_empty() {
        return Check::pass("criteria-covered", format!("{}/{} criteria have a task",
            spec.criteria.len(), spec.criteria.len()));
    }
    Check::fail(
        "criteria-covered",
        "every criterion is covered by a task",
        format!("uncovered: {}", missing.join(", ")),
    )
}

fn task_budgets(tasks: &Tasks, cfg: &Config) -> Check {
    let mut missing: Vec<String> = Vec::new();
    let mut over: Vec<String> = Vec::new();
    for t in &tasks.tasks {
        match t.budget {
            None => missing.push(format!("{} \u{201c}{}\u{201d} (line {})", t.id, t.title, t.line)),
            Some(n) if n > cfg.plan.max_task_lines => {
                over.push(format!("{} = {n}", t.id));
            }
            Some(_) => {}
        }
    }
    if !missing.is_empty() {
        return Check::fail("task-budgets", "every task declares a line budget",
            format!("no budget on: {}", super::join_capped(&missing, 5)));
    }
    if !over.is_empty() {
        return Check::fail(
            "task-budgets",
            format!("no task above {} lines", cfg.plan.max_task_lines),
            format!("{} — split the task", super::join_capped(&over, 5)),
        );
    }
    Check::pass("task-budgets", format!("all {} tasks budgeted, total {} lines",
        tasks.tasks.len(), tasks.total_budget()))
}

fn total_budget(spec: &Spec, tasks: &Tasks) -> Check {
    let Some(declared) = spec.front.budget.lines else {
        return Check::blocked("total-budget", "the spec declares no `budget.lines` to compare against");
    };
    let total = tasks.total_budget();
    if total > declared {
        return Check::fail(
            "total-budget",
            format!("task budgets sum to at most the spec's {declared} lines"),
            format!("{total} lines across {} tasks", tasks.tasks.len()),
        );
    }
    Check::pass("total-budget", format!("{total}/{declared} lines"))
}

fn task_exit_conditions(tasks: &Tasks) -> Check {
    let missing: Vec<String> = tasks
        .tasks
        .iter()
        .filter(|t| t.exit.as_ref().is_none_or(|e| e.trim().is_empty() || is_placeholder(e)))
        .map(|t| format!("{} \u{201c}{}\u{201d} (line {})", t.id, t.title, t.line))
        .collect();
    if missing.is_empty() {
        return Check::pass("task-exit-conditions", "every task states when it is done");
    }
    Check::fail(
        "task-exit-conditions",
        "every task states an exit condition",
        format!("no exit condition on: {}", super::join_capped(&missing, 5)),
    )
}

/// Dependencies must name real tasks and must be orderable.
fn task_dependencies(tasks: &Tasks) -> Check {
    let dangling = tasks.dangling_dependencies();
    if !dangling.is_empty() {
        return Check::fail(
            "task-dependencies",
            "every dependency names a task in this plan",
            format!("unknown: {}", super::join_capped(&dangling, 5)),
        );
    }
    match tasks.waves() {
        Err(stuck) => Check::fail(
            "task-dependencies",
            "the dependency graph is orderable",
            format!("cycle among {} — a plan that cannot be ordered is not a plan", stuck.join(", ")),
        ),
        Ok(waves) => {
            let widths: Vec<String> = waves.iter().map(|w| w.len().to_string()).collect();
            Check::pass(
                "task-dependencies",
                format!("{} wave(s), widths {}", waves.len(), widths.join("/")),
            )
        }
    }
}

/// Tasks in the same wave must not claim the same files.
///
/// A wave is the set keel may run concurrently, each in its own worktree. Two
/// tasks editing one file in parallel produce patches that conflict on the way
/// back, and the conflict surfaces after both agents have done their work
/// rather than before either started. Declaring the overlap is cheap; hitting
/// it is not.
fn wave_isolation(tasks: &Tasks) -> Check {
    let Ok(waves) = tasks.waves() else {
        return Check::blocked("wave-isolation", "the dependency graph is not orderable");
    };
    let mut clashes: Vec<String> = Vec::new();
    for (n, wave) in waves.iter().enumerate() {
        for (i, a) in wave.iter().enumerate() {
            for b in wave.iter().skip(i + 1) {
                let shared: Vec<&String> =
                    a.files.iter().filter(|f| b.files.contains(f)).collect();
                if !shared.is_empty() {
                    clashes.push(format!(
                        "wave {}: {} and {} both claim {}",
                        n + 1,
                        a.id,
                        b.id,
                        shared.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
    }
    if clashes.is_empty() {
        return Check::pass(
            "wave-isolation",
            format!("{} wave(s), no file claimed twice within one", waves.len()),
        );
    }
    Check::fail(
        "wave-isolation",
        "no two tasks in a wave touch the same file",
        format!(
            "{} — add a `depends_on` to order them",
            super::join_capped(&clashes, 4)
        ),
    )
}

fn task_files_in_scope(spec: &Spec, tasks: &Tasks) -> Check {
    if spec.front.scope.is_empty() {
        return Check::blocked("task-files-in-scope", "the spec declares no scope to check against");
    }
    let mut builder = GlobSetBuilder::new();
    for p in &spec.front.scope {
        match Glob::new(p.trim()) {
            Ok(g) => { builder.add(g); }
            Err(e) => return Check::blocked("task-files-in-scope", format!("scope glob `{p}` is invalid: {e}")),
        }
    }
    let Ok(set) = builder.build() else {
        return Check::blocked("task-files-in-scope", "could not build the scope matcher");
    };

    let mut outside: Vec<String> = Vec::new();
    let mut unnamed: Vec<String> = Vec::new();
    for t in &tasks.tasks {
        let named: Vec<&String> = t.files.iter().filter(|f| !is_placeholder(f)).collect();
        if named.is_empty() {
            unnamed.push(format!("{} (line {})", t.id, t.line));
            continue;
        }
        // `files: scope` means "this task touches files within the spec's
        // declared scope" — useful for config-only changes where enumerating
        // individual files per-task adds no information.
        if named.len() == 1 && named[0].eq_ignore_ascii_case("scope") {
            continue;
        }
        for f in named {
            if !set.is_match(f.as_str()) {
                outside.push(format!("{} → {f}", t.id));
            }
        }
    }
    if !unnamed.is_empty() {
        return Check::fail("task-files-in-scope", "every task names the files it touches",
            format!("no files on: {}", super::join_capped(&unnamed, 5)));
    }
    if !outside.is_empty() {
        return Check::fail(
            "task-files-in-scope",
            format!("task files match the declared scope ({})", spec.front.scope.join(", ")),
            format!("outside scope: {} — widen the scope deliberately or move the work", super::join_capped(&outside, 5)),
        );
    }
    Check::pass("task-files-in-scope", "all task files inside the declared scope")
}

fn rollback_stated(plan: &Plan) -> Check {
    let r = plan.front.rollback.trim();
    if !r.is_empty() && !is_placeholder(r) {
        return Check::pass("rollback-stated", crate::gate::truncate(r, 60));
    }
    // Fall back to the `## Rollback` section in the plan body — the scaffold
    // has one, and a user who fills it in rather than the front matter field
    // should not be punished for picking the wrong box.
    if let Some(body_text) = extract_rollback_section(&plan.body)
        && !body_text.is_empty() && !is_placeholder(&body_text)
    {
        return Check::pass("rollback-stated", crate::gate::truncate(&body_text, 60));
    }
    Check::fail(
        "rollback-stated",
        "front matter `rollback:` or a ## Rollback section states the rollback",
        "both are empty — \"git revert\" is a legitimate answer, silence is not",
    )
}

/// Extract the text under `## Rollback` until the next heading or end of file.
fn extract_rollback_section(body: &str) -> Option<String> {
    let mut capturing = false;
    let mut lines: Vec<&str> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            if capturing {
                break;
            }
            if trimmed.strip_prefix("## ").unwrap_or("").trim().eq_ignore_ascii_case("rollback") {
                capturing = true;
                continue;
            }
        }
        if capturing {
            lines.push(trimmed);
        }
    }
    if !capturing {
        return None;
    }
    let text = lines.join(" ").trim().to_string();
    // Strip the scaffold instruction that `keel plan` writes.
    let text = text.replace("_Fill in `rollback:` in the front matter above.", "")
        .replace("\"git revert\" is a legitimate answer; \"we would not need to\" is not._", "")
        .trim().to_string();
    Some(text)
}

fn blast_radius_declared(plan: &Plan) -> Check {
    let b = &plan.front.blast;
    if b.declared.is_empty() {
        return Check::fail("blast-radius-declared", "front matter declares `blast.declared`", "empty");
    }
    if b.depth == 0 {
        return Check::fail(
            "blast-radius-declared",
            "a blast depth of at least 1",
            "depth 0 only ever reports the files you named — it computes nothing",
        );
    }
    Check::pass(
        "blast-radius-declared",
        format!("{} file(s) at depth {}, {} lines", b.computed.len(), b.depth, b.computed_lines),
    )
}

/// Recompute the radius and compare. This is what makes "computed rather than
/// guessed" a property of the artefact rather than a claim about how it was made.
fn blast_radius_current(paths: &Paths, cfg: &Config, spec: &Spec, plan: &Plan) -> Result<Check> {
    let db = paths.index_db();
    if !db.exists() {
        return Ok(Check::blocked("blast-radius-current", "no symbol index — run `keel map`"));
    }
    let index = Index::open(&db)?;
    let depth = if plan.front.blast.depth > 0 { plan.front.blast.depth } else { cfg.plan.blast_depth };
    let fresh = match blast::compute(&index, &spec.front.scope, depth) {
        Ok(f) => f,
        Err(e) => return Ok(Check::blocked("blast-radius-current", format!("could not compute: {e}"))),
    };

    let mut computed: Vec<&str> = fresh.impact.iter().map(|i| i.path.as_str()).collect();
    let mut recorded: Vec<&str> = plan.front.blast.computed.iter().map(|s| s.as_str()).collect();
    computed.sort_unstable();
    recorded.sort_unstable();

    if computed == recorded {
        // An all-new scope is legitimate for an additive change, but it is also
        // what a typo'd glob looks like. keel cannot tell them apart, so it puts
        // the fact on the gate record rather than only in `keel plan` output.
        let note = if fresh.unmatched_globs.is_empty() {
            String::new()
        } else {
            format!(
                " — no indexed file matches {} (new files, or a typo)",
                fresh.unmatched_globs.join(", ")
            )
        };
        return Ok(Check::pass(
            "blast-radius-current",
            format!(
                "{} files, {} lines — matches a fresh computation{note}",
                computed.len(), fresh.impact_lines
            ),
        ));
    }

    let appeared: Vec<&str> = computed.iter().filter(|p| !recorded.contains(p)).copied().collect();
    let vanished: Vec<&str> = recorded.iter().filter(|p| !computed.contains(p)).copied().collect();
    let mut detail = Vec::new();
    if !appeared.is_empty() { detail.push(format!("now also touches {}", appeared.join(", "))); }
    if !vanished.is_empty() { detail.push(format!("no longer touches {}", vanished.join(", "))); }

    Ok(Check::fail(
        "blast-radius-current",
        "the recorded radius matches a fresh computation from the map",
        format!("{} — re-run `keel plan {}`", detail.join("; "), spec.front.slug),
    ))
}

#[cfg(test)]
mod tests {
    use super::is_placeholder;

    #[test]
    fn placeholder_text_never_counts_as_filled_in() {
        assert!(is_placeholder("_name the files this task touches_"));
        assert!(is_placeholder("TODO"));
        assert!(is_placeholder("<fill me in>"));
        assert!(!is_placeholder("cargo test --test rate_limit passes"));
        assert!(!is_placeholder("git revert the merge commit"));
    }
}
