//! The evidence chain: `keel.chain/1` (ADR-0001).
//!
//! One append-only, hash-chained log per repository at `.keel/chain.jsonl`.
//! Every entry commits to the hash of the entry before it, so an edit, a
//! deletion, a reorder or an insertion breaks the link at exactly one place and
//! `keel chain verify` can name it.
//!
//! A hash chain only proves order and integrity to someone who cannot rewrite
//! the whole tail. That is why keel owns the *format* but not the *pen*: under a
//! runtime, `KEEL_CHAIN_SINK` names where keel sends payloads, and the runtime's
//! host process — which the sandbox cannot reach — does the appending. Without
//! one, keel appends itself and marks every such entry `in-process`, so a
//! self-written chain never passes for a contained one.

use crate::config::Config;
use crate::hashing::{sha256_hex, SetHasher};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const CHAIN_SCHEMA: &str = "keel.chain/1";
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const SINK_ENV: &str = "KEEL_CHAIN_SINK";
pub const WRITER_IN_PROCESS: &str = "in-process";
const REDACTED: &str = "[REDACTED]";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub schema: String,
    pub seq: u64,
    pub ts: String,
    pub kind: String,
    /// `in-process` when keel wrote it; a runtime names itself.
    pub writer: String,
    pub data: Value,
    pub prev_hash: String,
    pub hash: String,
}

impl Entry {
    fn expected_hash(&self) -> String {
        compute_hash(&self.prev_hash, self.seq, &self.ts, &self.kind, &self.writer, &self.data)
    }
}

/// Every field but `hash` itself. Length-framed through `SetHasher`, so no
/// field can bleed into its neighbour and collide.
fn compute_hash(prev_hash: &str, seq: u64, ts: &str, kind: &str, writer: &str, data: &Value) -> String {
    let mut h = SetHasher::new();
    h.add("prev_hash", prev_hash.as_bytes());
    h.add("seq", seq.to_string().as_bytes());
    h.add("ts", ts.as_bytes());
    h.add("kind", kind.as_bytes());
    h.add("writer", writer.as_bytes());
    // serde_json's map is ordered, so the same data always serialises alike.
    h.add("data", data.to_string().as_bytes());
    h.finish()
}

pub fn path_in(repo: &Path) -> PathBuf {
    repo.join(".keel").join("chain.jsonl")
}

/// Record one piece of evidence from wherever keel just wrote it.
///
/// `near` is any path inside the repository — the file the caller just wrote
/// is the natural one — so no caller needs to thread `Paths` through. Outside
/// an initialised repository there is no chain to write to, and this does
/// nothing.
pub fn record(near: &Path, kind: &str, data: Value) -> Result<()> {
    let Some(repo) = near.ancestors().find(|a| a.join(".keel").is_dir()) else {
        return Ok(());
    };
    let data = redact(data, &secret_values(repo)?);
    if let Some(sink) = std::env::var_os(SINK_ENV) {
        return send(Path::new(&sink), kind, data);
    }
    append(&path_in(repo), kind, WRITER_IN_PROCESS, data).map(|_| ())
}

/// The values of the secrets `[chain] secrets` names, as the environment holds
/// them now. A name that is unset has nothing to leak.
fn secret_values(repo: &Path) -> Result<Vec<String>> {
    let cfg_path = repo.join(".keel").join("keel.toml");
    if !cfg_path.exists() {
        return Ok(vec![]);
    }
    let cfg = Config::load(&cfg_path)?;
    Ok(cfg
        .chain
        .secrets
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .filter(|v| !v.is_empty())
        .collect())
}

/// Masked before hashing, so the hash commits to what an auditor actually sees.
pub fn redact(data: Value, secrets: &[String]) -> Value {
    match data {
        Value::String(s) => Value::String(
            secrets.iter().fold(s, |acc, secret| acc.replace(secret.as_str(), REDACTED)),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(|v| redact(v, secrets)).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, redact(v, secrets))).collect()),
        other => other,
    }
}

/// Hand a payload to the runtime. Sequence, time and hash are the runtime's to
/// stamp: stamping them here would mean trusting the sandbox to have done it.
fn send(sink: &Path, kind: &str, data: Value) -> Result<()> {
    let line = serde_json::json!({ "schema": CHAIN_SCHEMA, "kind": kind, "data": data });
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(sink)
        .with_context(|| format!("opening chain sink {}", sink.display()))?;
    f.write_all(format!("{line}\n").as_bytes())
        .with_context(|| format!("writing to chain sink {}", sink.display()))
}

pub fn append(path: &Path, kind: &str, writer: &str, data: Value) -> Result<Entry> {
    let (seq, prev_hash) = match read(path)?.last() {
        Some(last) => (last.seq + 1, last.hash.clone()),
        None => (1, GENESIS_HASH.to_string()),
    };
    let ts = chrono::Local::now().to_rfc3339();
    let hash = compute_hash(&prev_hash, seq, &ts, kind, writer, &data);
    let entry = Entry {
        schema: CHAIN_SCHEMA.to_string(),
        seq,
        ts,
        kind: kind.to_string(),
        writer: writer.to_string(),
        data,
        prev_hash,
        hash,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {} for append", path.display()))?;
    // One write per entry: a torn line would read as tampering.
    f.write_all(format!("{}\n", serde_json::to_string(&entry)?).as_bytes())
        .with_context(|| format!("appending to {}", path.display()))?;
    Ok(entry)
}

/// Parse every entry, for appending. Verification reads lines itself, because
/// an unparseable line is a finding there, not an error.
fn read(path: &Path) -> Result<Vec<Entry>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| {
            serde_json::from_str(l).with_context(|| format!("{} line {}", path.display(), i + 1))
        })
        .collect()
}

pub fn head(path: &Path) -> Result<String> {
    Ok(read(path)?.last().map(|e| e.hash.clone()).unwrap_or_else(|| GENESIS_HASH.to_string()))
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Intact { entries: usize, head: String, self_attested: bool },
    /// The first entry that does not link. `line` is its position in the file;
    /// `seq` is what it claims.
    Broken { line: usize, seq: Option<u64>, reason: String },
    HeadMismatch { expected: String, actual: String },
}

pub fn verify(path: &Path, anchor: Option<&str>) -> Result<Outcome> {
    let raw = if path.exists() {
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };
    let mut prev_hash = GENESIS_HASH.to_string();
    let mut self_attested = false;
    let mut entries = 0;
    for (i, l) in raw.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let line = i + 1;
        let expected_seq = line as u64;
        let entry: Entry = match serde_json::from_str(l) {
            Ok(e) => e,
            Err(e) => return Ok(Outcome::Broken { line, seq: None, reason: format!("does not parse: {e}") }),
        };
        let broken = |reason: String| Ok(Outcome::Broken { line, seq: Some(entry.seq), reason });
        if entry.seq != expected_seq {
            return broken(format!("sequence {} where {expected_seq} belongs", entry.seq));
        }
        if entry.prev_hash != prev_hash {
            return broken("prev_hash does not match the preceding entry's hash".to_string());
        }
        if entry.expected_hash() != entry.hash {
            return broken("content does not match its hash".to_string());
        }
        self_attested |= entry.writer == WRITER_IN_PROCESS;
        prev_hash = entry.hash;
        entries += 1;
    }
    if let Some(expected) = anchor
        && expected != prev_hash
    {
        return Ok(Outcome::HeadMismatch { expected: expected.to_string(), actual: prev_hash });
    }
    Ok(Outcome::Intact { entries, head: prev_hash, self_attested })
}

/// SHA-256 of a file's bytes, for entries that commit to a file without
/// copying it.
pub fn file_sha256(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(sha256_hex(&bytes))
}
