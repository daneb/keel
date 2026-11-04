//! Oracles for SPEC-0021 `g3-human-judgement`.
mod support;
use sha2::{Digest, Sha256};
use support::Repo;

/// A repo whose only criterion has a `human` oracle, run once with a driver.
fn judged(name: &str) -> Repo {
    let r = Repo::bare(name);
    r.write_spec_with_oracle("oracle: human the error message names the file");
    let tasks = r.read(".keel/specs/demo/tasks.md");
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    r.ok(&["plan", "demo"]);
    r.set_rollback("git revert the merge");
    r.write(".keel/specs/demo/tasks.md", &tasks);
    r.ok(&["gate", "g1", "demo"]);
    r.install_driver("worker", "#!/bin/sh\ncat > /dev/null\necho done > \"$KEEL_REPO/src/api/done\"\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/done\"]}'\n");
    r.run(&["run", "demo"]);
    r
}

fn rerun(r: &Repo) -> String {
    r.run(&["run", "demo", "--no-driver"]).1
}

fn log(r: &Repo) -> Vec<serde_json::Value> {
    r.read(".keel/specs/demo/approvals.jsonl").lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}
#[test]
fn g3_fails_while_any_human_oracle_lacks_a_judgement() {
    let out = rerun(&judged("g3-unjudged"));
    assert!(out.contains("AC-1 unjudged — human: the error message names the file"), "{out}");
}

#[test]
fn keel_judge_records_a_hash_bound_judgement_in_the_chain() {
    let r = judged("g3-judge");
    r.ok(&["judge", "demo", "AC-1", "--pass", "--note", "reads well"]);
    let j = log(&r).into_iter().find(|a| a["stage"] == "judge:AC-1").expect("no judgement logged");
    assert_eq!((&j["decision"], &j["note"]), (&"approved".into(), &"reads well".into()));
    assert_eq!((j["run"].as_str(), j["artefact_hash"].as_str().map(str::len)), (Some(r.latest_run().as_str()), Some(64)));
    assert!(r.read(".keel/chain.jsonl").contains("\"kind\":\"judgement\""), "not in the chain");
    assert!(rerun(&r).contains("pass     human-oracles"));
}

#[test]
fn a_judgement_is_superseded_when_the_evidence_changes() {
    let r = judged("g3-superseded");
    r.ok(&["judge", "demo", "AC-1", "--pass"]);
    r.write("src/api/done", "something else entirely\n");
    let out = rerun(&r);
    assert!(out.contains("AC-1 judgement superseded") && out.contains(" → "), "{out}");
}

#[test]
fn a_failed_judgement_fails_g3_and_records_a_failure_episode() {
    let r = judged("g3-failed");
    r.ok(&["judge", "demo", "AC-1", "--fail", "--note", "names the wrong file"]);
    let out = rerun(&r);
    assert!(out.contains("AC-1 judged FAIL by Test Person: names the wrong file"), "{out}");
    assert!(r.run(&["failures", "--json"]).1.contains("human-oracles"));
}

#[test]
fn keel_review_writes_a_brief_and_the_merge_approval_records_its_hash() {
    let r = judged("g3-review");
    let out = r.ok(&["review", "demo"]);
    for needle in ["AC-1", "sensitivity", "declared:", "computed:", "G2.5 findings", "reviewable lines", "tokens spent"] {
        assert!(out.contains(needle), "brief lacks `{needle}`");
    }
    let brief = r.read(&format!(".keel/runs/{}/evidence/review-brief.md", r.latest_run()));
    r.ok(&["approve", "demo", "--stage", "merge"]);
    let merge = log(&r).into_iter().rfind(|a| a["stage"] == "merge").unwrap();
    let want: String = Sha256::digest(brief.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(merge["brief_hash"], want.as_str());
}

#[test]
fn metrics_flags_fast_approvals_and_states_it_measures_elapsed_time() {
    let r = judged("g3-metrics");
    r.ok(&["review", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);
    let out = r.ok(&["metrics"]);
    assert!(out.contains("fast") && out.contains("measures elapsed time, not attention"), "{out}");
    assert!(r.ok(&["metrics", "--json"]).contains("\"fast\": true"));
}

#[test]
fn bundle_verify_rejects_judgements_that_do_not_match_the_evidence() {
    let r = judged("g3-bundle");
    r.ok(&["judge", "demo", "AC-1", "--pass"]);
    let bundle = |r: &Repo| r.ok(&["export"]).trim().to_string();
    assert!(r.run(&["bundle", "verify", &bundle(&r)]).1.contains("pass     judgements"));

    // The manifest is honest about rewritten bytes; only the judgement can object.
    let oracles = format!(".keel/runs/{}/evidence/oracles.json", r.latest_run());
    r.write(&oracles, &format!("{} ", r.read(&oracles)));
    let (code, out) = r.run(&["bundle", "verify", &bundle(&r)]);
    assert!(code != 0 && out.contains("AC-1 judged other evidence"), "{out}");
}
