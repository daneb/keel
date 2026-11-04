//! `keel approve <slug> --stage spec|plan` — record a human decision.

use crate::approval::{self, Decision, Standing};
use crate::gate;
use crate::paths::Paths;
use crate::pipeline::Stage;
use crate::spec::SpecFront;
use crate::store::frontmatter;
use anyhow::{Context, Result, bail};

pub fn run(
    slug: Option<String>,
    stage: String,
    reject: bool,
    note: Option<String>,
    force: bool,
) -> Result<i32> {
    let paths = Paths::require_init()?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    if !approval::STAGES.contains(&stage.as_str()) {
        bail!("stage must be one of: {}", approval::STAGES.join(", "));
    }

    let artefact = approval::artefact_path(&paths, &slug, &stage);
    if !artefact.exists() {
        bail!("nothing to approve: {} does not exist", paths.rel(&artefact).display());
    }

    // A spec that has already reached Complete is done: its stages are
    // settled, and a slip naming the wrong (old, finished) slug should not be
    // able to silently rewrite its approval history. `--force` is the
    // deliberate escape hatch; anything else is refused outright.
    let mut note = note;
    if crate::pipeline::stage(&paths, &slug) == Stage::Complete {
        if !force {
            bail!(
                "`{slug}` is already complete (merge approved) — its stages are locked. \
                 Pass --force if you really mean to record a new {stage} decision on it."
            );
        }
        let override_note = format!("--force: overrode the completed-spec lock on `{slug}`");
        note = Some(match note {
            Some(n) => format!("{override_note}; {n}"),
            None => override_note,
        });
    }

    let gate_name = match stage.as_str() {
        "spec" => "G0",
        "plan" => "G1",
        _ => "G3",
    };
    let verdict = if gate_name == "G3" {
        // G3 lives in the run directory, not the spec directory: a merge is
        // approved against a particular run's evidence, not against the spec.
        // Find the latest run *for this spec* — not just any run.
        find_g3_verdict(&paths, &slug)?
    } else {
        gate::previous(&paths, &slug, gate_name).map(|r| r.verdict)
    };

    // Approving over a failing gate is allowed — a human may always overrule —
    // but it is recorded as exactly that, not laundered into a pass.
    if !reject {
        match verdict {
            None => println!(
                "  note: {gate_name} has never been run for `{slug}`. Approving anyway is recorded as such."
            ),
            Some(v) if v != gate::Verdict::Pass => println!(
                "  note: {gate_name} is {} — this approval overrides a gate that did not pass.",
                v.glyph()
            ),
            _ => {}
        }
    }

    let decision = if reject { Decision::Rejected } else { Decision::Approved };

    // Transition the spec's `status` field before recording the approval so the
    // hash covers the file with its correct status. Without this, `status: draft`
    // would persist forever — confusing in `keel spec list` and anywhere else
    // that reads the front matter.
    if stage == "spec" {
        transition_spec_status(&paths, &slug, decision)?;
    }

    let recorded = approval::record(
        &paths,
        &slug,
        &stage,
        decision,
        verdict.map(|v| v.glyph().to_lowercase()),
        note,
    )?;

    // A human decision is part of the record (PLAN.md §4.5 lists them among the
    // event kinds). Appending it to the run's own stream is also what makes the
    // stream reopenable rather than write-once.
    if let Some(run_id) = crate::run::latest(&paths)?
        && let Ok(run) = crate::run::Run::load(&paths, &run_id)
        && run.meta.spec == slug
    {
        let mut traj = run.open_trajectory()?;
        traj.append(crate::trajectory::Payload::Human {
            stage: stage.clone(),
            decision: if reject { "rejected".into() } else { "approved".into() },
            by: recorded.by.clone(),
            note: recorded.note.clone(),
        })?;
    }

    println!(
        "  {} {} stage of `{slug}` as {} ({})",
        if reject { "rejected" } else { "approved" },
        stage,
        recorded.by,
        crate::hashing::short(&recorded.artefact_hash)
    );
    println!("  recorded in {}", paths.rel(&crate::spec::Spec::dir(&paths, &slug).join("approvals.jsonl")).display());
    Ok(0)
}

/// Update the spec file's `status` field in place.
///
/// The hash recorded by `approval::record` covers the file byte-for-byte, so
/// this must happen *before* that call — otherwise changing the status would
/// immediately invalidate the approval it accompanies.
fn transition_spec_status(paths: &Paths, slug: &str, decision: Decision) -> Result<()> {
    let path = crate::spec::Spec::path_for(paths, slug);
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let (mut front, body): (SpecFront, String) = frontmatter::split_typed(&raw)
        .with_context(|| format!("parsing {}", path.display()))?;

    let new_status = match decision {
        Decision::Approved => "approved",
        Decision::Rejected => "rejected",
        // `keel approve` never records an abandonment — that is
        // `keel spec abandon`, which writes `status: abandoned` itself. Leave
        // the status untouched rather than inventing one here.
        Decision::Abandoned => return Ok(()),
    };

    // Only rewrite if the status actually changes — avoids spurious diffs.
    if front.status == new_status {
        return Ok(());
    }

    front.status = new_status.to_string();
    let updated = frontmatter::join_typed(&front, &body)?;
    std::fs::write(&path, updated)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Find the G3 verdict from the most recent run for a given spec.
///
/// Run IDs are `YYYY-MM-DD-xxx` where the suffix is a hash, not a sequence
/// number — so lexicographic order is only chronological across days, not
/// within a day. Use the G3 result's `generated_at` timestamp to find the
/// truly latest one.
fn find_g3_verdict(paths: &Paths, slug: &str) -> Result<Option<gate::Verdict>> {
    let runs = crate::run::list(paths)?;
    let mut best: Option<(String, gate::Verdict)> = None;
    for id in runs {
        let Ok(run) = crate::run::Run::load(paths, &id) else { continue };
        if run.meta.spec != slug {
            continue;
        }
        if let Ok(results) = run.gate_results()
            && let Some(g3) = results.into_iter().find(|r| r.gate == "G3")
        {
            let dominated = best.as_ref().is_some_and(|(ts, _)| *ts >= g3.generated_at);
            if !dominated {
                best = Some((g3.generated_at.clone(), g3.verdict));
            }
        }
    }
    Ok(best.map(|(_, v)| v))
}

pub fn show(slug: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    let history = approval::history(&paths, &slug)?;

    if json {
        let mut standing = serde_json::Map::new();
        for stage in approval::STAGES {
            standing.insert(
                stage.to_string(),
                approval::standing_json(&approval::standing(&paths, &slug, stage)?),
            );
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": "keel.approvals/1",
                "spec": slug,
                "history": history,
                "standing": standing,
            }))?
        );
        return Ok(0);
    }

    if history.is_empty() {
        println!("  no approvals recorded for `{slug}`");
    }
    for a in &history {
        println!(
            "  {:<10} {:<9} {:<20} {} {}",
            a.stage,
            format!("{:?}", a.decision).to_lowercase(),
            a.by,
            &a.at[..a.at.len().min(19)],
            a.note.clone().unwrap_or_default()
        );
    }
    println!();
    for stage in approval::STAGES {
        let line = match approval::standing(&paths, &slug, stage)? {
            Standing::Current { by, .. } => format!("approved by {by}, current"),
            Standing::Absent => "not approved".to_string(),
            Standing::Rejected { by, .. } => format!("rejected by {by}"),
            Standing::Superseded { approved_hash, current_hash } => {
                format!("SUPERSEDED — approved {approved_hash}, now {current_hash}")
            }
        };
        println!("  {stage:<6} {line}");
    }
    Ok(0)
}

/// `keel judge <slug> <criterion> --pass|--fail` — one person's judgement of
/// one `human` oracle, bound to the latest run's diff and `oracles.json`.
pub fn judge(slug: String, criterion: String, pass: bool, fail: bool, note: Option<String>) -> Result<i32> {
    if pass == fail {
        bail!("say which: --pass or --fail");
    }
    let paths = Paths::require_init()?;
    let id = approval::latest_run_for(&paths, &slug)
        .with_context(|| format!("no run for `{slug}` — `keel run` first, so there is evidence to judge"))?;
    let run = crate::run::Run::load(&paths, &id)?;
    if !crate::spec::Spec::load(&paths, &slug)?.criteria.iter().any(|c| c.id == criterion) {
        bail!("`{slug}` has no criterion {criterion}");
    }
    let a = approval::judge(&paths, &run, &criterion, pass, note)?;
    // A human event: allowed after `run_end`, and G4's signal for a failed judgement.
    run.open_trajectory()?.append(crate::trajectory::Payload::Human {
        stage: a.stage.clone(),
        decision: if pass { "approved" } else { "rejected" }.into(),
        by: a.by.clone(),
        note: a.note.clone(),
    })?;
    println!("  judged {criterion} of `{slug}` {} by {} against run {id}", if pass { "pass" } else { "FAIL" }, a.by);
    Ok(0)
}
