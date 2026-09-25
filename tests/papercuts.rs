//! Oracles for SPEC-0013 `papercuts`.

mod support;

use serde_json::Value;
use support::{Repo, noop_driver};

fn g25_check(r: &Repo, id: &str) -> Value {
    let run = r.latest_run();
    let g: Value = serde_json::from_str(&r.read(&format!(".keel/runs/{run}/gates/G2.5.json"))).unwrap();
    g["checks"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap().clone()
}

/// A repo whose run changes code and no test.
fn code_only(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.install_driver("noop", &noop_driver());
    r.write("src/api/mod.rs", "pub fn serve() { let _limit = 100; }\n");
    r.run(&["run", "demo"]);
    r
}

/// AC-1 — flagged, blocked, the files named, and told how to clear it.
#[test]
fn a_code_only_change_is_flagged_for_review() {
    let r = code_only("papercut-flag");
    let check = g25_check(&r, "test-movement");
    assert_eq!(check["verdict"], "blocked", "{check}");
    assert!(check["detail"].as_str().unwrap().contains("keel approve --stage review demo"), "{check}");
    let flags = r.read(".keel/specs/demo/review-flags.txt");
    assert!(flags.contains("test-movement: no test changed with src/api/mod.rs"), "{flags}");
}

/// AC-2 — a current review sign-off passes it, naming the reviewer.
#[test]
fn a_review_sign_off_clears_test_movement() {
    let r = code_only("papercut-clear");
    r.ok(&["approve", "demo", "--stage", "review"]);
    r.run(&["run", "demo"]);
    let check = g25_check(&r, "test-movement");
    assert_eq!(check["verdict"], "pass", "{check}");
    assert!(check["detail"].as_str().unwrap().contains("reviewed and accepted by Test Person"), "{check}");
}

/// AC-3 — a different change needs a fresh review.
#[test]
fn a_new_change_supersedes_the_review() {
    let r = code_only("papercut-supersede");
    r.ok(&["approve", "demo", "--stage", "review"]);
    r.write("src/api/limit.rs", "pub const RPM: u32 = 100;\n");
    r.run(&["run", "demo"]);
    let check = g25_check(&r, "test-movement");
    assert_eq!(check["verdict"], "blocked", "the old review covered a change it never saw: {check}");
    assert!(r.read(".keel/specs/demo/review-flags.txt").contains("src/api/limit.rs"));
}

/// AC-4 — a bundle that cannot be verified is called blocked, not failed.
#[test]
fn a_chainless_bundle_is_reported_blocked() {
    let r = Repo::ready("papercut-blocked");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    std::fs::remove_file(r.dir.join(".keel/chain.jsonl")).unwrap();
    r.ok(&["export"]);
    let (code, out) = r.run(&["cover"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("verification blocked (chain"), "{out}");
    assert!(!out.contains("failed verification"), "{out}");
}
