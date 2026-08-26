//! Oracles for SPEC-0003 `evidence-bundle`.

mod support;

use support::{Repo, noop_driver};

fn bundled(r: &Repo) -> (String, Vec<String>) {
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    let archive = r.ok(&["export"]).trim().to_string();
    let listing = std::process::Command::new("tar")
        .args(["-tzf", &archive])
        .output()
        .expect("listing the archive");
    let members = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();
    (archive, members)
}

/// AC-1 — `keel export` writes a single .tar.gz and prints its path on stdout.
#[test]
fn export_writes_one_archive_and_prints_its_path() {
    let r = Repo::ready("exp-one-file");
    let (archive, _) = bundled(&r);
    assert!(archive.ends_with(".tar.gz"), "not a .tar.gz: {archive}");
    let path = std::path::Path::new(&archive);
    assert!(path.is_file(), "{archive} is not a file");
    assert!(path.metadata().unwrap().len() > 0, "the bundle is empty");
    // stdout must be the path alone, so it composes with other tools.
    assert_eq!(archive.lines().count(), 1, "stdout carried more than the path");
}

/// AC-2 — the bundle contains the trajectory, every gate result, the evidence
/// files, and the spec, plan and tasks.
#[test]
fn bundle_contains_every_required_member() {
    let r = Repo::ready("exp-members");
    let (_, members) = bundled(&r);

    let has = |needle: &str| members.iter().any(|m| m.contains(needle));
    for required in [
        "README.md",
        "manifest.json",
        "trajectory.jsonl",
        "run.json",
        "gates/G2.json",
        "evidence/",
        "demo/spec.md",
        "demo/plan.md",
        "demo/tasks.md",
        "steering/conventions.md",
    ] {
        assert!(has(required), "the bundle is missing {required}:\n{members:#?}");
    }
}

/// AC-3 — the manifest carries the run id, store hash, keel version and the
/// SHA-256 of every member. Its own oracle is the JSON Schema; this checks the
/// schema file keel publishes is the one that validates.
#[test]
fn manifest_validates_against_the_published_schema() {
    let r = Repo::ready("exp-manifest");
    let (archive, _) = bundled(&r);

    let dir = r.dir.join("unpacked");
    std::fs::create_dir_all(&dir).unwrap();
    std::process::Command::new("tar")
        .args(["-xzf", &archive, "-C", dir.to_str().unwrap()])
        .status()
        .unwrap();

    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema"], "keel.manifest/1");
    assert!(!manifest["run"].as_str().unwrap().is_empty());
    assert_eq!(manifest["spec"], "demo");
    assert_eq!(manifest["store_hash"].as_str().unwrap().len(), 64);
    assert!(!manifest["keel_version"].as_str().unwrap().is_empty());
    for m in manifest["members"].as_array().unwrap() {
        assert_eq!(m["sha256"].as_str().unwrap().len(), 64, "member {m} has no full hash");
    }

    // `keel export` writes the schema; the manifest must satisfy it.
    let schema: serde_json::Value = serde_json::from_str(&r.read(".keel/schemas/manifest.json"))
        .expect(".keel/schemas/manifest.json");
    let validator = jsonschema::validator_for(&schema).expect("the published schema compiles");
    let errors: Vec<String> = validator.iter_errors(&manifest).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "the manifest does not satisfy its own schema: {errors:?}");
}

/// AC-4 — verify exits 0 on an intact bundle.
#[test]
fn verify_accepts_an_intact_bundle() {
    let r = Repo::ready("exp-verify");
    let (archive, _) = bundled(&r);
    let (code, out) = r.run(&["export", "--verify", &archive]);
    assert_eq!(code, 0, "an intact bundle failed verification:\n{out}");
    assert!(out.contains("intact"), "{out}");
}

/// AC-5 — IF a member does not match its manifest hash THEN verify exits
/// non-zero and names the member.
#[test]
fn verify_names_the_tampered_member() {
    let r = Repo::ready("exp-tamper");
    let (archive, _) = bundled(&r);

    // Unpack, edit one member, repack: the manifest still claims the old hash.
    let dir = r.dir.join("tamper");
    std::fs::create_dir_all(&dir).unwrap();
    std::process::Command::new("tar")
        .args(["-xzf", &archive, "-C", dir.to_str().unwrap()])
        .status()
        .unwrap();

    let readme = dir.join("README.md");
    let original = std::fs::read_to_string(&readme).unwrap();
    std::fs::write(&readme, format!("{original}\n\nEverything was fine, honest.\n")).unwrap();

    let repacked = r.dir.join("tampered.tar.gz");
    let status = std::process::Command::new("tar")
        .args(["-czf", repacked.to_str().unwrap(), "-C", dir.to_str().unwrap(), "."])
        .status()
        .unwrap();
    assert!(status.success());

    let (code, out) = r.run(&["export", "--verify", repacked.to_str().unwrap()]);
    assert_ne!(code, 0, "a tampered bundle verified clean:\n{out}");
    assert!(out.contains("README.md"), "the tampered member is not named:\n{out}");
    assert!(out.contains("TAMPERED"), "{out}");
}

/// AC-6's oracle is human judgement; this checks the material that judgement
/// needs is actually present, which is the part a machine can check.
#[test]
fn the_readme_states_what_changed_and_how_each_gate_ruled() {
    let r = Repo::ready("exp-readme");
    let (archive, _) = bundled(&r);

    let dir = r.dir.join("unpacked");
    std::fs::create_dir_all(&dir).unwrap();
    std::process::Command::new("tar")
        .args(["-xzf", &archive, "-C", dir.to_str().unwrap()])
        .status()
        .unwrap();

    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(readme.contains("## What changed"), "{readme}");
    assert!(readme.contains("## Gates"), "{readme}");
    for gate in ["G2", "G2.5", "G3"] {
        assert!(readme.contains(gate), "the README does not report {gate}:\n{readme}");
    }
    assert!(readme.contains("## The record"), "{readme}");
    assert!(readme.contains("export --verify"), "the README does not say how to verify it");
}
