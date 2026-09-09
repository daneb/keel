//! `keel report` — the assembled view of a feature's whole life.
//!
//! Two properties matter more than the layout. First, `keel.report/1` joins the
//! spine, so the field names a viewer keys off are pinned here. Second, the
//! report must render a repository that is *damaged or mid-flight* without
//! either crashing or quietly pretending the damage isn't there — a viewer that
//! drops a corrupt record is exactly the gate theatre keel argues against.

mod support;

use support::Repo;

fn report_json(r: &Repo, args: &[&str]) -> serde_json::Value {
    let (_, out) = r.run(args);
    serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("keel {args:?} emitted invalid JSON: {e}\n{out}"))
}

/// Walk a spec to a gated run so the report has something to describe.
fn repo_with_a_run(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.ok(&["approve", "demo", "--stage", "plan"]);
    r.write("src/api/mod.rs", "pub fn serve() { /* limited */ }\n");
    r.write(
        "tests/limit.rs",
        "#[test]\nfn respects_config() { assert_eq!(1 + 1, 2); }\n",
    );
    r.run(&["run", "demo", "--no-driver"]);
    r
}

#[test]
fn the_report_carries_the_spine_the_approvals_and_the_runs() {
    let r = repo_with_a_run("report-shape");
    let v = report_json(&r, &["report", "demo", "--json"]);

    assert_eq!(v["schema"], "keel.report/1");
    assert!(v["generated_at"].is_string());
    assert!(v["keel_version"].is_string());

    let spec = &v["specs"][0];
    assert_eq!(spec["slug"], "demo");
    assert!(spec["stage"].is_string(), "the stage key is what a viewer renders");
    assert!(spec["complete"].is_boolean());

    // G0 and G1 belong to the spec, not to any one attempt.
    let gates: Vec<&str> = spec["gates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["gate"].as_str().unwrap())
        .collect();
    assert_eq!(gates, vec!["G0", "G1"], "spec-scoped gates missing");

    // Every approval stage appears, so a reader never guesses which exist.
    for stage in ["spec", "plan", "review", "security", "merge"] {
        assert!(spec["approvals"][stage]["state"].is_string(), "{stage} missing");
    }
    assert_eq!(spec["approvals"]["spec"]["state"], "current");

    let run = &spec["runs"][0];
    assert_eq!(run["schema"], "keel.run/1", "the run record is embedded verbatim");
    assert!(run["id"].is_string());
    assert!(run["events"].as_u64().unwrap() > 0, "the trajectory was not read");
    assert!(run["anomalies"].as_array().unwrap().is_empty());
    // G2 and beyond belong to the attempt.
    assert!(
        run["gates"].as_array().unwrap().iter().any(|g| g["gate"] == "G2"),
        "run-scoped gates missing: {}",
        run["gates"]
    );
}

#[test]
fn a_slug_that_does_not_exist_is_refused_rather_than_invented() {
    let r = Repo::bare("report-unknown");
    r.write_spec();
    let (code, out) = r.run(&["report", "nope", "--json"]);
    assert_ne!(code, 0, "a phantom spec was reported as real:\n{out}");
}

#[test]
fn with_no_slug_every_spec_is_reported() {
    let r = Repo::bare("report-all");
    r.write_spec();
    r.run(&["spec", "new", "other", "--scope", "src/api/**"]);

    let v = report_json(&r, &["report", "--json"]);
    let slugs: Vec<&str> = v["specs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["slug"].as_str().unwrap())
        .collect();
    assert!(slugs.contains(&"demo") && slugs.contains(&"other"), "{slugs:?}");
}

// ---------------------------------------------------------------------------
// degradation
// ---------------------------------------------------------------------------

#[test]
fn a_trajectory_with_a_gap_is_reported_not_hidden_and_not_fatal() {
    let r = repo_with_a_run("report-gap");
    let id = r.latest_run();
    let p = format!(".keel/runs/{id}/trajectory.jsonl");

    // Drop a line from the middle: the sequence now has a hole.
    let raw = r.read(&p);
    let kept: Vec<&str> = raw
        .lines()
        .enumerate()
        .filter(|(i, _)| *i != 2)
        .map(|(_, l)| l)
        .collect();
    r.write(&p, &format!("{}\n", kept.join("\n")));

    let (code, out) = r.run(&["report", "demo", "--json"]);
    assert_eq!(code, 0, "a damaged trajectory made the report fail:\n{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("invalid JSON");
    let run = &v["specs"][0]["runs"][0];

    let anomalies = run["anomalies"].as_array().unwrap();
    assert!(!anomalies.is_empty(), "the gap was silently swallowed");
    assert_eq!(anomalies[0]["anomaly"], "gap");
    assert!(
        run["events"].as_u64().unwrap() > 0,
        "events after the gap were discarded"
    );
}

#[test]
fn a_trajectory_caught_mid_append_is_not_reported_as_corruption() {
    let r = repo_with_a_run("report-partial");
    let id = r.latest_run();
    let p = format!(".keel/runs/{id}/trajectory.jsonl");

    // An append in flight: a half-written last line with no closing newline.
    r.write(&p, &format!("{}{{\"t\":\"2026", r.read(&p)));

    let v = report_json(&r, &["report", "demo", "--json"]);
    let anomalies = v["specs"][0]["runs"][0]["anomalies"].as_array().unwrap().clone();
    assert_eq!(anomalies.len(), 1, "{anomalies:?}");
    assert_eq!(
        anomalies[0]["anomaly"], "trailing_partial",
        "a live append was reported as corruption: {anomalies:?}"
    );
}

#[test]
fn a_run_whose_record_is_unreadable_does_not_sink_the_report() {
    let r = repo_with_a_run("report-broken-run");
    let id = r.latest_run();
    r.write(&format!(".keel/runs/{id}/run.json"), "{ this is not json");

    let (code, out) = r.run(&["report", "demo", "--json"]);
    assert_eq!(code, 0, "one broken run record failed the whole report:\n{out}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("invalid JSON");
    // The spec still reports; the unreadable run is simply absent rather than
    // guessed at.
    assert_eq!(v["specs"][0]["slug"], "demo");
    assert!(v["specs"][0]["runs"].as_array().unwrap().is_empty());
}
