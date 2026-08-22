//! **G4 — learning.**
//!
//! Checks (PLAN.md §4.4): every failure episode classified; lesson candidates
//! proposed; promotions accepted or rejected by a human.
//!
//! The check that matters most here is `unattributable-rate`. Peralta et al.
//! found a third of rejected agentic PRs had no observable rationale; a harness
//! that quietly folds those into its lesson store is training on noise. So the
//! rate is a number on every G4 report, and it is a *failure* when it goes high
//! enough that classification has stopped meaning anything.

use super::{Check, GateResult, run_plugins};
use crate::config::Config;
use crate::failure::{self, Attribution, Episode};
use crate::lesson::{self, Candidate, Lesson};
use crate::paths::Paths;
use crate::run::Run;
use anyhow::Result;

pub fn run(
    paths: &Paths,
    cfg: &Config,
    run: &Run,
    episodes: &[Episode],
    candidates: &[Candidate],
) -> Result<GateResult> {
    let existing = lesson::list(paths)?;
    let mut checks = vec![
        episodes_classified(episodes),
        unattributable_rate(cfg, episodes),
        candidates_proposed(candidates),
        promotion_decisions(candidates),
        no_contradictions(&existing),
        lesson_size(&existing),
        enforced_share(&existing),
        decay_review(paths, &existing)?,
    ];
    checks.extend(run_plugins(paths, cfg, "G4", Some(&run.meta.spec)));
    Ok(GateResult::new("G4", Some(run.meta.spec.clone()), checks))
}

/// Every episode has an attribution. `UNATTRIBUTABLE` counts as classified —
/// it is a decision, not an omission.
fn episodes_classified(episodes: &[Episode]) -> Check {
    if episodes.is_empty() {
        return Check::pass("episodes-classified", "no failure episodes in this run");
    }
    // An agentic episode with no class is a hole in the taxonomy, not a verdict.
    let unclassed: Vec<String> = episodes
        .iter()
        .filter(|e| e.attribution == Attribution::Agentic && e.class.is_none())
        .map(|e| e.id.clone())
        .collect();
    if unclassed.is_empty() {
        return Check::pass(
            "episodes-classified",
            format!("{} episode(s), all attributed", episodes.len()),
        );
    }
    Check::fail(
        "episodes-classified",
        "every agentic episode carries a class",
        format!("unclassed: {}", super::join_capped(&unclassed, 5)),
    )
}

fn unattributable_rate(cfg: &Config, episodes: &[Episode]) -> Check {
    let d = failure::distribution(episodes);
    if episodes.is_empty() {
        return Check::pass("unattributable-rate", "no episodes to attribute");
    }
    let pct = d.unattributable_rate * 100.0;
    let max = cfg.learn.max_unattributable_rate * 100.0;
    let detail = format!(
        "{pct:.0}% unattributable ({}), {:.0}% of agentic failures are harness-fixable",
        d.by_attribution
            .iter()
            .map(|(k, n)| format!("{n} {k}"))
            .collect::<Vec<_>>()
            .join(", "),
        d.harness_fixable_rate * 100.0
    );
    if pct > max {
        return Check::fail(
            "unattributable-rate",
            format!("at most {max:.0}% of episodes unattributable"),
            format!("{detail} — the classifier has stopped explaining anything; extend the taxonomy rather than learning from noise"),
        );
    }
    Check::pass("unattributable-rate", detail)
}

fn candidates_proposed(candidates: &[Candidate]) -> Check {
    if candidates.is_empty() {
        return Check::pass("candidates-proposed", "nothing recurring enough to distil");
    }
    let ready = candidates.iter().filter(|c| c.promotable).count();
    Check::pass(
        "candidates-proposed",
        format!("{} candidate(s), {ready} meet the promotion rules", candidates.len()),
    )
}

/// A promotable candidate that nobody has accepted or rejected is the whole
/// point of G4: the gate exists to force the decision, not to make it.
fn promotion_decisions(candidates: &[Candidate]) -> Check {
    let undecided: Vec<String> = candidates
        .iter()
        .filter(|c| c.promotable)
        .map(|c| format!("{} in {}", c.class.code(), c.scope))
        .collect();
    if undecided.is_empty() {
        return Check::pass("promotion-decisions", "no candidate is awaiting a decision");
    }
    Check::fail(
        "promotion-decisions",
        "every promotable candidate is accepted or rejected",
        format!(
            "{} awaiting a human: {} — `keel lesson promote <n>` or `keel lesson reject <n>`",
            undecided.len(),
            super::join_capped(&undecided, 4)
        ),
    )
}

/// Promotion rule 4, checked across the whole store rather than per candidate:
/// two lessons of the same class in overlapping scopes are a merge waiting to
/// happen, and the plan says to fail loudly rather than let both stand.
fn no_contradictions(lessons: &[Lesson]) -> Check {
    let mut clashes: Vec<String> = Vec::new();
    for (i, a) in lessons.iter().enumerate() {
        for b in lessons.iter().skip(i + 1) {
            if a.class() == b.class()
                && a.class().is_some()
                && lesson::scopes_overlap(&a.front.scope, &b.front.scope)
            {
                clashes.push(format!("{} and {} both cover {}", a.front.id, b.front.id, a.front.class));
            }
        }
    }
    if clashes.is_empty() {
        return Check::pass("no-contradictions", format!("{} lesson(s), no overlap", lessons.len()));
    }
    Check::fail(
        "no-contradictions",
        "no two lessons cover the same class in overlapping scopes",
        format!("{} — merge them", super::join_capped(&clashes, 4)),
    )
}

/// Promotion rule 5.
fn lesson_size(lessons: &[Lesson]) -> Check {
    let long: Vec<String> = lessons
        .iter()
        .filter(|l| l.body_lines() > lesson::MAX_BODY_LINES)
        .map(|l| format!("{} ({} lines)", l.front.id, l.body_lines()))
        .collect();
    if long.is_empty() {
        return Check::pass("lesson-size", format!("all within {} lines", lesson::MAX_BODY_LINES));
    }
    Check::fail(
        "lesson-size",
        format!("every lesson at most {} lines", lesson::MAX_BODY_LINES),
        format!("{} — long lessons are specs in disguise", long.join(", ")),
    )
}

/// Promotion rule 3, as a visible number rather than an aspiration: a lesson
/// with an oracle is enforced and costs no context; a lesson without one is a
/// paragraph competing for the budget.
fn enforced_share(lessons: &[Lesson]) -> Check {
    if lessons.is_empty() {
        return Check::pass("enforced-share", "no lessons in force");
    }
    let enforced = lessons.iter().filter(|l| l.oracle().is_some()).count();
    let prompts: Vec<String> = lessons
        .iter()
        .filter(|l| l.oracle().is_none())
        .map(|l| l.front.id.clone())
        .collect();
    Check::pass(
        "enforced-share",
        format!(
            "{enforced}/{} enforced by a check{}",
            lessons.len(),
            if prompts.is_empty() {
                String::new()
            } else {
                format!("; still prompts: {}", super::join_capped(&prompts, 5))
            }
        ),
    )
}

/// The direct counter to unbounded `CLAUDE.md` growth: a lesson that has done
/// nothing for its decay period goes to demotion review.
fn decay_review(paths: &Paths, lessons: &[Lesson]) -> Result<Check> {
    let ledger = lesson::usage::Ledger::load(paths)?;
    let stale: Vec<String> = lessons
        .iter()
        .filter(|l| ledger.idle_days(&l.front.id, &l.front.verified_at) as u64 > l.decay_days())
        .map(|l| {
            format!(
                "{} (idle {}d, decay {})",
                l.front.id,
                ledger.idle_days(&l.front.id, &l.front.verified_at),
                l.front.decay
            )
        })
        .collect();
    if stale.is_empty() {
        return Ok(Check::pass("decay-review", format!("{} lesson(s) in date", lessons.len())));
    }
    Ok(Check::fail(
        "decay-review",
        "no lesson is past its decay period unused",
        format!(
            "{} — `keel lesson demote <id>` or re-verify it",
            super::join_capped(&stale, 4)
        ),
    ))
}
