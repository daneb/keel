//! `keel approve <slug> --stage spec|plan` — record a human decision.

use crate::approval::{self, Decision, Standing};
use crate::gate;
use crate::paths::Paths;
use crate::spec::SpecFront;
use crate::store::frontmatter;
use anyhow::{Context, Result, bail};

pub fn run(slug: Option<String>, stage: String, reject: bool, note: Option<String>) -> Result<i32> {
    let paths = Paths::require_init()?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    if !approval::STAGES.contains(&stage.as_str()) {
        bail!("stage must be one of: {}", approval::STAGES.join(", "));
    }

    let artefact = approval::artefact_path(&paths, &slug, &stage);
    if !artefact.exists() {
        bail!("nothing to approve: {} does not exist", paths.rel(&artefact).display());
    }

    let gate_name = match stage.as_str() {
        "spec" => "G0",
        "plan" => "G1",
        _ => "G3",
    };
    let verdict = if gate_name == "G3" {
        // G3 lives in the run directory, not the spec directory: a merge is
        // approved against a particular run's evidence, not against the spec.
        crate::run::latest(&paths)?
            .and_then(|id| crate::run::Run::load(&paths, &id).ok())
            .and_then(|r| r.gate_results().ok())
            .and_then(|rs| rs.into_iter().find(|r| r.gate == "G3").map(|r| r.verdict))
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

pub fn show(slug: Option<String>) -> Result<i32> {
    let paths = Paths::require_init()?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    let history = approval::history(&paths, &slug)?;
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
