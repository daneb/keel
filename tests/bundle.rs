//! Oracles for SPEC-0011 `bundle-v1`.

mod support;

use serde_json::Value;
use std::path::{Path, PathBuf};
use support::{BIN, Repo, noop_driver, unique_dir};

/// A repo with an approved spec, a finished run, and a chain entry after the
/// run too — so "through run_end" is distinguishable from "the whole chain".
/// The later entry is a plan approval: it changes no file the bundle ships.
fn ran(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    r.ok(&["approve", "demo", "--stage", "plan"]);
    r
}

fn export(r: &Repo, extra: &[&str]) -> PathBuf {
    let mut args = vec!["export"];
    args.extend_from_slice(extra);
    PathBuf::from(r.ok(&args).trim())
}

fn member(archive: &Path, path: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(archive).unwrap();
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for e in tar.entries().unwrap() {
        let mut e = e.unwrap();
        if e.path().unwrap().to_string_lossy().trim_start_matches("./") == path {
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut e, &mut out).unwrap();
            return Some(out);
        }
    }
    None
}

fn manifest(archive: &Path) -> Value {
    serde_json::from_slice(&member(archive, "manifest.json").unwrap()).unwrap()
}

fn lines(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(bytes).lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// Rewrite one member and re-seal the manifest over it, as someone swapping a
/// file would: the manifest check then passes, so only the joins can catch it.
fn tamper(archive: &Path, path: &str, edit: impl FnOnce(&mut Vec<u8>)) -> PathBuf {
    let dir = unique_dir("bundle-tamper");
    std::fs::create_dir_all(&dir).unwrap();
    let ok = std::process::Command::new("tar").arg("-xzf").arg(archive).arg("-C").arg(&dir).status().unwrap();
    assert!(ok.success());
    let target = dir.join(path);
    let mut bytes = std::fs::read(&target).unwrap();
    edit(&mut bytes);
    std::fs::write(&target, &bytes).unwrap();

    let mut m: Value = serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    for entry in m["members"].as_array_mut().unwrap() {
        if entry["path"] == path {
            use sha2::{Digest, Sha256};
            entry["sha256"] = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>().into();
            entry["bytes"] = bytes.len().into();
        }
    }
    std::fs::write(dir.join("manifest.json"), serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let out = dir.with_extension("tar.gz");
    let ok = std::process::Command::new("tar").arg("-czf").arg(&out).arg("-C").arg(&dir).arg(".").status().unwrap();
    assert!(ok.success());
    out
}

fn verify(archive: &Path, cwd: &Path) -> (i32, String) {
    let out = std::process::Command::new(BIN)
        .args(["bundle", "verify"])
        .arg(archive)
        .current_dir(cwd)
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// AC-1 — the repository's chain ships through this run's run_end, anchored.
#[test]
fn bundle_carries_the_chain_through_run_end() {
    let r = ran("bundle-chain");
    let run = r.latest_run();
    let archive = export(&r, &[]);

    let chain = lines(&member(&archive, "chain.jsonl").expect("no chain.jsonl in the bundle"));
    let last = chain.last().unwrap();
    assert_eq!(last["kind"], "run_end");
    assert_eq!(last["data"]["run"], run.as_str());
    assert!(lines(r.read(".keel/chain.jsonl").as_bytes()).len() > chain.len(), "later entries were shipped too");
    assert_eq!(manifest(&archive)["chain_head"], last["hash"]);
}

/// AC-2 — `--chain` ships the runtime's chain instead of the repository's.
#[test]
fn a_supplied_chain_replaces_the_local_one() {
    let r = ran("bundle-supplied");
    let local = r.read(".keel/chain.jsonl");
    // A different chain that still reaches this run's run_end: the local one
    // minus its first entry, as a runtime's would differ in what it holds.
    let supplied: String = local.lines().skip(1).map(|l| format!("{l}\n")).collect();
    r.write("runtime-chain.jsonl", &supplied);
    let archive = export(&r, &["--chain", r.dir.join("runtime-chain.jsonl").to_str().unwrap()]);

    let shipped = String::from_utf8(member(&archive, "chain.jsonl").unwrap()).unwrap();
    assert!(supplied.starts_with(&shipped), "the shipped chain is not the supplied one");
    assert!(!local.starts_with(&shipped), "the local chain was shipped");
    assert_eq!(lines(shipped.as_bytes()).last().unwrap()["kind"], "run_end");
}

/// AC-3 — G2 keeps the full patch it measured, untracked files included.
#[test]
fn g2_keeps_the_patch_it_judged() {
    let r = Repo::ready("bundle-patch");
    r.install_driver("noop", &noop_driver());
    r.write("src/api/mod.rs", "pub fn serve() { let _limit = 100; }\n");
    r.write("src/api/limit.rs", "pub const RPM: u32 = 100;\n");
    r.run(&["run", "demo"]);

    let run = r.latest_run();
    let patch = r.read(&format!(".keel/runs/{run}/evidence/diff.patch"));
    assert!(patch.contains("src/api/mod.rs") && patch.contains("+pub fn serve() { let _limit = 100; }"), "{patch}");
    assert!(patch.contains("src/api/limit.rs") && patch.contains("+pub const RPM: u32 = 100;"), "untracked file missing:\n{patch}");
}

/// AC-4 — an untouched bundle passes every check.
#[test]
fn an_intact_bundle_verifies() {
    let r = ran("bundle-intact");
    let archive = export(&r, &[]);
    let (code, out) = verify(&archive, &r.dir);
    assert_eq!(code, 0, "{out}");
    for id in ["members", "chain", "approvals", "gate-verdicts", "trajectory"] {
        assert!(out.contains(id), "check `{id}` not printed:\n{out}");
    }
}

/// AC-5 — a spec swapped after approval fails, naming the stage.
#[test]
fn a_swapped_spec_fails_its_approval() {
    let r = ran("bundle-swap");
    let archive = export(&r, &[]);
    let swapped = tamper(&archive, "demo/spec.md", |b| b.extend_from_slice(b"\nAn extra requirement nobody approved.\n"));
    let (code, out) = verify(&swapped, &r.dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("approvals") && out.contains("spec ("), "stage not named:\n{out}");
    assert!(out.contains("members") && !out.contains("tampered"), "the manifest caught it, not the join:\n{out}");
}

/// AC-6 — a replaced verdict or trajectory is named.
#[test]
fn a_replaced_verdict_or_trajectory_is_named() {
    let r = ran("bundle-replace");
    let archive = export(&r, &[]);

    let verdict = tamper(&archive, "gates/G2.json", |b| {
        let mut v: Value = serde_json::from_slice(b).unwrap();
        v["verdict"] = "pass".into();
        v["checks"] = Value::Array(vec![]);
        *b = serde_json::to_vec_pretty(&v).unwrap();
    });
    let (code, out) = verify(&verdict, &r.dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("gates/G2.json"), "replaced verdict not named:\n{out}");

    let trajectory = tamper(&archive, "trajectory.jsonl", |b| b.truncate(b.len() / 2));
    let (code, out) = verify(&trajectory, &r.dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("trajectory.jsonl differs"), "trajectory not named:\n{out}");

    // Human decisions may follow run_end; anything else may not.
    let appended = tamper(&archive, "trajectory.jsonl", |b| {
        b.extend_from_slice(br#"{"t":"2026-09-24T00:00:00+00:00","seq":999,"kind":"driver_call","driver":"x","prompt_tokens":1}"#);
        b.push(b'\n');
    });
    let (code, out) = verify(&appended, &r.dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("`driver_call` event after run_end"), "{out}");
}

/// AC-7 — `--json` prints one report that validates against the schema.
#[test]
fn json_report_validates_against_the_published_schema() {
    let r = ran("bundle-json");
    let archive = export(&r, &[]);
    let out = std::process::Command::new(BIN).args(["bundle", "verify", "--json"]).arg(&archive).output().unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).expect("stdout is one JSON object");

    let schema: Value = serde_json::from_str(include_str!("../schemas/bundleverify.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator.iter_errors(&report).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:?}\n{report:#}");
    assert_eq!(report["verdict"], "pass");
    assert_eq!(report["checks"].as_array().unwrap().len(), 5);
}

/// AC-8 — no repository, no .keel/, no HOME, no reachable network: same verdict.
#[test]
fn verifies_with_nothing_but_the_bundle() {
    let r = ran("bundle-offline");
    let archive = export(&r, &[]);
    let (in_repo_code, in_repo_out) = verify(&archive, &r.dir);

    let bare = unique_dir("bundle-bare");
    std::fs::create_dir_all(&bare).unwrap();
    let copy = bare.join("evidence.tar.gz");
    std::fs::copy(&archive, &copy).unwrap();
    let dead = "http://127.0.0.1:9";
    let out = std::process::Command::new(BIN)
        .args(["bundle", "verify"])
        .arg(&copy)
        .current_dir(&bare)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HTTP_PROXY", dead)
        .env("HTTPS_PROXY", dead)
        .env("ALL_PROXY", dead)
        .output()
        .unwrap();
    let bare_out = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = std::fs::remove_dir_all(&bare);

    assert_eq!(out.status.code(), Some(in_repo_code), "{bare_out}\n---\n{in_repo_out}");
    assert_eq!(in_repo_code, 0, "{in_repo_out}");
    let tail = |s: &str| s.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(tail(&bare_out), tail(&in_repo_out), "a different verdict away from the repository");
}
