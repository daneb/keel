//! Oracles for SPEC-0014 `ci-runtime`.

mod support;

use serde_json::{Value, json};
use std::path::Path;
use support::{BIN, unique_dir};

fn keel(args: &[&str], env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = std::process::Command::new(BIN);
    cmd.args(args);
    for k in ["GITHUB_REPOSITORY", "GITHUB_WORKFLOW", "GITHUB_RUN_ID", "GITHUB_SHA"] {
        cmd.env_remove(k);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn entries(chain: &Path) -> Vec<Value> {
    std::fs::read_to_string(chain).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// A container as `docker create` in runtime/action.yml makes it.
fn inspect(runtime: &str, network: &str) -> Value {
    json!([{
        "Image": "sha256:feedface",
        "Config": { "User": "1001:1001" },
        "HostConfig": {
            "Runtime": runtime,
            "ReadonlyRootfs": true,
            "Privileged": false,
            "CapDrop": ["ALL"],
            "SecurityOpt": ["no-new-privileges"],
        },
        "Mounts": [
            { "Type": "bind", "Destination": "/workspace" },
            { "Type": "bind", "Destination": "/run/keel-sink" },
        ],
        "NetworkSettings": { "Networks": { network: {} } },
    }])
}

fn attest(dir: &Path, runtime: &str, network: &str, env: &[(&str, &str)]) -> (i32, String, Value) {
    let insp = dir.join("inspect.json");
    std::fs::write(&insp, inspect(runtime, network).to_string()).unwrap();
    let out = dir.join("posture/posture.json");
    let (code, text) = keel(
        &[
            "runtime", "attest",
            "--inspect", insp.to_str().unwrap(),
            "--chain", dir.join("chain.jsonl").to_str().unwrap(),
            "--out", out.to_str().unwrap(),
        ],
        env,
    );
    let doc = std::fs::read_to_string(&out).map(|t| serde_json::from_str(&t).unwrap()).unwrap_or(Value::Null);
    (code, text, doc)
}

fn fresh(name: &str) -> std::path::PathBuf {
    let d = unique_dir(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// AC-1 — payloads folded once, under the writer; bad lines kept.
#[test]
fn fold_appends_payloads_once_with_the_writer() {
    let d = fresh("ci-fold");
    let (sink, chain) = (d.join("sink.jsonl"), d.join("chain.jsonl"));
    std::fs::write(&sink, "{\"schema\":\"keel.chain/1\",\"kind\":\"gate\",\"data\":{\"gate\":\"G2\"}}\nnot json\n").unwrap();
    let fold = || keel(&["runtime", "fold", "--sink", sink.to_str().unwrap(), "--chain", chain.to_str().unwrap(), "--writer", "github-actions"], &[]);

    assert_eq!(fold().0, 0);
    std::fs::write(&sink, format!("{}{{\"kind\":\"run_end\",\"data\":{{\"run\":\"r\"}}}}\n", std::fs::read_to_string(&sink).unwrap())).unwrap();
    assert_eq!(fold().0, 0);

    let e = entries(&chain);
    let kinds: Vec<&str> = e.iter().map(|x| x["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["gate", "sink_malformed", "run_end"], "folded twice or lost a line");
    assert!(e.iter().all(|x| x["writer"] == "github-actions" && x["data"]["source"] == "sandbox"));
    let _ = std::fs::remove_dir_all(&d);
}

/// AC-2 — a keel.posture/1 document derived from inspect, its hash on the chain.
#[test]
fn attest_derives_posture_from_inspect_output() {
    let d = fresh("ci-attest");
    let (code, out, doc) = attest(&d, "runsc", "bridge", &[]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(doc["schema"], "keel.posture/1");
    assert_eq!(doc["runtime"], "github-actions");
    let p = &doc["properties"];
    for good in ["fs.read_only_root", "privileged.off", "caps.dropped_all", "privileges.no_new", "user.non_root"] {
        assert_eq!(p[good]["status"], "proven", "{good}: {}", p[good]);
    }
    assert_eq!(p["mounts.no_host_bind"]["status"], "violated");
    assert!(p["mounts.no_host_bind"]["evidence"].as_str().unwrap().contains("/workspace"));
    assert_eq!(p["network.internal_only"]["status"], "violated", "bridge is not internal");

    let text = std::fs::read_to_string(d.join("posture/posture.json")).unwrap();
    let e = entries(&d.join("chain.jsonl"));
    assert_eq!(e[0]["kind"], "attest");
    use sha2::{Digest, Sha256};
    let sha: String = Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(e[0]["data"]["sha256"], sha.as_str());

    let (_, _, none) = attest(&fresh("ci-attest-none"), "runsc", "none", &[]);
    assert_eq!(none["properties"]["network.internal_only"]["status"], "proven", "--network none");
    let _ = std::fs::remove_dir_all(&d);
}

/// AC-3 — kernel.isolated is proven only under runsc.
#[test]
fn kernel_isolated_is_proven_only_under_runsc() {
    let (_, _, gvisor) = attest(&fresh("ci-runsc"), "runsc", "none", &[]);
    assert_eq!(gvisor["properties"]["kernel.isolated"]["status"], "proven");
    let (_, _, runc) = attest(&fresh("ci-runc"), "runc", "none", &[]);
    assert_eq!(runc["properties"]["kernel.isolated"]["status"], "unproven");
    assert!(runc["properties"]["kernel.isolated"]["evidence"].as_str().unwrap().contains("runc"));
}

/// AC-4 — the run's identity is recorded, as the runner's claim.
#[test]
fn run_identity_is_recorded_as_a_claim() {
    let env = [
        ("GITHUB_REPOSITORY", "daneb/keel-cover-demo"),
        ("GITHUB_WORKFLOW", "keel runtime"),
        ("GITHUB_RUN_ID", "12345"),
        ("GITHUB_SHA", "abc123"),
    ];
    let d = fresh("ci-identity");
    let (code, out, doc) = attest(&d, "runsc", "none", &env);
    assert_eq!(code, 0, "{out}");
    let id = &doc["runtime_identity"];
    assert_eq!(id["claimed_by"], "runner");
    assert_eq!(id["repository"], "daneb/keel-cover-demo");
    assert_eq!(id["run_id"], "12345");
    assert_eq!(entries(&d.join("chain.jsonl"))[0]["data"]["runtime_identity"]["sha"], "abc123");

    let (_, _, bare) = attest(&fresh("ci-no-identity"), "runsc", "none", &[]);
    assert!(bare.get("runtime_identity").is_none(), "an identity invented from nothing");
}

fn action() -> Value {
    serde_yaml::from_str(include_str!("../runtime/action.yml")).unwrap()
}

fn script_with(needle: &str) -> String {
    let a = action();
    a["runs"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["run"].as_str())
        .find(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("no step runs {needle}"))
        .to_string()
}

/// AC-5 — the container is hardened, and $RUNNER_TEMP is out of its reach
/// except the sink and the read-only attestation.
#[test]
fn the_action_hardens_the_container_and_keeps_host_files_out() {
    let s = script_with("docker create");
    for flag in [
        "--read-only",
        "--cap-drop ALL",
        "--security-opt no-new-privileges",
        "--user \"$(id -u):$(id -g)\"",
        "--network \"$NETWORK\"",
        "RUNTIME_FLAG=(--runtime runsc)",
    ] {
        assert!(s.contains(flag), "missing {flag}:\n{s}");
    }
    let mounts: Vec<&str> = s.lines().filter(|l| l.contains("-v \"")).collect();
    let host_mounts: Vec<&&str> = mounts.iter().filter(|l| l.contains("$KEEL_HOST")).collect();
    assert_eq!(host_mounts.len(), 2, "{mounts:?}");
    assert!(host_mounts.iter().any(|l| l.contains("/sink:/run/keel-sink\"")));
    assert!(host_mounts.iter().any(|l| l.contains("/posture:/run/keel-posture:ro\"")));
    assert!(!s.contains("chain.jsonl"), "the host chain is mounted:\n{s}");
    assert_eq!(action()["inputs"]["network"]["default"], "bridge");
}

/// AC-6 — the host chain starts as a verified copy of the repository's.
#[test]
fn the_action_starts_from_the_verified_repository_chain() {
    let s = script_with("chain verify");
    let verify = s.find("chain verify").unwrap();
    let copy = s.find("cp .keel/chain.jsonl \"$H/chain.jsonl\"").expect("no copy of the repository chain");
    assert!(verify < copy, "copied before verifying:\n{s}");
    assert!(s.contains("set -euo pipefail"), "a failed verify would not stop the job");
    assert!(script_with("runtime fold").contains("--chain \"$KEEL_HOST/chain.jsonl\""));
}

/// AC-7 — fold, export with the host chain, verify or stop, commit back.
#[test]
fn the_action_builds_verifies_and_commits_the_bundle() {
    let s = script_with("runtime fold");
    let order = ["runtime fold", "export --chain \"$KEEL_HOST/chain.jsonl\"", "bundle verify \"$BUNDLE\""];
    let at: Vec<usize> = order.iter().map(|n| s.find(n).unwrap_or_else(|| panic!("missing {n}:\n{s}"))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "out of order:\n{s}");
    assert!(s.contains("set -euo pipefail"));

    let commit = script_with("git push");
    assert!(commit.contains("git add -f \".keel/bundles/"), "{commit}");
    assert!(commit.contains("github-actions[bot]"));
    let checkout = &action()["runs"]["steps"][0];
    assert_eq!(checkout["with"]["ref"], "${{ github.head_ref }}", "can't push to a detached head");
    assert_eq!(checkout["with"]["token"], "${{ inputs.token }}");
}
