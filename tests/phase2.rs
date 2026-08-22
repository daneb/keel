//! End-to-end tests for Phase 2: execute, gate, evidence.
//!
//! Encodes the Phase 2 exit criteria from PLAN.md §5: a full spec→merge cycle
//! with no manual gate bookkeeping, a run reproducible from its trajectory, and
//! a ratchet that actually blocks a regression.

mod support;

use support::{Repo, noop_driver};

/// A driver that makes a legitimate in-scope change and says so.
fn working_driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     printf 'pub fn serve() { /* limited */ }\\n' > \"$KEEL_REPO/src/api/mod.rs\"\n\
     printf '#[test]\\nfn respects_config() { assert_eq!(1 + 1, 2); }\\n' > \"$KEEL_REPO/tests/limit.rs\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\",\"tests/limit.rs\"]}'\n"
        .to_string()
}

/// A driver that changes code and no test at all — G2.5 should ask why.
fn code_only_driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     printf 'pub fn serve() { /* limited */ }\\n' > \"$KEEL_REPO/src/api/mod.rs\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\"]}'\n"
        .to_string()
}

// ---------------------------------------------------------------------------
// the full cycle
// ---------------------------------------------------------------------------

#[test]
fn a_full_cycle_runs_every_gate_and_produces_a_verifiable_bundle() {
    let r = Repo::ready("cycle");
    r.install_driver("worker", &working_driver());

    // G3 needs a human decision, so the first run stops short of passing.
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "G3 passed with no human decision on the record:\n{out}");
    assert!(out.contains("human-verdict"), "{out}");
    assert!(out.contains("G2 pass"), "G2 should have passed:\n{out}");
    assert!(out.contains("G2.5"), "G2.5 did not run:\n{out}");

    // Record the decision, then re-gate.
    r.ok(&["approve", "demo", "--stage", "merge", "--note", "reviewed the diff"]);
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 0, "the cycle did not complete:\n{out}");
    assert!(out.contains("G3 pass"), "{out}");

    // And the whole thing exports and verifies.
    let archive = r.ok(&["export"]).trim().to_string();
    let (code, out) = r.run(&["export", "--verify", &archive]);
    assert_eq!(code, 0, "the bundle did not verify:\n{out}");
}

#[test]
fn a_run_is_reproducible_from_its_own_trajectory() {
    let r = Repo::ready("reproducible");
    r.install_driver("worker", &working_driver());
    r.run(&["run", "demo"]);
    let id = r.latest_run();

    let events: Vec<serde_json::Value> = r
        .read(&format!(".keel/runs/{id}/trajectory.jsonl"))
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // The stream alone must answer: what was attempted, what was shown to the
    // model, what the driver did, and how every gate ruled.
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    for required in ["run_start", "inject", "driver_call", "driver_result", "gate", "run_end"] {
        assert!(kinds.contains(&required), "the stream has no {required} event: {kinds:?}");
    }

    // Every recorded gate verdict points at a result file that exists and agrees.
    for e in events.iter().filter(|e| e["kind"] == "gate") {
        let rel = e["result"].as_str().unwrap();
        let recorded: serde_json::Value =
            serde_json::from_str(&r.read(&format!(".keel/runs/{id}/{rel}"))).unwrap();
        assert_eq!(
            recorded["verdict"].as_str().unwrap(),
            e["verdict"].as_str().unwrap(),
            "the stream and the gate file disagree about {rel}"
        );
    }

    let start = &events[0];
    assert_eq!(start["kind"], "run_start");
    assert_eq!(start["spec"], "demo");
    assert!(start["store_hash"].as_str().unwrap().len() == 64, "no store hash recorded");
}

// ---------------------------------------------------------------------------
// G2
// ---------------------------------------------------------------------------

#[test]
fn a_change_outside_the_declared_scope_fails_g2() {
    let r = Repo::ready("scope-creep");
    r.install_driver(
        "wanderer",
        "#!/bin/sh\ncat > /dev/null\n\
         printf 'fn main() { /* wandered */ }\\n' > \"$KEEL_REPO/src/main.rs\"\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/main.rs\"]}'\n",
    );
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("blast-radius"), "{out}");
    assert!(out.contains("src/main.rs"), "the offending file is not named:\n{out}");
}

#[test]
fn a_change_over_the_line_budget_fails_g2() {
    let r = Repo::ready("over-budget");
    // 400 lines into an in-scope file, against a 120-line budget.
    r.install_driver(
        "verbose",
        "#!/bin/sh\ncat > /dev/null\n\
         i=0; while [ $i -lt 400 ]; do echo \"// line $i\" >> \"$KEEL_REPO/src/api/mod.rs\"; i=$((i+1)); done\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\"]}'\n",
    );
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("line-budget"), "{out}");
}

#[test]
fn a_failing_oracle_fails_g2() {
    let r = Repo::ready("oracle-fail");
    r.edit_config(|cfg| {
        cfg["oracle"]["test_cmd"] = toml::Value::String("false".into());
    });
    r.install_driver("noop", &noop_driver());
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("oracle-coverage"), "{out}");
    assert!(out.contains("AC-2"), "the failing criterion is not named:\n{out}");
}

#[test]
fn an_unconfigured_verify_step_blocks_rather_than_passes() {
    // keel cannot tell "there is no lint step" from "nobody wrote one down".
    let r = Repo::ready("no-lint");
    r.edit_config(|cfg| {
        cfg["verify"].as_table_mut().unwrap().remove("lint");
    });
    r.install_driver("worker", &working_driver());
    // Approve the merge first, so a blocked lint is the *only* thing standing
    // between this run and a pass — otherwise a failing G3 would mask it.
    r.ok(&["approve", "demo", "--stage", "merge"]);

    let (code, out) = r.run(&["run", "demo"]);
    assert!(out.contains("BLOCKED  lint"), "a missing verify step passed silently:\n{out}");
    assert_eq!(code, 3, "blocked did not survive to the run verdict:\n{out}");
    assert!(!out.contains("G2 pass"), "G2 passed with an unrun check:\n{out}");
}

#[test]
fn the_ratchet_blocks_a_regression() {
    let r = Repo::ready("ratchet");
    r.write("metric.txt", "3\n");
    r.edit_config(|cfg| {
        let mut t = toml::value::Table::new();
        t.insert("id".into(), toml::Value::String("warnings".into()));
        t.insert("cmd".into(), toml::Value::String("cat metric.txt".into()));
        t.insert("direction".into(), toml::Value::String("down".into()));
        cfg.as_table_mut().unwrap()
            .insert("ratchet".into(), toml::Value::Array(vec![toml::Value::Table(t)]));
    });
    r.ok(&["ratchet", "--accept"]);

    // Improving is fine.
    r.write("metric.txt", "1\n");
    assert_eq!(r.run(&["ratchet"]).0, 0, "an improvement was reported as a regression");

    // Regressing is not.
    r.write("metric.txt", "7\n");
    let (code, out) = r.run(&["ratchet"]);
    assert_eq!(code, 1, "the ratchet let a regression through:\n{out}");
    assert!(out.contains("REGRESSED"), "{out}");

    // And it blocks the run, even though build/test/lint are all green.
    r.install_driver("noop", &noop_driver());
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("baseline-ratchet"), "{out}");
    assert!(out.contains("3 → 7"), "the movement is not shown:\n{out}");
}

// ---------------------------------------------------------------------------
// G2.5
// ---------------------------------------------------------------------------

#[test]
fn an_added_mock_is_flagged_for_a_human_look() {
    let r = Repo::ready("mock");
    r.install_driver(
        "mocker",
        "#!/bin/sh\ncat > /dev/null\n\
         printf '#[test]\\nfn respects_config() { let _m = mock(); }\\n' > \"$KEEL_REPO/tests/limit.rs\"\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"tests/limit.rs\"]}'\n",
    );
    r.ok(&["approve", "demo", "--stage", "merge"]);
    let (code, out) = r.run(&["run", "demo"]);
    // Blocked, not failed: legitimate test doubles exist, and a check that
    // failed on every one of them would be routed around within a week.
    assert!(out.contains("test-invalidation"), "an added mock was not surfaced:\n{out}");
    assert!(out.contains("mock()"), "the offending line is not shown:\n{out}");
    assert_eq!(code, 3, "the mock did not block the run:\n{out}");
}

#[test]
fn code_changed_without_any_test_is_flagged() {
    let r = Repo::ready("no-test-moved");
    r.install_driver("worker", &code_only_driver());
    let (_, out) = r.run(&["run", "demo"]);
    assert!(out.contains("test-movement"), "{out}");
    assert!(
        out.contains("no test file did"),
        "changing code with no test change was not surfaced:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// G3 and ordering
// ---------------------------------------------------------------------------

#[test]
fn an_unreviewably_large_diff_fails_g3() {
    let r = Repo::ready("too-big");
    r.edit_config(|cfg| {
        // Allow the churn through G2 so G3 is the gate under test.
        cfg["plan"]["max_reviewable_lines"] = toml::Value::Integer(10);
    });
    let p = ".keel/specs/demo/spec.md";
    r.write(p, &r.read(p).replace("lines: 120", "lines: 5000"));
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    r.ok(&["gate", "g1", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);

    r.install_driver(
        "verbose",
        "#!/bin/sh\ncat > /dev/null\n\
         i=0; while [ $i -lt 60 ]; do echo \"// line $i\" >> \"$KEEL_REPO/src/api/mod.rs\"; i=$((i+1)); done\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\"]}'\n",
    );
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("reviewable-size"), "{out}");
}

#[test]
fn a_merge_approval_is_superseded_by_editing_the_plan() {
    let r = Repo::ready("merge-supersede");
    r.install_driver("worker", &working_driver());
    r.ok(&["approve", "demo", "--stage", "merge"]);
    assert_eq!(r.run(&["run", "demo"]).0, 0);

    // Change the agreed shape of the work after sign-off.
    let p = ".keel/specs/demo/tasks.md";
    r.write(p, &r.read(p).replace("budget: 60", "budget: 90"));

    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "a stale merge approval was honoured:\n{out}");
    assert!(out.contains("human-verdict"), "{out}");
    assert!(out.contains("changed after approval"), "{out}");
}

#[test]
fn running_without_a_passing_g1_is_refused() {
    let r = Repo::bare("no-g1");
    r.write_spec();
    r.ok(&["gate", "g0", "demo"]);
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("G1"), "{out}");
    assert!(
        std::fs::read_dir(r.dir.join(".keel/runs")).map(|d| d.count()).unwrap_or(0) == 0,
        "a run directory was created for work that was refused"
    );
}

#[test]
fn runs_are_listed_newest_last_and_latest_resolves() {
    let r = Repo::ready("runs-list");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    std::thread::sleep(std::time::Duration::from_millis(5));
    r.run(&["run", "demo"]);

    let listing = r.ok(&["runs"]);
    assert_eq!(listing.lines().count(), 2, "{listing}");
    let latest = r.ok(&["runs", "--latest"]).trim().to_string();
    assert!(listing.lines().last().unwrap().contains(&latest), "{listing} / {latest}");
}
