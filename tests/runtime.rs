//! Oracles for SPEC-0010 `runtime-contract`.

mod support;

use serde_json::Value;
use support::{BIN, Repo};

const MARKER: &str = "driver-saw-chain.jsonl";

/// A repo whose driver, when invoked, snapshots the chain as it stood at that
/// moment. The snapshot's existence is the proof the driver ran; its contents
/// are the proof of what was recorded before it did.
fn repo(name: &str, require: &[&str]) -> Repo {
    let r = Repo::ready(name);
    let dir = r.dir.display();
    r.install_driver(
        "spy",
        &format!(
            "#!/bin/sh\ncat > /dev/null\ncp '{dir}/.keel/chain.jsonl' '{dir}/{MARKER}'\n\
             echo '{{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[],\"tokens\":1}}'\n"
        ),
    );
    let require: Vec<toml::Value> = require.iter().map(|s| toml::Value::String(s.to_string())).collect();
    r.edit_config(|cfg| {
        let mut rt = toml::value::Table::new();
        rt.insert("require".into(), toml::Value::Array(require));
        cfg.as_table_mut().unwrap().insert("runtime".into(), toml::Value::Table(rt));
    });
    r
}

fn attestation(r: &Repo, properties: &[(&str, &str)]) -> String {
    let props: serde_json::Map<String, Value> = properties
        .iter()
        .map(|(k, s)| (k.to_string(), serde_json::json!({ "status": s, "evidence": "docker inspect" })))
        .collect();
    let doc = serde_json::json!({ "schema": "keel.posture/1", "runtime": "fake", "properties": props });
    write_attestation(r, &doc.to_string())
}

fn write_attestation(r: &Repo, raw: &str) -> String {
    r.write("attestation.json", raw);
    r.dir.join("attestation.json").display().to_string()
}

/// `keel run` with the attestation env var set (or cleared, for `None`).
fn run(r: &Repo, args: &[&str], attest: Option<&str>) -> (i32, String) {
    let mut cmd = std::process::Command::new(BIN);
    cmd.args(args).current_dir(&r.dir).env_remove("KEEL_CHAIN_SINK").env_remove("KEEL_RUNTIME_ATTESTATION");
    if let Some(path) = attest {
        cmd.env("KEEL_RUNTIME_ATTESTATION", path);
    }
    let out = cmd.output().expect("running keel");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn driver_ran(r: &Repo) -> bool {
    r.exists(MARKER)
}

fn posture_gate(r: &Repo) -> Option<Value> {
    let run = r.latest_run();
    let p = format!(".keel/runs/{run}/gates/posture.json");
    r.exists(&p).then(|| serde_json::from_str(&r.read(&p)).unwrap())
}

/// AC-1 — the attestation is in the chain, with its hash and properties, by the
/// time the driver is invoked.
#[test]
fn attest_entry_precedes_the_driver() {
    let r = repo("rt-order", &[]);
    let path = attestation(&r, &[("egress.allowlist", "proven")]);
    run(&r, &["run", "demo"], Some(&path));

    assert!(driver_ran(&r), "the driver never ran");
    let seen: Vec<Value> = r.read(MARKER).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let attest = seen.iter().find(|e| e["kind"] == "attest").expect("no attest entry before the driver");
    let expected = {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(&path).unwrap();
        Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    assert_eq!(attest["data"]["sha256"], expected.as_str());
    assert_eq!(attest["data"]["properties"]["egress.allowlist"]["status"], "proven");
}

/// AC-2 — every required property proven: posture passes and the driver runs.
#[test]
fn proven_posture_passes_and_runs_the_driver() {
    let r = repo("rt-proven", &["egress.allowlist", "audit.unmounted"]);
    let path = attestation(&r, &[("egress.allowlist", "proven"), ("audit.unmounted", "proven")]);
    run(&r, &["run", "demo"], Some(&path));

    assert_eq!(posture_gate(&r).expect("no posture gate")["verdict"], "pass");
    assert!(driver_ran(&r), "posture passed but the driver never ran");
}

/// AC-3 — absent or unproven: blocked, named, exit 3, no driver.
#[test]
fn unproven_property_blocks_before_the_driver() {
    for (name, props) in [
        ("unproven", vec![("egress.allowlist", "proven"), ("audit.unmounted", "unproven")]),
        ("absent", vec![("egress.allowlist", "proven")]),
    ] {
        let r = repo(&format!("rt-{name}"), &["egress.allowlist", "audit.unmounted"]);
        let path = attestation(&r, &props);
        let (code, out) = run(&r, &["run", "demo"], Some(&path));

        assert_eq!(code, 3, "{name}: expected blocked:\n{out}");
        assert!(out.contains("posture:audit.unmounted"), "{name}: property not named:\n{out}");
        assert_eq!(posture_gate(&r).unwrap()["verdict"], "blocked");
        assert!(!driver_ran(&r), "{name}: the driver ran on an unproven posture");
    }
}

/// AC-4 — violated: fail, named, exit 1, no driver.
#[test]
fn violated_property_fails_before_the_driver() {
    let r = repo("rt-violated", &["egress.allowlist"]);
    let path = attestation(&r, &[("egress.allowlist", "violated")]);
    let (code, out) = run(&r, &["run", "demo"], Some(&path));

    assert_eq!(code, 1, "expected fail:\n{out}");
    assert!(out.contains("posture:egress.allowlist"), "property not named:\n{out}");
    assert_eq!(posture_gate(&r).unwrap()["verdict"], "fail");
    assert!(!driver_ran(&r), "the driver ran on a violated posture");
}

/// AC-5 — something required, nothing (readable) supplied: blocked, exit 3.
#[test]
fn missing_attestation_blocks() {
    for (name, attest) in [("unset", None), ("unreadable", Some("/nonexistent/attestation.json"))] {
        let r = repo(&format!("rt-missing-{name}"), &["egress.allowlist"]);
        let (code, out) = run(&r, &["run", "demo"], attest);

        assert_eq!(code, 3, "{name}: expected blocked:\n{out}");
        assert_eq!(posture_gate(&r).unwrap()["verdict"], "blocked");
        assert!(!driver_ran(&r), "{name}: the driver ran with no attestation");
    }
}

/// AC-6 — an attestation that breaks the schema blocks and names the field.
#[test]
fn malformed_attestation_names_the_field() {
    let r = repo("rt-malformed", &["egress.allowlist"]);
    let path = write_attestation(
        &r,
        r#"{"schema":"keel.posture/1","runtime":"fake","properties":{"egress.allowlist":{"status":"probably"}}}"#,
    );
    let (code, out) = run(&r, &["run", "demo"], Some(&path));

    assert_eq!(code, 3, "expected blocked:\n{out}");
    assert!(out.contains("/properties/egress.allowlist/status"), "field not named:\n{out}");
    assert!(!driver_ran(&r));
}

/// AC-7 — nothing required: no attestation needed, no posture gate written.
#[test]
fn no_requirement_leaves_runs_unchanged() {
    let r = repo("rt-none", &[]);
    run(&r, &["run", "demo"], None);

    assert!(driver_ran(&r), "the driver did not run");
    assert!(posture_gate(&r).is_none(), "a posture gate was written with nothing required");
}

/// AC-8 — `--waves` judges posture once, before the first wave, and stops every
/// driver when it does not pass.
#[test]
fn waves_check_posture_before_the_first_wave() {
    let r = repo("rt-waves-blocked", &["egress.allowlist"]);
    let (code, out) = run(&r, &["run", "demo", "--waves"], None);
    assert_eq!(code, 3, "expected blocked:\n{out}");
    assert!(!out.contains("wave 1"), "a wave started on a blocked posture:\n{out}");
    assert!(!driver_ran(&r));

    let r = repo("rt-waves-proven", &["egress.allowlist"]);
    let path = attestation(&r, &[("egress.allowlist", "proven")]);
    let (_, out) = run(&r, &["run", "demo", "--waves"], Some(&path));
    let posture = out.find("posture — demo").expect("posture not judged");
    let wave = out.find("wave 1").expect("no wave ran");
    assert!(posture < wave, "posture judged after the first wave:\n{out}");
    assert_eq!(out.matches("posture — demo").count(), 1, "posture judged more than once:\n{out}");
}
