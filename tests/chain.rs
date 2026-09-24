//! Oracles for SPEC-0009 `evidence-chain`.

mod support;

use serde_json::Value;
use support::{BIN, Repo, noop_driver};

const CHAIN: &str = ".keel/chain.jsonl";

fn entries(r: &Repo) -> Vec<Value> {
    r.read(CHAIN).lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

/// A repo whose chain holds approvals, G0/G1 verdicts and one whole run.
fn with_run(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    r
}

fn keel_with_env(r: &Repo, args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = std::process::Command::new(BIN);
    cmd.args(args).current_dir(&r.dir).env_remove("KEEL_CHAIN_SINK");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("running keel")
}

/// Rewrite the chain's lines, keeping the trailing newline.
fn rewrite(r: &Repo, f: impl FnOnce(&mut Vec<String>)) {
    let mut lines: Vec<String> = r.read(CHAIN).lines().map(String::from).collect();
    f(&mut lines);
    r.write(CHAIN, &format!("{}\n", lines.join("\n")));
}

/// AC-1 — approvals, gate verdicts and run start each append an entry linked
/// to the one before.
#[test]
fn approval_gate_and_run_entries_link() {
    let r = with_run("chain-link");
    let chain = entries(&r);
    let kinds: Vec<&str> = chain.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    for kind in ["approval", "gate", "run_start", "run_end"] {
        assert!(kinds.contains(&kind), "no `{kind}` entry in {kinds:?}");
    }
    assert!(chain[0]["prev_hash"].as_str().unwrap().bytes().all(|b| b == b'0'));
    for pair in chain.windows(2) {
        assert_eq!(pair[1]["prev_hash"], pair[0]["hash"], "entry {} does not link", pair[1]["seq"]);
    }
}

/// AC-1 — every entry keel writes matches the published schema.
#[test]
fn every_entry_validates_against_the_published_schema() {
    let r = with_run("chain-schema");
    let schema: Value =
        serde_json::from_str(include_str!("../schemas/chain.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for e in entries(&r) {
        let errors: Vec<String> = validator.iter_errors(&e).map(|x| x.to_string()).collect();
        assert!(errors.is_empty(), "entry {} fails the schema: {errors:?}", e["seq"]);
    }
}

/// AC-2 — an intact chain verifies and prints its head.
#[test]
fn verify_accepts_an_intact_chain() {
    let r = Repo::ready("chain-intact");
    let out = r.ok(&["chain", "verify"]);
    let head = r.ok(&["chain", "head"]).trim().to_string();
    assert_eq!(head.len(), 64);
    assert!(out.contains(&head), "verify did not print the head:\n{out}");
}

/// AC-3 — each kind of tampering fails and names the first entry that does not
/// link.
#[test]
fn verify_names_each_tamper() {
    type Tamper = fn(&mut Vec<String>);
    let cases: [(&str, Tamper, &str); 4] = [
        ("edit", |l| l[1] = l[1].replacen("\"kind\":\"", "\"kind\":\"x", 1), "entry 2 (line 2)"),
        ("delete", |l| { l.remove(1); }, "entry 3 (line 2)"),
        ("reorder", |l| l.swap(1, 2), "entry 3 (line 2)"),
        ("insert", |l| { let dup = l[1].clone(); l.insert(1, dup); }, "entry 2 (line 3)"),
    ];
    for (name, tamper, names) in cases {
        let r = Repo::ready(&format!("chain-tamper-{name}"));
        assert!(entries(&r).len() >= 3, "{name}: too few entries to tamper with");
        rewrite(&r, tamper);
        let (code, out) = r.run(&["chain", "verify"]);
        assert_ne!(code, 0, "{name}: tampered chain verified:\n{out}");
        assert!(out.contains(names), "{name}: expected `{names}` in:\n{out}");
    }
}

/// AC-4 — a tail rewritten into a validly-linked chain still fails against the
/// head anchored before the rewrite.
#[test]
fn verify_rejects_a_recomputed_tail() {
    let r = Repo::ready("chain-anchor");
    let anchor = r.ok(&["chain", "head"]).trim().to_string();
    r.ok(&["chain", "verify", "--head", &anchor]);

    // Drop the last entry and let keel append a new, correctly-linked one.
    rewrite(&r, |l| { l.pop(); });
    r.ok(&["gate", "g0", "demo"]);
    let actual = r.ok(&["chain", "head"]).trim().to_string();
    r.ok(&["chain", "verify"]);

    let (code, out) = r.run(&["chain", "verify", "--head", &anchor]);
    assert_ne!(code, 0, "rewritten tail passed its anchor:\n{out}");
    assert!(out.contains(&anchor) && out.contains(&actual), "both hashes not printed:\n{out}");
}

/// AC-5 — with a sink configured, payloads go to the sink and the chain file is
/// never written.
#[test]
fn sink_mode_never_writes_the_chain_file() {
    let r = Repo::bare("chain-sink");
    r.write_spec();
    let _ = std::fs::remove_file(r.dir.join(CHAIN));
    let sink = r.dir.join("runtime-sink.jsonl");
    let env = [("KEEL_CHAIN_SINK", sink.to_str().unwrap())];
    assert!(keel_with_env(&r, &["gate", "g0", "demo"], &env).status.success());
    assert!(keel_with_env(&r, &["approve", "demo", "--stage", "spec"], &env).status.success());

    assert!(!r.exists(CHAIN), "keel wrote the chain file while a sink was set");
    let sent: Vec<Value> = std::fs::read_to_string(&sink)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let kinds: Vec<&str> = sent.iter().map(|p| p["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["gate", "approval"]);
    // Stamping is the runtime's job.
    assert!(sent.iter().all(|p| p.get("hash").is_none() && p.get("seq").is_none()));
}

/// AC-6 — entries keel writes itself are marked and reported as self-attested.
#[test]
fn in_process_chain_reports_self_attested() {
    let r = Repo::ready("chain-self");
    assert!(entries(&r).iter().all(|e| e["writer"] == "in-process"));
    let out = r.ok(&["chain", "verify"]);
    assert!(out.contains("self-attested"), "not reported as self-attested:\n{out}");
}

/// AC-7 — a declared secret never reaches the chain, and redaction happens
/// before hashing, so the chain still verifies.
#[test]
fn canary_secret_absent_and_chain_verifies() {
    let canary = "canary-7f3a9e21";
    let r = Repo::bare("chain-redact");
    r.edit_config(|cfg| {
        let mut chain = toml::value::Table::new();
        chain.insert(
            "secrets".into(),
            toml::Value::Array(vec![toml::Value::String("KEEL_TEST_CANARY".into())]),
        );
        cfg.as_table_mut().unwrap().insert("chain".into(), toml::Value::Table(chain));
    });
    // The approver's name is recorded verbatim, so it carries the canary in.
    r.git(&["config", "user.name", &format!("Dev {canary}")]);
    r.write_spec();
    let env = [("KEEL_TEST_CANARY", canary)];
    assert!(keel_with_env(&r, &["gate", "g0", "demo"], &env).status.success());
    assert!(keel_with_env(&r, &["approve", "demo", "--stage", "spec"], &env).status.success());

    let raw = r.read(CHAIN);
    assert!(!raw.contains(canary), "the canary reached the chain:\n{raw}");
    assert!(raw.contains("[REDACTED]"), "nothing was redacted:\n{raw}");
    r.ok(&["chain", "verify"]);
}

/// AC-8 — `run_end` commits to the trajectory by hash and carries none of it.
#[test]
fn run_end_carries_trajectory_hash_only() {
    let r = with_run("chain-traj");
    let run = r.latest_run();
    let trajectory = std::fs::read(r.dir.join(".keel/runs").join(&run).join("trajectory.jsonl")).unwrap();
    let expected = {
        use sha2::{Digest, Sha256};
        Sha256::digest(&trajectory).iter().map(|b| format!("{b:02x}")).collect::<String>()
    };

    let chain = entries(&r);
    let end = chain.iter().rev().find(|e| e["kind"] == "run_end").expect("no run_end entry");
    assert_eq!(end["data"]["trajectory_sha256"], expected.as_str());
    let mut keys: Vec<&str> = end["data"].as_object().unwrap().keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(keys, ["run", "trajectory_sha256", "verdict"]);

    let kinds: Vec<&str> = chain.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    for leaked in ["inject", "driver_call", "driver_result", "oracle", "command"] {
        assert!(!kinds.contains(&leaked), "trajectory event `{leaked}` copied into the chain");
    }
}
