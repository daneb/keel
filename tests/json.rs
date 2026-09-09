//! The `--json` surfaces added for external tools.
//!
//! These are wire contracts, not conveniences: anything reading keel from the
//! outside — a viewer, a CI bot, an editor plugin — depends on the shape, and
//! the spine freeze makes a schema additive-only once it ships. So the fields
//! that other things key off are pinned here rather than left to drift.

mod support;

use support::Repo;

fn json(r: &Repo, args: &[&str]) -> serde_json::Value {
    let (_, out) = r.run(args);
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("keel {args:?} emitted invalid JSON: {e}\n{out}"))
}

#[test]
fn next_json_names_the_stage_and_the_command_that_advances_it() {
    let r = Repo::bare("json-next");
    r.write_spec();

    let v = json(&r, &["next", "--json"]);
    assert_eq!(v["schema"], "keel.next/1");
    assert_eq!(v["specs"][0]["slug"], "demo");
    assert_eq!(v["specs"][0]["stage"], "spec", "G0 has not run yet");
    assert_eq!(v["specs"][0]["command"], "keel gate g0 demo");
    assert_eq!(v["specs"][0]["complete"], false);

    r.ok(&["gate", "g0", "demo"]);
    let v = json(&r, &["next", "--json"]);
    assert_eq!(v["specs"][0]["stage"], "spec_approval");
    assert_eq!(v["specs"][0]["command"], "keel approve demo --stage spec");
}

/// The stage key is a schema value; the label is prose. They are allowed to
/// differ, and the wire must carry the stable one.
#[test]
fn next_json_uses_stable_keys_not_display_labels() {
    let r = Repo::bare("json-next-keys");
    r.write_spec();
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    r.ok(&["plan", "demo"]);
    r.set_rollback("git revert the merge");
    r.write_tasks();

    let v = json(&r, &["next", "--json"]);
    assert_eq!(v["specs"][0]["stage"], "plan_gate", "expected the machine key");
}

#[test]
fn next_json_reports_an_uninitialised_repo_as_a_blocker_not_an_error() {
    let dir = support::unique_dir("json-next-bare");
    std::fs::create_dir_all(&dir).unwrap();
    let out = std::process::Command::new(support::BIN)
        .args(["next", "--json"])
        .current_dir(&dir)
        .output()
        .expect("running keel");
    assert!(out.status.success(), "an uninitialised repo should not be an error");
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("invalid JSON outside a keel repo");
    assert_eq!(v["schema"], "keel.next/1");
    assert_eq!(v["blockers"][0]["reason"], "not initialised");
    assert_eq!(v["blockers"][0]["command"], "keel init");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn runs_json_carries_the_run_records() {
    let r = Repo::bare("json-runs");
    let v = json(&r, &["runs", "--json"]);
    assert_eq!(v["schema"], "keel.runs/1");
    assert_eq!(v["runs"].as_array().unwrap().len(), 0, "a fresh repo has no runs");
}

#[test]
fn approvals_json_carries_history_and_every_stage_standing() {
    let r = Repo::bare("json-approvals");
    r.write_spec();
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec", "--note", "criteria are falsifiable"]);

    let v = json(&r, &["approvals", "demo", "--json"]);
    assert_eq!(v["schema"], "keel.approvals/1");
    assert_eq!(v["spec"], "demo");
    assert_eq!(v["history"][0]["stage"], "spec");
    assert_eq!(v["history"][0]["note"], "criteria are falsifiable");
    assert_eq!(v["standing"]["spec"]["state"], "current");

    // Every stage is present, so a reader never has to guess which exist.
    for stage in ["spec", "plan", "review", "security", "merge"] {
        assert!(v["standing"][stage].is_object(), "{stage} missing from standing");
    }

    // Editing the artefact must show up as superseded, not as still approved.
    let p = ".keel/specs/demo/spec.md";
    r.write(p, &r.read(p).replace("HTTP 429", "HTTP 503"));
    let v = json(&r, &["approvals", "demo", "--json"]);
    assert_eq!(v["standing"]["spec"]["state"], "superseded");
    assert!(v["standing"]["spec"]["current_hash"].is_string());
}
