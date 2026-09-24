//! Verify a bundle from its bytes alone (SPEC-0011).
//!
//! The manifest check says the members are the ones exported. The checks here
//! say they are the *right* ones: the chain links, the shipped spec and plan
//! are what was approved, and every verdict and the trajectory are the ones
//! the chain recorded as they happened. Nothing is read but the archive — no
//! repository, no `.keel/`, no config, no network — so an auditor can run it
//! anywhere keel's binary runs.

use crate::approval;
use crate::chain::{self, Outcome};
use crate::gate::{Check, Verdict, roll_up};
use crate::hashing::sha256_hex;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub const REPORT_SCHEMA: &str = "keel.bundleverify/1";

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: String,
    pub archive: String,
    pub run: Option<String>,
    pub spec: Option<String>,
    pub verdict: Verdict,
    pub checks: Vec<Check>,
}

pub fn check(archive: &Path) -> Result<Report> {
    let members: BTreeMap<String, Vec<u8>> = super::read_members(archive)?.into_iter().collect();
    let mut checks = Vec::new();

    let manifest = match super::verify(archive) {
        Ok(v) => {
            checks.push(members_check(&v));
            Some(v.manifest)
        }
        Err(e) => {
            checks.push(Check::blocked("members", format!("{e:#}")));
            None
        }
    };
    let run = manifest.as_ref().map(|m| m.run.clone());
    let spec = manifest.as_ref().map(|m| m.spec.clone());

    let entries = match members.get("chain.jsonl") {
        None => {
            let why = "the bundle carries no chain.jsonl — nothing to check the joins against";
            for id in ["chain", "approvals", "gate-verdicts", "trajectory"] {
                checks.push(Check::blocked(id, why));
            }
            None
        }
        Some(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            let anchor = manifest.as_ref().and_then(|m| m.chain_head.as_deref());
            checks.push(chain_check(&text, anchor));
            Some(text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect::<Vec<_>>())
        }
    };

    if let (Some(entries), Some(run), Some(spec)) = (&entries, &run, &spec) {
        checks.push(approvals_check(entries, spec, &members));
        checks.push(gate_verdicts_check(entries, spec, &members));
        checks.push(trajectory_check(entries, run, &members));
    } else if entries.is_some() {
        for id in ["approvals", "gate-verdicts", "trajectory"] {
            checks.push(Check::blocked(id, "no readable manifest to say which run this is"));
        }
    }

    Ok(Report {
        schema: REPORT_SCHEMA.to_string(),
        archive: archive.display().to_string(),
        run,
        spec,
        verdict: roll_up(&checks),
        checks,
    })
}

fn members_check(v: &super::Verification) -> Check {
    if v.is_intact() {
        return Check::pass("members", format!("{} member(s) match the manifest", v.manifest.members.len()));
    }
    let mut named = Vec::new();
    named.extend(v.tampered.iter().map(|m| format!("tampered {m}")));
    named.extend(v.missing.iter().map(|m| format!("missing {m}")));
    named.extend(v.unlisted.iter().map(|m| format!("unlisted {m}")));
    Check::fail("members", "every member matches the manifest", named.join(", "))
}

fn chain_check(text: &str, anchor: Option<&str>) -> Check {
    match chain::verify_str(text, anchor) {
        Outcome::Intact { entries, head, .. } if anchor.is_some() => {
            Check::pass("chain", format!("{entries} entries link, ending at chain_head {}", &head[..12]))
        }
        Outcome::Intact { .. } => Check::fail("chain", "the manifest names the chain's head", "no chain_head in the manifest"),
        Outcome::Broken { line, seq, reason } => Check::fail(
            "chain",
            "every entry links to the one before",
            format!("entry {} (line {line}): {reason}", seq.map(|s| s.to_string()).unwrap_or_else(|| "?".into())),
        ),
        Outcome::HeadMismatch { expected, actual } => {
            Check::fail("chain", format!("chain ends at chain_head {expected}"), format!("it ends at {actual}"))
        }
    }
}

fn entries_of<'a>(entries: &'a [Value], kind: &'a str) -> impl Iterator<Item = &'a Value> {
    entries.iter().filter(move |e| e["kind"] == kind)
}

/// Each stage's latest recorded approval must be the hash of what shipped.
fn approvals_check(entries: &[Value], spec: &str, members: &BTreeMap<String, Vec<u8>>) -> Check {
    let mut latest: BTreeMap<String, String> = BTreeMap::new();
    for e in entries_of(entries, "approval").filter(|e| e["data"]["spec"] == spec) {
        if let (Some(stage), Some(hash)) = (e["data"]["stage"].as_str(), e["data"]["artefact_hash"].as_str()) {
            latest.insert(stage.to_string(), hash.to_string());
        }
    }
    if latest.is_empty() {
        return Check::blocked("approvals", format!("the chain records no approval for `{spec}`"));
    }
    let mut wrong = Vec::new();
    for (stage, recorded) in &latest {
        let names = approval::artefact_names(stage);
        let shipped: Option<Vec<(&str, &[u8])>> = names
            .iter()
            .map(|n| members.get(&format!("{spec}/{n}")).map(|b| (*n, b.as_slice())))
            .collect();
        match shipped {
            None => wrong.push(format!("{stage} (its artefacts are not in the bundle)")),
            Some(files) if approval::hash_named(files.iter().copied()) != *recorded => {
                wrong.push(format!("{stage} (approved {}, shipped differs)", &recorded[..12.min(recorded.len())]))
            }
            Some(_) => {}
        }
    }
    if wrong.is_empty() {
        Check::pass("approvals", format!("{} stage(s) match what was approved", latest.len()))
    } else {
        Check::fail("approvals", "each stage ships what was approved", wrong.join(", "))
    }
}

/// Every verdict in the bundle must be one the chain recorded as it happened.
fn gate_verdicts_check(entries: &[Value], spec: &str, members: &BTreeMap<String, Vec<u8>>) -> Check {
    let recorded: Vec<&str> = entries_of(entries, "gate").filter_map(|e| e["data"]["sha256"].as_str()).collect();
    let spec_gates = format!("{spec}/gates/");
    let gate_members: Vec<(&String, &Vec<u8>)> = members
        .iter()
        .filter(|(p, _)| {
            p.ends_with(".json")
                && (p.strip_prefix("gates/").is_some_and(|r| !r.contains('/'))
                    || p.strip_prefix(&spec_gates).is_some_and(|r| !r.contains('/')))
        })
        .collect();
    let unrecorded: Vec<&str> = gate_members
        .iter()
        .filter(|(_, bytes)| !recorded.contains(&sha256_hex(bytes).as_str()))
        .map(|(p, _)| p.as_str())
        .collect();
    if unrecorded.is_empty() {
        Check::pass("gate-verdicts", format!("{} verdict(s) match the chain", gate_members.len()))
    } else {
        // Usually a spec gate re-run after this run: its file moved on, and its
        // chain entry sits after run_end, outside what this bundle ships.
        Check::fail(
            "gate-verdicts",
            "every verdict has a gate entry with its SHA-256",
            format!("{} — not recorded up to this run (re-run since?)", unrecorded.join(", ")),
        )
    }
}

/// `run_end` commits to the trajectory as it stood when the run ended. A
/// human decision made afterwards (`keel approve`) is appended to the same
/// stream on purpose, so the check hashes the stream through the `run_end`
/// event and allows only `human` events after it.
fn trajectory_check(entries: &[Value], run: &str, members: &BTreeMap<String, Vec<u8>>) -> Check {
    let Some(end) = entries_of(entries, "run_end").filter(|e| e["data"]["run"] == run).last() else {
        return Check::fail("trajectory", format!("a run_end entry for run {run}"), "none in the chain");
    };
    let Some(shipped) = members.get("trajectory.jsonl") else {
        return Check::fail("trajectory", "trajectory.jsonl in the bundle", "missing");
    };
    let kind_of = |line: &[u8]| {
        serde_json::from_slice::<Value>(line).ok().and_then(|v| v["kind"].as_str().map(String::from))
    };
    let mut through_end = None;
    let mut offset = 0;
    for line in shipped.split_inclusive(|b| *b == b'\n') {
        offset += line.len();
        if kind_of(line).as_deref() == Some("run_end") {
            through_end = Some(offset);
        }
    }
    let Some(cut) = through_end else {
        return Check::fail("trajectory", "trajectory.jsonl ends its run", "trajectory.jsonl differs: no run_end event");
    };
    if end["data"]["trajectory_sha256"] != sha256_hex(&shipped[..cut]).as_str() {
        return Check::fail("trajectory", "trajectory.jsonl matches its run_end entry", "trajectory.jsonl differs");
    }
    let after: Vec<Option<String>> = shipped[cut..]
        .split(|b| *b == b'\n')
        .filter(|l| !l.iter().all(u8::is_ascii_whitespace))
        .map(kind_of)
        .collect();
    if let Some(other) = after.iter().find(|k| k.as_deref() != Some("human")) {
        return Check::fail(
            "trajectory",
            "only human decisions after run_end",
            format!("trajectory.jsonl differs: a `{}` event after run_end", other.as_deref().unwrap_or("unparseable")),
        );
    }
    let appended = if after.is_empty() { String::new() } else { format!(", then {} human decision(s)", after.len()) };
    Check::pass("trajectory", format!("trajectory.jsonl matches run_end{appended}"))
}
