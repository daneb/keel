//! Oracles for SPEC-0012 `keel-cover`.

mod support;

use serde_json::Value;
use std::path::PathBuf;
use support::{Repo, noop_driver};

fn cover(r: &Repo, extra: &[&str]) -> (i32, String) {
    let mut args = vec!["cover"];
    args.extend_from_slice(extra);
    r.run(&args)
}

fn report(r: &Repo) -> Value {
    let out = r.keel(&["cover", "--json"]);
    serde_json::from_slice(&out.stdout).expect("cover --json prints one object")
}

fn head(r: &Repo) -> String {
    report(r)["head"].as_str().unwrap().to_string()
}

/// Run the demo spec to a `pass` verdict and export its bundle into
/// `.keel/bundles/`, where `keel cover` looks. Returns the bundle path.
fn passing_bundle(r: &Repo) -> PathBuf {
    r.install_driver("noop", &noop_driver());
    r.ok(&["approve", "demo", "--stage", "merge"]);
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 0, "the run did not pass:\n{out}");
    PathBuf::from(r.ok(&["export"]).trim())
}

/// AC-1 — G2 writes the hash of the tree it judged.
#[test]
fn g2_records_the_tree_it_judged() {
    let r = Repo::ready("cover-g2");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    let run = r.latest_run();
    let recorded = r.read(&format!(".keel/runs/{run}/evidence/tree.txt"));
    assert_eq!(recorded.trim(), head(&r), "tree.txt is not the tree's content hash");
}

/// AC-2 — where the bytes live doesn't matter; what is left out doesn't count;
/// lockfiles do.
#[test]
fn content_hash_is_the_same_committed_or_not() {
    let r = Repo::ready("cover-hash");
    r.write(".gitignore", &format!("{}build/\n", r.read(".gitignore")));
    r.git(&["add", "-A"]);
    r.git(&["commit", "-q", "-m", "ignore build"]);

    r.write("src/api/limit.rs", "pub const RPM: u32 = 100;\n");
    let untracked = head(&r);
    r.git(&["add", "src/api/limit.rs"]);
    let staged = head(&r);
    r.git(&["commit", "-q", "-m", "limit"]);
    let committed = head(&r);
    assert_eq!(untracked, staged);
    assert_eq!(staged, committed);

    r.write(".keel/runs/scratch.txt", "records\n");
    r.write("CLAUDE.md", "re-rendered\n");
    r.write("build/out.bin", "ignored\n");
    assert_eq!(head(&r), committed, "keel's records, projections or ignored files moved the hash");

    r.write("Cargo.lock", "# a dependency changed\n");
    assert_ne!(head(&r), committed, "a lockfile change went unnoticed");
}

/// AC-3 — a verified bundle of a passing run of this tree covers it.
#[test]
fn a_verified_bundle_of_this_tree_covers_it() {
    let r = Repo::ready("cover-yes");
    let bundle = passing_bundle(&r);
    let (code, out) = cover(&r, &[]);
    assert_eq!(code, 0, "{out}");
    let name = bundle.file_name().unwrap().to_string_lossy().to_string();
    assert!(out.contains("covers ") && out.contains(&name), "bundle not named:\n{out}");
    assert!(out.trim_end().ends_with("covered"), "{out}");
}

/// AC-4 — nothing covers the head: exit 1, the head's hash, and each reason.
#[test]
fn an_uncovered_change_names_each_reason() {
    let r = Repo::ready("cover-reasons");
    r.install_driver("noop", &noop_driver());
    // A failing run: no merge approval, so G3 fails.
    r.run(&["run", "demo"]);
    let failing = PathBuf::from(r.ok(&["export"]).trim());
    let passing = passing_bundle(&r);
    // A corrupt copy of the passing bundle.
    let mut bytes = std::fs::read(&passing).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(r.dir.join(".keel/bundles/keel-corrupt.tar.gz"), bytes).unwrap();
    // And a change after every run.
    r.write("src/api/mod.rs", "pub fn serve() { /* changed */ }\n");

    let (code, out) = cover(&r, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains(&head(&r)), "head hash not printed:\n{out}");
    assert!(out.contains("failed verification"), "{out}");
    assert!(out.contains("run verdict fail"), "{}\n{out}", failing.display());
    assert!(out.contains("a different tree"), "{out}");
    assert!(out.trim_end().contains("uncovered"), "{out}");
}

/// AC-5 — a lockfile edit after the run means the bundle covers a different tree.
#[test]
fn an_edit_after_the_run_is_not_covered() {
    let r = Repo::ready("cover-edit");
    passing_bundle(&r);
    assert_eq!(cover(&r, &[]).0, 0);
    r.write("Cargo.lock", "# bumped after the run\n");
    let (code, out) = cover(&r, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("a different tree"), "{out}");
}

/// AC-6 — an exemption passes, and says it is an exemption.
#[test]
fn an_exemption_passes_as_exempted_not_covered() {
    let r = Repo::ready("cover-exempt");
    let (code, out) = cover(&r, &["--exempt", "docs-only change, reviewed by hand"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("exempted, not covered — docs-only change, reviewed by hand"), "{out}");
    assert!(!out.trim_end().ends_with("\ncovered"), "{out}");
}

/// AC-7 — `--json` validates against the published schema, in every verdict.
#[test]
fn json_report_validates_against_the_published_schema() {
    let schema: Value = serde_json::from_str(include_str!("../schemas/cover.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let check = |v: &Value| {
        let errors: Vec<String> = validator.iter_errors(v).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:?}\n{v:#}");
    };

    let r = Repo::ready("cover-json");
    let uncovered = report(&r);
    check(&uncovered);
    assert_eq!(uncovered["verdict"], "uncovered");

    let exempted: Value =
        serde_json::from_slice(&r.keel(&["cover", "--json", "--exempt", "why"]).stdout).unwrap();
    check(&exempted);
    assert_eq!(exempted["verdict"], "exempted");

    passing_bundle(&r);
    let covered = report(&r);
    check(&covered);
    assert_eq!(covered["verdict"], "covered");
}

/// AC-8 — the action checks out the PR head, installs a pinned keel, and runs
/// `keel cover`, turning the `keel:exempt` label into `--exempt` via env.
#[test]
fn the_action_runs_keel_cover_on_the_pr_head() {
    let action: Value = serde_yaml::from_str(include_str!("../action.yml")).unwrap();
    assert_eq!(action["runs"]["using"], "composite");
    let steps = action["runs"]["steps"].as_array().unwrap();

    let checkout = steps.iter().find(|s| s["uses"].as_str().is_some_and(|u| u.starts_with("actions/checkout@"))).unwrap();
    assert_eq!(checkout["with"]["ref"], "${{ github.event.pull_request.head.sha }}");

    let run_of = |needle: &str| steps.iter().find(|s| s["run"].as_str().is_some_and(|r| r.contains(needle)));
    let install = run_of("cargo install keel-harness").expect("no install step");
    assert!(install["run"].as_str().unwrap().contains("--version \"$KEEL_VERSION\" --locked"));
    assert_eq!(install["env"]["KEEL_VERSION"], "${{ inputs.keel-version }}");
    assert!(action["inputs"]["keel-version"]["default"].as_str().is_some_and(|v| !v.is_empty()));

    let cover = run_of("keel cover").expect("no keel cover step");
    let script = cover["run"].as_str().unwrap();
    assert!(script.contains("--exempt"), "{script}");
    assert!(!script.contains("${{"), "a PR value is interpolated into the script: {script}");
    assert!(cover["env"]["KEEL_EXEMPT"].as_str().unwrap().contains("'keel:exempt'"));
}
