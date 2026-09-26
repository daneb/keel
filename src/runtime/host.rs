//! The host's side of the runtime contract (ADR-0001, ADR-0002).
//!
//! A runtime's host process, which the sandbox can't reach, is the only writer
//! of the evidence chain. Moor does this in its own code; these are the same
//! jobs as keel subcommands, so any runtime whose host can run keel (GitHub
//! Actions, first) gets them without reimplementing the frozen formats.

use crate::chain;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn offset_path(chain: &Path) -> PathBuf {
    let mut p = chain.as_os_str().to_owned();
    p.push(".sink-offset");
    PathBuf::from(p)
}

/// Fold new sink lines into the host chain under `writer`. Each payload keeps
/// its own `kind`, marked `source: "sandbox"`: it is what the sandbox said,
/// recorded faithfully. A line that is not a payload is kept as
/// `sink_malformed` — dropping it would let the sandbox hide a line by
/// breaking it. Progress is kept beside the chain, and reset when the sink got
/// shorter (it was recreated).
pub fn fold(sink: &Path, chain_path: &Path, writer: &str) -> Result<usize> {
    let text = match std::fs::read_to_string(sink) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e).with_context(|| format!("reading {}", sink.display())),
    };
    let lines: Vec<&str> = text.lines().collect();
    let offsets = offset_path(chain_path);
    let prev: usize = std::fs::read_to_string(&offsets).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let start = if prev > lines.len() { 0 } else { prev };

    let mut folded = 0;
    for (i, raw) in lines.iter().enumerate().skip(start) {
        if raw.trim().is_empty() {
            continue;
        }
        let payload = serde_json::from_str::<Value>(raw).ok().and_then(|v| {
            let kind = v.get("kind")?.as_str()?.to_string();
            let data = v.get("data")?.as_object()?.clone();
            Some((kind, data))
        });
        match payload {
            Some((kind, mut data)) => {
                data.insert("source".into(), json!("sandbox"));
                chain::append(chain_path, &kind, writer, Value::Object(data))?;
            }
            None => {
                chain::append(
                    chain_path,
                    "sink_malformed",
                    writer,
                    json!({ "line_no": i + 1, "line": raw, "source": "sandbox" }),
                )?;
            }
        }
        folded += 1;
    }
    std::fs::write(&offsets, lines.len().to_string())?;
    Ok(folded)
}

fn verdict(status: &str, evidence: String) -> Value {
    json!({ "status": status, "evidence": evidence })
}

fn unproven(what: &str) -> Value {
    verdict("unproven", format!("docker inspect does not report {what}"))
}

fn flag(v: &Value, what: &str, good: bool) -> Value {
    match v.as_bool() {
        Some(b) if b == good => verdict("proven", format!("{what} is {b}")),
        Some(b) => verdict("violated", format!("{what} is {b}")),
        None => unproven(what),
    }
}

fn listed(host: &Value, field: &str, wanted: fn(&str) -> bool, what: &str) -> Value {
    match host[field].as_array() {
        None => unproven(&format!("HostConfig.{field}")),
        Some(items) => {
            let items: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            let status = if items.iter().any(|i| wanted(i)) { "proven" } else { "violated" };
            verdict(status, format!("HostConfig.{field} is [{}] ({what})", items.join(", ")))
        }
    }
}

/// Posture properties from one `docker inspect` object — the same rules as
/// Moor's, plus two a CI runner needs. Anything the output does not establish
/// is `unproven`, never assumed.
pub fn derive(inspect: &Value) -> Value {
    let host = &inspect["HostConfig"];
    let mut p = serde_json::Map::new();
    p.insert("fs.read_only_root".into(), flag(&host["ReadonlyRootfs"], "HostConfig.ReadonlyRootfs", true));
    p.insert("privileged.off".into(), flag(&host["Privileged"], "HostConfig.Privileged", false));
    p.insert(
        "caps.dropped_all".into(),
        listed(host, "CapDrop", |c| c.eq_ignore_ascii_case("ALL"), "needs ALL"),
    );
    p.insert(
        "privileges.no_new".into(),
        listed(
            host,
            "SecurityOpt",
            |o| o == "no-new-privileges" || o == "no-new-privileges:true",
            "needs no-new-privileges",
        ),
    );
    p.insert(
        "user.non_root".into(),
        match inspect["Config"]["User"].as_str() {
            None => unproven("Config.User"),
            Some(u) => {
                let name = u.split(':').next().unwrap_or("");
                let root = name.is_empty() || name == "root" || name == "0";
                verdict(if root { "violated" } else { "proven" }, format!("Config.User is {u:?}"))
            }
        },
    );
    // In CI, mounting the checkout is the point: say what is mounted.
    p.insert(
        "mounts.no_host_bind".into(),
        match inspect["Mounts"].as_array() {
            None => unproven("Mounts"),
            Some(mounts) => {
                let binds: Vec<&str> = mounts
                    .iter()
                    .filter(|m| m["Type"] == "bind")
                    .filter_map(|m| m["Destination"].as_str())
                    .collect();
                if binds.is_empty() {
                    verdict("proven", format!("{} mount(s), none of type bind", mounts.len()))
                } else {
                    verdict("violated", format!("bind mount(s) at {}", binds.join(", ")))
                }
            }
        },
    );
    // `--network none` attaches a network named "none", which is not marked
    // internal but carries no traffic.
    p.insert(
        "network.internal_only".into(),
        match inspect["NetworkSettings"]["Networks"].as_object() {
            None => unproven("NetworkSettings.Networks"),
            Some(nets) if nets.is_empty() => unproven("any attached network"),
            Some(nets) if nets.keys().all(|n| n == "none") => verdict("proven", "network none".to_string()),
            Some(nets) => verdict(
                "violated",
                format!("attached to {}", nets.keys().cloned().collect::<Vec<_>>().join(", ")),
            ),
        },
    );
    // Only gVisor keeps the workload's syscalls off the host kernel.
    p.insert(
        "kernel.isolated".into(),
        match host["Runtime"].as_str() {
            Some("runsc") => verdict("proven", "HostConfig.Runtime is runsc (gVisor)".to_string()),
            Some(other) => verdict("unproven", format!("HostConfig.Runtime is {other}, not runsc")),
            None => unproven("HostConfig.Runtime"),
        },
    );
    Value::Object(p)
}

/// The run's identity as the runner reports it. A claim, not proof, until it
/// is signed; `claimed_by` says whose claim it is.
pub fn runtime_identity(env: impl Fn(&str) -> Option<String>) -> Option<Value> {
    let fields = [
        ("repository", "GITHUB_REPOSITORY"),
        ("workflow", "GITHUB_WORKFLOW"),
        ("run_id", "GITHUB_RUN_ID"),
        ("sha", "GITHUB_SHA"),
    ];
    let mut id = serde_json::Map::new();
    for (key, var) in fields {
        if let Some(v) = env(var).filter(|v| !v.is_empty()) {
            id.insert(key.into(), json!(v));
        }
    }
    if id.is_empty() {
        return None;
    }
    id.insert("claimed_by".into(), json!("runner"));
    Some(Value::Object(id))
}

/// Attest a container from its `docker inspect` output: write the
/// `keel.posture/1` document keel inside will read, and put its hash on the
/// host chain.
pub fn attest(inspect_path: &Path, chain_path: &Path, out: &Path, runtime: &str) -> Result<()> {
    let raw = std::fs::read_to_string(inspect_path)
        .with_context(|| format!("reading {}", inspect_path.display()))?;
    let parsed: Value = serde_json::from_str(&raw).context("parsing docker inspect output")?;
    let inspect = parsed.as_array().and_then(|a| a.first()).cloned().unwrap_or(parsed);

    let mut doc = json!({
        "schema": super::POSTURE_SCHEMA,
        "runtime": runtime,
        "attested_at": chrono::Utc::now().to_rfc3339(),
        "properties": derive(&inspect),
    });
    if let Some(digest) = inspect["Image"].as_str() {
        doc["image_digest"] = json!(digest);
    }
    if let Some(id) = runtime_identity(|k| std::env::var(k).ok()) {
        doc["runtime_identity"] = id;
    }
    let text = format!("{}\n", serde_json::to_string_pretty(&doc)?);
    // Checked against the published schema before anyone relies on it.
    super::parse(&text).map_err(|e| anyhow::anyhow!("the attestation does not match keel.posture/1: {e}"))?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, &text).with_context(|| format!("writing {}", out.display()))?;

    let mut entry = json!({
        "sha256": crate::hashing::sha256_hex(text.as_bytes()),
        "runtime": runtime,
        "properties": doc["properties"],
    });
    if let Some(id) = doc.get("runtime_identity") {
        entry["runtime_identity"] = id.clone();
    }
    chain::append(chain_path, "attest", runtime, entry)?;
    Ok(())
}
