//! Where a spec sits between G0 and a merged change.
//!
//! One evaluation, two readers. `keel next` renders prose from it and the
//! report renders a lifecycle spine from it, so the compact stage label and the
//! detailed guidance cannot disagree about what is blocking — they are derived
//! from the same [`Position`].
//!
//! [`Position`] deliberately carries the evidence the decision was made from,
//! not just the conclusion: `next` needs to distinguish an approval that is
//! absent from one that was rejected or superseded, and the report needs the
//! gate results and the passing run id. Recomputing any of that at the call
//! site is how the two copies this module replaced drifted apart.
//!
//! An approval that cannot be read at all is treated as [`Standing::Absent`]
//! rather than propagated. That keeps the multi-spec listing legible when one
//! spec is broken, and it matches what the compact path did before this module
//! existed.

use crate::approval::{self, Standing};
use crate::gate::{self, GateResult, Verdict};
use crate::paths::Paths;
use crate::plan::Plan;

/// The stage a spec is waiting at.
///
/// Ordering is the pipeline's own order and several callers rely on it, so the
/// variants must not be reordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// G0 has never run or is failing.
    Spec,
    /// G0 passes but the spec is not approved.
    SpecApproval,
    /// Spec approved, no plan yet.
    Plan,
    /// A plan exists but G1 is not passing.
    PlanGate,
    /// G1 passes but the plan is not approved.
    PlanApproval,
    /// Plan approved, work not done or G2 not passing.
    Run,
    /// A run passed, merge not approved.
    MergeApproval,
    /// All done.
    Complete,
}

impl Stage {
    pub fn label(&self) -> &'static str {
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

    /// A stable identifier for the wire.
    ///
    /// Separate from [`Stage::label`] because that one is prose and may be
    /// reworded; this one is a schema value and may not be.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::SpecApproval => "spec_approval",
            Self::Plan => "plan",
            Self::PlanGate => "plan_gate",
            Self::PlanApproval => "plan_approval",
            Self::Run => "run",
            Self::MergeApproval => "merge_approval",
            Self::Complete => "complete",
        }
    }
}

/// A spec's place in the pipeline, and what that conclusion was drawn from.
#[derive(Debug, Clone)]
pub struct Position {
    pub stage: Stage,
    pub g0: Option<GateResult>,
    pub g1: Option<GateResult>,
    /// G1 passed, but before the spec's current approval — so it judged a spec
    /// that has since changed.
    pub g1_stale: bool,
    pub spec_approval: Standing,
    pub plan_approval: Standing,
    pub merge_approval: Standing,
    /// The most recent run for this spec whose G2 passed, if any.
    pub passing_run: Option<String>,
}

/// Read an approval, treating an unreadable one as absent. See the module doc.
fn standing_or_absent(paths: &Paths, slug: &str, stage: &str) -> Standing {
    approval::standing(paths, slug, stage).unwrap_or(Standing::Absent)
}

fn passed(g: &Option<GateResult>) -> bool {
    matches!(g, Some(r) if r.verdict == Verdict::Pass)
}

/// Evaluate a spec's position once.
pub fn position(paths: &Paths, slug: &str) -> Position {
    let g0 = gate::previous(paths, slug, "G0");
    let spec_approval = standing_or_absent(paths, slug, "spec");
    let has_plan = Plan::load(paths, slug).is_ok();
    let g1 = gate::previous(paths, slug, "G1");

    // A G1 pass that predates the current spec approval judged a spec that has
    // since changed — new criteria, a wider scope — so the plan needs
    // re-verifying against what was actually approved.
    let g1_stale = match (&g1, &spec_approval) {
        (Some(r), Standing::Current { at, .. }) => r.verdict == Verdict::Pass && r.generated_at < *at,
        _ => false,
    };

    let plan_approval = standing_or_absent(paths, slug, "plan");
    let merge_approval = standing_or_absent(paths, slug, "merge");
    let passing_run = passing_run(paths, slug);

    let stage = derive_stage(
        &g0,
        &spec_approval,
        has_plan,
        &g1,
        g1_stale,
        &plan_approval,
        &merge_approval,
        passing_run.is_some(),
    );

    Position {
        stage,
        g0,
        g1,
        g1_stale,
        spec_approval,
        plan_approval,
        merge_approval,
        passing_run,
    }
}

/// Just the stage, for callers that do not need the evidence.
pub fn stage(paths: &Paths, slug: &str) -> Stage {
    position(paths, slug).stage
}

#[allow(clippy::too_many_arguments)]
fn derive_stage(
    g0: &Option<GateResult>,
    spec_approval: &Standing,
    has_plan: bool,
    g1: &Option<GateResult>,
    g1_stale: bool,
    plan_approval: &Standing,
    merge_approval: &Standing,
    has_passing_run: bool,
) -> Stage {
    if !passed(g0) {
        return Stage::Spec;
    }
    if !matches!(spec_approval, Standing::Current { .. }) {
        return Stage::SpecApproval;
    }
    if !has_plan {
        return Stage::Plan;
    }
    if !passed(g1) || g1_stale {
        return Stage::PlanGate;
    }
    if !matches!(plan_approval, Standing::Current { .. }) {
        return Stage::PlanApproval;
    }
    if !has_passing_run {
        return Stage::Run;
    }
    // A superseded merge means the plan changed since the last passing run, so
    // the work needs redoing rather than merely re-approving.
    match merge_approval {
        Standing::Superseded { .. } => Stage::Run,
        Standing::Current { .. } => Stage::Complete,
        _ => Stage::MergeApproval,
    }
}

/// The newest run for this spec whose G2 passed.
fn passing_run(paths: &Paths, slug: &str) -> Option<String> {
    let ids = crate::run::list(paths).ok()?;
    // `list` is chronological, so the last match is the most recent.
    ids.into_iter().rev().find(|id| {
        let Ok(run) = crate::run::Run::load(paths, id) else { return false };
        if run.meta.spec != slug {
            return false;
        }
        run.gate_results()
            .map(|rs| rs.iter().any(|r| r.gate == "G2" && r.verdict == Verdict::Pass))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Ord` derives from declaration order, and callers compare stages, so a
    /// reordered variant would silently change what "further along" means.
    #[test]
    fn declaration_order_is_pipeline_order() {
        let spine = [
            Stage::Spec,
            Stage::SpecApproval,
            Stage::Plan,
            Stage::PlanGate,
            Stage::PlanApproval,
            Stage::Run,
            Stage::MergeApproval,
            Stage::Complete,
        ];
        for pair in spine.windows(2) {
            assert!(pair[0] < pair[1], "{:?} should precede {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn a_stale_g1_reopens_the_gate_even_though_it_passed() {
        let g1 = GateResult {
            schema: "keel.gate/1".into(),
            gate: "G1".into(),
            run: "r".into(),
            spec: None,
            verdict: Verdict::Pass,
            generated_at: "2026-01-01T00:00:00+00:00".into(),
            checks: vec![],
        };
        let g0 = GateResult { gate: "G0".into(), ..g1.clone() };
        let approved = Standing::Current { by: "x".into(), at: "2026-06-01T00:00:00+00:00".into() };

        let s = derive_stage(
            &Some(g0),
            &approved,
            true,
            &Some(g1),
            true,
            &approved,
            &Standing::Absent,
            true,
        );
        assert_eq!(s, Stage::PlanGate);
    }

    #[test]
    fn a_superseded_merge_returns_to_the_work() {
        let pass = GateResult {
            schema: "keel.gate/1".into(),
            gate: "G0".into(),
            run: "r".into(),
            spec: None,
            verdict: Verdict::Pass,
            generated_at: "2026-01-01T00:00:00+00:00".into(),
            checks: vec![],
        };
        let approved = Standing::Current { by: "x".into(), at: "2026-01-01T00:00:00+00:00".into() };
        let superseded = Standing::Superseded {
            approved_hash: "aaa".into(),
            current_hash: "bbb".into(),
        };

        let s = derive_stage(
            &Some(pass.clone()),
            &approved,
            true,
            &Some(pass),
            false,
            &approved,
            &superseded,
            true,
        );
        assert_eq!(s, Stage::Run);
    }
}
