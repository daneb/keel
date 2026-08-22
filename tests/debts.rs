//! Tests for the roadmap debts: the adversarial reviewer, run pruning, and the
//! oracle kinds that parsed but had never run.

mod support;

use support::{Repo, noop_driver};

/// A reviewer that reports whatever findings it is given, verbatim.
fn scripted_reviewer(findings_json: &str) -> String {
    format!(
        "#!/bin/sh\ncat > /dev/null\n\
         echo '{{\"schema\":\"keel.reviewresult/1\",\"findings\":{findings_json},\"summary\":\"scripted\"}}'\n"
    )
}

fn install_reviewer(r: &Repo, script: &str, advisory: bool) {
    let path = r.dir.join(".keel/reviewers/test");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&path).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&path, p).unwrap();
    }
    r.edit_config(|cfg| {
        let mut t = toml::value::Table::new();
        t.insert("cmd".into(), toml::Value::String(".keel/reviewers/test".into()));
        t.insert("timeout_secs".into(), toml::Value::Integer(10));
        t.insert("advisory".into(), toml::Value::Boolean(advisory));
        cfg.as_table_mut().unwrap().insert("review".into(), toml::Value::Table(t));
    });
}

/// A driver that makes an in-scope change plus a test change, so G2 passes and
/// G2.5 is the gate under test.
fn working_driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     printf 'pub fn serve() { /* limited */ }\\n' > \"$KEEL_REPO/src/api/mod.rs\"\n\
     printf '#[test]\\nfn respects_config() { assert_eq!(1 + 1, 2); }\\n' > \"$KEEL_REPO/tests/limit.rs\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\",\"tests/limit.rs\"]}'\n"
        .to_string()
}

fn ready(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.install_driver("worker", &working_driver());
    r.ok(&["approve", "demo", "--stage", "merge"]);
    r
}

// ---------------------------------------------------------------------------
// the adversarial reviewer
// ---------------------------------------------------------------------------

#[test]
fn without_a_reviewer_the_heuristics_still_stand() {
    let r = ready("no-reviewer");
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no adversarial reviewer configured"), "{out}");
    // The heuristic checks must not have been replaced by the absent reviewer.
    assert!(out.contains("test-invalidation"), "{out}");
    assert!(out.contains("test-movement"), "{out}");
}

#[test]
fn a_reviewer_finding_fails_the_gate_and_names_the_place() {
    let r = ready("reviewer-fails");
    install_reviewer(
        &r,
        &scripted_reviewer(
            r#"[{"id":"test-invalidation","severity":"fail","detail":"respects_config no longer asserts the limit","file":"tests/limit.rs","line":2}]"#,
        ),
        false,
    );
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "a reviewer defect did not fail the gate:\n{out}");
    assert!(out.contains("review:test-invalidation"), "{out}");
    assert!(out.contains("tests/limit.rs:2"), "the finding lost its location:\n{out}");
    assert!(out.contains("no longer asserts"), "{out}");

    // And it is on the record as evidence.
    let id = r.latest_run();
    let review: serde_json::Value =
        serde_json::from_str(&r.read(&format!(".keel/runs/{id}/evidence/review.json"))).unwrap();
    assert_eq!(review["findings"][0]["id"], "test-invalidation");
}

#[test]
fn advisory_mode_surfaces_findings_without_blocking_a_merge() {
    // The honest setting while you learn whether to trust a reviewer.
    let r = ready("reviewer-advisory");
    install_reviewer(
        &r,
        &scripted_reviewer(
            r#"[{"id":"scope-creep","severity":"fail","detail":"this looks unrelated","file":"src/api/mod.rs"}]"#,
        ),
        true,
    );
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 3, "advisory findings should block for a look, not fail:\n{out}");
    assert!(out.contains("BLOCKED  review:scope-creep"), "{out}");
}

#[test]
fn a_concern_is_a_look_not_a_refusal() {
    let r = ready("reviewer-concern");
    install_reviewer(
        &r,
        &scripted_reviewer(
            r#"[{"id":"missing-coverage","severity":"concern","detail":"AC-2 has no test change"}]"#,
        ),
        false,
    );
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("BLOCKED  review:missing-coverage"), "{out}");
}

#[test]
fn a_clean_review_passes_and_reports_how_long_it_took() {
    let r = ready("reviewer-clean");
    install_reviewer(&r, &scripted_reviewer("[]"), false);
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("pass     reviewer"), "{out}");
    assert!(out.contains("s)"), "the review duration is not reported:\n{out}");
}

#[test]
fn a_reviewer_that_cannot_run_blocks_rather_than_passes() {
    let r = ready("reviewer-missing");
    r.edit_config(|cfg| {
        let mut t = toml::value::Table::new();
        t.insert("cmd".into(), toml::Value::String("./not-a-reviewer".into()));
        t.insert("timeout_secs".into(), toml::Value::Integer(5));
        cfg.as_table_mut().unwrap().insert("review".into(), toml::Value::Table(t));
    });
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 3, "a missing reviewer passed silently:\n{out}");
    assert!(out.contains("BLOCKED  reviewer"), "{out}");
    assert!(out.contains("could not start"), "{out}");
}

#[test]
fn a_reviewer_that_prints_nonsense_blocks_rather_than_fails() {
    // Its output being unreadable says nothing about the change.
    let r = ready("reviewer-nonsense");
    install_reviewer(&r, "#!/bin/sh\ncat > /dev/null\necho 'I have opinions'\n", false);
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 3, "{out}");
    assert!(out.contains("BLOCKED  reviewer"), "{out}");
    assert!(out.contains("not JSON"), "{out}");
}

#[test]
fn the_reviewer_is_given_the_diff_conventions_and_lessons() {
    let r = ready("reviewer-request");
    install_reviewer(
        &r,
        "#!/bin/sh\ncat > \"$KEEL_REPO/received-review.json\"\n\
         echo '{\"schema\":\"keel.reviewresult/1\",\"findings\":[]}'\n",
        false,
    );
    r.write(
        ".keel/store/lessons/L-0001.md",
        "---\nid: L-0001\nschema: keel.lesson/1\nclass: SCOPE-CREEP\nscope: repo\n\
         occurrences: 2\nrule_kind: prompt-injection\nverified_at: 2999-01-01\ndecay: 90d\n\
         sources: []\nstages:\n  - implement\n---\n\n**Rule** Stay in the declared scope.\n",
    );
    r.sync_store();
    let (_, run_out) = r.run(&["run", "demo"]);
    assert!(
        r.exists("received-review.json"),
        "the reviewer was never invoked:\n{run_out}"
    );

    let req: serde_json::Value =
        serde_json::from_str(&r.read("received-review.json")).expect("the request was not JSON");
    assert_eq!(req["schema"], "keel.reviewrequest/1");
    assert!(req["diff"].as_str().unwrap().contains("src/api/mod.rs"), "no diff was sent");
    assert!(!req["conventions"].as_str().unwrap().is_empty(), "no conventions were sent");
    assert!(
        req["lessons"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().contains("L-0001")),
        "the lessons in force were not sent: {}", req["lessons"]
    );
    assert!(
        req["criteria"].as_array().unwrap().iter().any(|c| c.as_str().unwrap().contains("AC-1")),
        "the criteria were not sent"
    );
    assert!(req["prompt"].as_str().unwrap().contains("test-invalidation"));
}

// ---------------------------------------------------------------------------
// run pruning
// ---------------------------------------------------------------------------

#[test]
fn pruning_never_removes_a_run_a_lesson_cites() {
    let r = ready("prune-provenance");
    for _ in 0..3 {
        r.run(&["run", "demo"]);
    }
    let ids = r.ok(&["runs"]);
    let cited = ids.lines().next().unwrap().split_whitespace().next().unwrap().to_string();

    r.write(
        ".keel/store/lessons/L-0001.md",
        &format!(
            "---\nid: L-0001\nschema: keel.lesson/1\nclass: SCOPE-CREEP\nscope: repo\n\
             occurrences: 2\nrule_kind: prompt-injection\nverified_at: 2999-01-01\ndecay: 90d\n\
             sources:\n  - runs/{cited}\nstages:\n  - implement\n---\n\n**Rule** Stay in scope.\n"
        ),
    );

    let plan = r.ok(&["runs", "--prune", "--keep", "1"]);
    assert!(plan.contains(&cited), "{plan}");
    assert!(plan.contains("KEPT — cited by a lesson"), "{plan}");
    assert!(plan.contains("re-run with --apply"), "a dry run deleted something");

    r.ok(&["runs", "--prune", "--keep", "1", "--apply"]);
    assert!(r.exists(&format!(".keel/runs/{cited}")), "a cited run was pruned");
    // The lesson's provenance still resolves.
    let lesson = r.read(".keel/store/lessons/L-0001.md");
    assert!(lesson.contains(&cited));
}

#[test]
fn pruning_keeps_the_recent_window_and_removes_the_rest() {
    let r = ready("prune-window");
    for _ in 0..4 {
        r.run(&["run", "demo"]);
    }
    let before = r.ok(&["runs"]).lines().count();
    assert!(before >= 4);

    r.ok(&["runs", "--prune", "--keep", "2", "--apply"]);
    let after = r.ok(&["runs"]).lines().count();
    assert_eq!(after, 2, "expected the 2 most recent to survive, got {after}");
}

#[test]
fn pruning_with_nothing_to_do_says_so() {
    let r = ready("prune-noop");
    r.run(&["run", "demo"]);
    let out = r.ok(&["runs", "--prune", "--keep", "50"]);
    assert!(out.contains("nothing to prune"), "{out}");
}

// ---------------------------------------------------------------------------
// oracle kinds that had never run
// ---------------------------------------------------------------------------

#[test]
fn the_schema_oracle_kind_actually_validates() {
    let r = Repo::ready("oracle-schema");
    r.write(
        "schemas/thing.json",
        r#"{"$schema":"https://json-schema.org/draft/2020-12/schema",
            "type":"object","required":["name"],
            "properties":{"name":{"type":"string"}},"additionalProperties":false}"#,
    );
    r.write("thing.json", r#"{"name":"valid"}"#);
    r.write_spec_with_oracle("oracle: schema `schemas/thing.json` validates `thing.json`");
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    r.ok(&["plan", "demo"]);
    r.set_rollback("git revert");
    r.ok(&["gate", "g1", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);
    r.install_driver("noop", &noop_driver());

    let (_, out) = r.run(&["run", "demo"]);
    assert!(out.contains("oracle-coverage"), "{out}");
    assert!(!out.contains("BLOCKED  oracle-coverage"), "a valid document blocked:\n{out}");

    // Break the document; the oracle must fail, not block.
    r.write("thing.json", r#"{"name":42,"extra":true}"#);
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "an invalid document passed its schema oracle:\n{out}");
    assert!(out.contains("oracle-coverage"), "{out}");
}

#[test]
fn the_doctest_oracle_kind_runs_its_configured_command() {
    let r = Repo::ready("oracle-doctest");
    r.write_spec_with_oracle("oracle: doctest src/api/mod.rs");
    r.edit_config(|cfg| {
        // The template takes {path}; prove it is substituted and executed.
        cfg["oracle"]["doctest_cmd"] =
            toml::Value::String("test -f {path} && grep -q serve {path}".into());
    });
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    r.ok(&["plan", "demo"]);
    r.set_rollback("git revert");
    r.ok(&["gate", "g1", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);
    r.install_driver("noop", &noop_driver());

    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 0, "a passing doctest oracle did not pass:\n{out}");

    // Point it at a file that does not satisfy the command.
    r.edit_config(|cfg| {
        cfg["oracle"]["doctest_cmd"] = toml::Value::String("grep -q NOTHING_HERE {path}".into());
    });
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "a failing doctest oracle did not fail:\n{out}");
}
