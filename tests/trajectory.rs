//! Oracles for SPEC-0001 `trajectory-log`.
//!
//! Each test here is named by an `oracle:` line in
//! `.keel/specs/trajectory-log/spec.md`. Renaming one breaks the spec's oracle,
//! which is the point: the criterion and the check that proves it are bound
//! together by name.

mod support;

use support::{Repo, noop_driver};

/// AC-1 — WHEN an event is appended THE SYSTEM SHALL write exactly one JSON
/// object, terminated by a newline.
#[test]
fn one_json_object_per_line() {
    let r = Repo::ready("traj-lines");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let raw = r.read(&format!(".keel/runs/{id}/trajectory.jsonl"));
    assert!(!raw.is_empty(), "no trajectory was written");
    assert!(raw.ends_with('\n'), "the final event is not newline-terminated");

    for (n, line) in raw.lines().enumerate() {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("line {} is not one JSON object: {e}\n{line}", n + 1));
        assert!(v.is_object(), "line {} is not an object", n + 1);
        assert!(v.get("kind").is_some(), "line {} has no kind", n + 1);
        assert!(v.get("t").is_some(), "line {} has no timestamp", n + 1);
    }
}

/// AC-2 — sequence numbers start at 1 and increase by exactly 1.
#[test]
fn seq_is_gapless_and_increasing() {
    let r = Repo::ready("traj-seq");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let raw = r.read(&format!(".keel/runs/{id}/trajectory.jsonl"));
    let seqs: Vec<u64> = raw
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["seq"].as_u64().unwrap())
        .collect();

    assert!(seqs.len() > 3, "expected a substantial stream, got {}", seqs.len());
    assert_eq!(seqs[0], 1, "the stream does not start at 1");
    for (i, s) in seqs.iter().enumerate() {
        assert_eq!(*s, i as u64 + 1, "gap or duplicate at position {i}");
    }
}

/// AC-3 — IF a trajectory already exists THEN appending preserves every line.
#[test]
fn append_preserves_existing_lines() {
    let r = Repo::ready("traj-append");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let path = format!(".keel/runs/{id}/trajectory.jsonl");
    let before = r.read(&path);
    let before_lines: Vec<&str> = before.lines().collect();
    assert!(!before_lines.is_empty());

    // Recording a human decision reopens this run's existing stream.
    r.ok(&["approve", "demo", "--stage", "merge", "--note", "looks right"]);
    let after = r.read(&path);
    let after_lines: Vec<&str> = after.lines().collect();

    assert!(
        after_lines.len() > before_lines.len(),
        "nothing was appended, so nothing was preserved either"
    );
    assert!(after.starts_with(&before), "reopening truncated or rewrote the stream");
    assert_eq!(
        &after_lines[..before_lines.len()],
        &before_lines[..],
        "existing lines were modified by the append"
    );

    // And the continued sequence is still gapless across the reopen.
    let seqs: Vec<u64> = after_lines
        .iter()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["seq"].as_u64().unwrap())
        .collect();
    for (i, s) in seqs.iter().enumerate() {
        assert_eq!(*s, i as u64 + 1, "the reopened stream renumbered at position {i}");
    }
    assert_eq!(
        after_lines.last().map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["kind"].as_str().unwrap().to_string()),
        Some("human".to_string()),
        "the human decision did not reach the stream"
    );
}

/// AC-4 — WHEN a gate reaches a verdict THE SYSTEM SHALL record a `gate` event
/// carrying the gate id, the verdict, and the result file's path.
#[test]
fn gate_verdict_is_recorded() {
    let r = Repo::ready("traj-gate");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let raw = r.read(&format!(".keel/runs/{id}/trajectory.jsonl"));
    let gates: Vec<serde_json::Value> = raw
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["kind"] == "gate")
        .collect();

    assert!(!gates.is_empty(), "no gate event in the stream");
    let g2 = gates.iter().find(|g| g["gate"] == "G2").expect("no G2 event");
    assert!(g2["verdict"].is_string(), "the G2 event carries no verdict");
    let result = g2["result"].as_str().expect("the G2 event names no result file");
    assert!(
        r.exists(&format!(".keel/runs/{id}/{result}")),
        "the recorded result file {result} does not exist — the verdict is not reproducible"
    );
}

/// AC-5 — WHEN keel injects a store document THE SYSTEM SHALL record the source
/// and the token count.
#[test]
fn injection_records_source_and_tokens() {
    let r = Repo::ready("traj-inject");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo", "--task", "T-1"]);

    let id = r.latest_run();
    let raw = r.read(&format!(".keel/runs/{id}/trajectory.jsonl"));
    let injects: Vec<serde_json::Value> = raw
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["kind"] == "inject")
        .collect();

    assert!(!injects.is_empty(), "nothing was recorded as injected");
    for i in &injects {
        assert!(i["source"].is_string(), "an injection names no source: {i}");
        assert!(i["tokens"].as_u64().unwrap() > 0, "an injection reports zero tokens: {i}");
    }
    // The house rules and the spec are what the agent is being held to; if they
    // are not in the stream, the run cannot be reconstructed.
    let sources: Vec<&str> = injects.iter().map(|i| i["source"].as_str().unwrap()).collect();
    assert!(
        sources.iter().any(|s| s.contains("conventions.md")),
        "the house rules were not recorded as injected: {sources:?}"
    );
    assert!(
        sources.iter().any(|s| s.contains("spec.md")),
        "the spec was not recorded as injected: {sources:?}"
    );
}

/// AC-7 — IF a line is not valid JSON THEN THE SYSTEM SHALL exit non-zero and
/// name the file and line number.
#[test]
fn corrupt_line_names_file_and_line() {
    let r = Repo::ready("traj-corrupt");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let path = format!(".keel/runs/{id}/trajectory.jsonl");
    let mut raw = r.read(&path);
    raw.push_str("{ this is not json }\n");
    let corrupt_line = raw.lines().count();
    r.write(&path, &raw);

    let (code, out) = r.run(&["replay", &id]);
    assert_ne!(code, 0, "a corrupt trajectory was read as if it were fine:\n{out}");
    assert!(out.contains("trajectory.jsonl"), "the file is not named:\n{out}");
    assert!(
        out.contains(&format!(":{corrupt_line}")),
        "line {corrupt_line} is not named:\n{out}"
    );
}

/// AC-6 — `keel replay` prints every event in seq order and exits 0.
#[test]
fn replay_prints_events_in_order() {
    let r = Repo::ready("traj-replay");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let (code, out) = r.run(&["replay", &id, "--json"]);
    assert_eq!(code, 0, "{out}");

    let seqs: Vec<u64> = out
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["seq"].as_u64().unwrap())
        .collect();
    assert!(!seqs.is_empty(), "replay printed nothing");
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "replay is out of order: {seqs:?}");
}
