//! Characterisation of the pipeline state machine behind `keel next`.
//!
//! `keel next` derives where a spec sits twice: once compactly for the
//! multi-spec listing, and once in full to produce focused guidance. The two
//! derivations must agree, and neither may drift when the machine is
//! refactored. These tests pin the observable behaviour of both so a shared
//! implementation can be substituted underneath them without changing what a
//! user sees.

mod support;

use support::Repo;

/// A driver that makes a legitimate in-scope change, so G2 can pass.
fn working_driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     printf 'pub fn serve() { /* limited */ }\\n' > \"$KEEL_REPO/src/api/mod.rs\"\n\
     printf '#[test]\\nfn respects_config() { assert_eq!(1 + 1, 2); }\\n' > \"$KEEL_REPO/tests/limit.rs\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\",\"tests/limit.rs\"]}'\n"
        .to_string()
}

/// A spec whose only criterion carries no oracle, so G0 fails.
fn write_unoracled_spec(r: &Repo) {
    r.write(
        ".keel/specs/demo/spec.md",
        "---\n\
         id: SPEC-0001\nslug: demo\nschema: keel.spec/1\nstatus: draft\n\
         scope:\n  - \"src/api/**\"\n\
         budget:\n  criteria: 6\n  lines: 120\n---\n\n\
         # Demo\n\n## Acceptance criteria\n\n\
         ### AC-1 Requests over the limit are rejected\n\n\
         WHEN a client exceeds 100 requests per minute THE SYSTEM SHALL respond with HTTP 429.\n",
    );
}

/// The headline `keel next <slug>` prints, without the leading glyph.
fn headline(r: &Repo, slug: &str) -> String {
    let (_, out) = r.run(&["next", slug]);
    out.lines()
        .find(|l| l.trim_start().starts_with('▸'))
        .unwrap_or_else(|| panic!("no guidance headline in:\n{out}"))
        .trim_start()
        .trim_start_matches('▸')
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// the happy path, one state at a time
// ---------------------------------------------------------------------------

#[test]
fn next_names_every_stage_of_the_pipeline_in_order() {
    let r = Repo::bare("next-walk");
    r.write_spec();

    // G0 has never run.
    let h = headline(&r, "demo");
    assert!(h.contains("run G0"), "before G0: {h}");

    r.ok(&["gate", "g0", "demo"]);
    let h = headline(&r, "demo");
    assert!(h.contains("approve") && h.contains("spec"), "after G0: {h}");

    r.ok(&["approve", "demo", "--stage", "spec"]);
    let h = headline(&r, "demo");
    assert!(h.contains("create a plan"), "after spec approval: {h}");

    r.ok(&["plan", "demo"]);
    r.set_rollback("git revert the merge");
    r.write_tasks();
    let h = headline(&r, "demo");
    assert!(h.contains("run G1"), "after planning: {h}");

    r.ok(&["gate", "g1", "demo"]);
    let h = headline(&r, "demo");
    assert!(h.contains("approve") && h.contains("plan"), "after G1: {h}");

    r.ok(&["approve", "demo", "--stage", "plan"]);
    let h = headline(&r, "demo");
    assert!(h.contains("do the work"), "after plan approval: {h}");

    // A passing run, then the merge decision.
    r.install_driver("worker", &working_driver());
    assert_eq!(r.run(&["run", "demo"]).0, 1, "G3 should want a human");
    let h = headline(&r, "demo");
    assert!(h.contains("approve the merge"), "after a passing G2: {h}");

    r.ok(&["approve", "demo", "--stage", "merge"]);
    let h = headline(&r, "demo");
    assert!(h.contains("complete"), "after merge approval: {h}");
}

// ---------------------------------------------------------------------------
// the arms the compact stage collapses but the guidance distinguishes
// ---------------------------------------------------------------------------

#[test]
fn a_failing_g0_asks_for_the_spec_to_be_fixed_not_gated() {
    let r = Repo::bare("next-g0-fail");
    write_unoracled_spec(&r);
    assert_eq!(r.run(&["gate", "g0", "demo"]).0, 1, "G0 should have failed");

    let h = headline(&r, "demo");
    assert!(h.contains("fix"), "{h}");
    assert!(h.contains("G0 has"), "the failure count should be named: {h}");
}

#[test]
fn a_rejected_spec_reads_differently_from_an_unapproved_one() {
    let r = Repo::bare("next-rejected");
    r.write_spec();
    r.ok(&["gate", "g0", "demo"]);

    let absent = headline(&r, "demo");
    assert!(absent.contains("approve"), "{absent}");

    r.ok(&["approve", "demo", "--stage", "spec", "--reject", "--note", "criteria are vague"]);
    let rejected = headline(&r, "demo");
    assert!(rejected.contains("rejected"), "{rejected}");
    assert_ne!(absent, rejected, "rejected and absent produced the same guidance");

    let (_, full) = r.run(&["next", "demo"]);
    assert!(full.contains("criteria are vague"), "the rejection note was dropped:\n{full}");
}

#[test]
fn editing_an_approved_spec_asks_for_re_approval_not_first_approval() {
    let r = Repo::bare("next-superseded");
    r.write_spec();
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);

    let p = ".keel/specs/demo/spec.md";
    r.write(p, &r.read(p).replace("HTTP 429", "HTTP 503"));

    let h = headline(&r, "demo");
    assert!(h.contains("re-approve"), "a superseded approval should say re-approve: {h}");
}

#[test]
fn a_g1_pass_that_predates_the_spec_approval_is_stale() {
    let r = Repo::ready("next-stale-g1");

    // Re-approving the spec moves the approval past G1's timestamp.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    r.ok(&["approve", "demo", "--stage", "spec"]);

    let h = headline(&r, "demo");
    assert!(h.contains("G1"), "a stale G1 should be re-run, got: {h}");
}

/// Editing the tasks after a completed cycle supersedes the *plan* approval as
/// well as the merge one, and the plan check sits earlier in the machine — so
/// the guidance sends you back to re-approve the plan rather than straight to
/// the work. The stage machine's own superseded-merge arm is therefore only
/// reachable if a merge approval can go stale while the plan approval stays
/// current, which the shared artefact hash presently prevents. Pinned here
/// because the compact and focused paths must keep agreeing about it.
#[test]
fn editing_the_tasks_after_a_completed_cycle_reopens_the_plan_approval() {
    let r = Repo::ready("next-merge-supersede");
    r.install_driver("worker", &working_driver());
    r.ok(&["approve", "demo", "--stage", "plan"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);
    assert_eq!(r.run(&["run", "demo"]).0, 0);
    assert!(headline(&r, "demo").contains("complete"), "the cycle should be done");

    // The agreed shape of the work changes after sign-off.
    let p = ".keel/specs/demo/tasks.md";
    r.write(p, &r.read(p).replace("budget: 60", "budget: 90"));

    let h = headline(&r, "demo");
    assert!(!h.contains("complete"), "a stale approval was treated as done: {h}");
    assert!(h.contains("plan"), "the reopened decision is the plan approval: {h}");
}

// ---------------------------------------------------------------------------
// the compact listing and the focused guidance must agree
// ---------------------------------------------------------------------------

#[test]
fn the_listed_stage_agrees_with_the_focused_guidance() {
    let r = Repo::bare("next-agree");
    r.write_spec();
    r.ok(&["gate", "g0", "demo"]);

    // A second spec puts `keel next` into listing mode.
    // A scaffolded spec fails G0 by design, so this exits non-zero.
    r.run(&["spec", "new", "other", "--scope", "src/api/**"]);

    // Walk `demo` forward; at each step the listed label must describe the same
    // stage the focused guidance acts on.
    let checkpoints: &[(&[&str], &str)] = &[
        (&[], "approve spec"),
        (&["approve", "demo", "--stage", "spec"], "plan"),
    ];
    for (cmd, expected_label) in checkpoints {
        if !cmd.is_empty() {
            r.ok(cmd);
        }
        let (_, listing) = r.run(&["next"]);
        let line = listing
            .lines()
            .find(|l| l.trim_start().starts_with("demo"))
            .unwrap_or_else(|| panic!("demo missing from listing:\n{listing}"));
        assert!(
            line.contains(expected_label),
            "listing said {line:?}, expected the stage {expected_label:?}"
        );
    }
}

#[test]
fn a_spec_that_never_passed_g0_is_listed_at_the_spec_stage() {
    let r = Repo::bare("next-listing-g0");
    write_unoracled_spec(&r);
    r.run(&["gate", "g0", "demo"]);
    // A scaffolded spec fails G0 by design, so this exits non-zero.
    r.run(&["spec", "new", "other", "--scope", "src/api/**"]);

    let (_, listing) = r.run(&["next"]);
    let line = listing
        .lines()
        .find(|l| l.trim_start().starts_with("demo"))
        .unwrap_or_else(|| panic!("demo missing from listing:\n{listing}"));
    assert!(line.contains("spec"), "{line}");
}

// ---------------------------------------------------------------------------
// a completed spec is locked against further approvals
// ---------------------------------------------------------------------------

/// Walk a spec all the way to `Stage::Complete`.
fn completed_repo(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.ok(&["approve", "demo", "--stage", "plan"]);
    r.install_driver("worker", &working_driver());
    assert_eq!(r.run(&["run", "demo"]).0, 1, "G3 should want a human");
    r.ok(&["approve", "demo", "--stage", "merge"]);
    assert!(headline(&r, "demo").contains("complete"), "the cycle should be done");
    r
}

#[test]
fn approving_a_completed_spec_is_refused() {
    let r = completed_repo("lock-refused");
    let before = r.read(".keel/specs/demo/approvals.jsonl");

    let (code, _) = r.run(&["approve", "demo", "--stage", "spec"]);
    assert_ne!(code, 0, "approving a completed spec should fail");

    let after = r.read(".keel/specs/demo/approvals.jsonl");
    assert_eq!(before, after, "a refused approval must not be appended to the log");
}

#[test]
fn the_refusal_names_the_completed_slug() {
    let r = completed_repo("lock-message");
    let (_, out) = r.run(&["approve", "demo", "--stage", "spec"]);
    assert!(out.contains("demo"), "the refusal should name the slug: {out}");
    assert!(out.contains("complete"), "the refusal should say why: {out}");
}

#[test]
fn force_overrides_the_completed_lock() {
    let r = completed_repo("lock-force");
    let before = r.read(".keel/specs/demo/approvals.jsonl").lines().count();

    r.ok(&["approve", "demo", "--stage", "spec", "--force"]);

    let after = r.read(".keel/specs/demo/approvals.jsonl");
    assert_eq!(after.lines().count(), before + 1, "the forced approval should be recorded");
    assert!(
        after.contains("overrode the completed-spec lock"),
        "the override should be noted in the log: {after}"
    );
}

#[test]
fn an_incomplete_spec_still_approves_normally() {
    let r = Repo::bare("lock-unaffected");
    r.write_spec();
    r.ok(&["gate", "g0", "demo"]);

    let (code, _) = r.run(&["approve", "demo", "--stage", "spec"]);
    assert_eq!(code, 0, "a spec that is not complete must approve as before");
    assert!(r.read(".keel/specs/demo/approvals.jsonl").contains("\"stage\":\"spec\""));
}
