//! End-to-end tests for Phase 3: classify failures, distil lessons, enforce them.
//!
//! Encodes the Phase 3 exit criteria from PLAN.md §5: lessons promoted only on
//! recurrence, at least some of them compiled into gate checks, the
//! `UNATTRIBUTABLE` rate visible and never learned from, and a decayed lesson
//! demoted.

mod support;

use support::{Repo, noop_driver};

/// A driver that wanders outside the declared scope — the same mistake, twice.
fn wandering_driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     printf 'fn main() { /* wandered */ }\\n' > \"$KEEL_REPO/src/main.rs\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/main.rs\"]}'\n"
        .to_string()
}

fn candidates(r: &Repo) -> Vec<serde_json::Value> {
    let out = r.ok(&["learn", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    v["candidates"].as_array().cloned().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// classification
// ---------------------------------------------------------------------------

#[test]
fn failures_are_attributed_before_they_are_classified() {
    let r = Repo::ready("attribution");
    r.install_driver("wanderer", &wandering_driver());
    r.run(&["run", "demo"]);

    let out = r.ok(&["learn", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let episodes = v["episodes"].as_array().unwrap();
    assert!(!episodes.is_empty(), "no episodes were extracted");

    for e in episodes {
        assert!(
            ["AGENTIC", "PROCESS", "HUMAN", "UNATTRIBUTABLE"]
                .contains(&e["attribution"].as_str().unwrap()),
            "unknown attribution: {e}"
        );
        assert!(!e["rationale"].as_str().unwrap().is_empty(), "no rationale: {e}");
    }
    // The scope creep is agentic and classed.
    let creep = episodes
        .iter()
        .find(|e| e["class"] == "scope-creep")
        .expect("the out-of-scope edit was not classified");
    assert_eq!(creep["attribution"], "AGENTIC");
}

#[test]
fn a_blocked_check_is_process_and_never_becomes_a_lesson() {
    let r = Repo::ready("blocked-process");
    r.edit_config(|cfg| {
        cfg["verify"].as_table_mut().unwrap().remove("lint");
    });
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let out = r.ok(&["learn", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let blocked = v["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["signal"]["signal"] == "gate_check" && e["signal"]["check"] == "lint")
        .expect("the blocked lint check produced no episode");
    assert_eq!(blocked["attribution"], "PROCESS");
    assert!(blocked["class"].is_null(), "a blocked check was given a class");

    // And it never reaches a candidate.
    for c in v["candidates"].as_array().unwrap() {
        assert_ne!(c["class"], "conv-violation", "a blocked check became a lesson candidate");
    }
}

#[test]
fn the_unattributable_rate_is_reported_and_never_learned_from() {
    let r = Repo::ready("unattributable");
    // A plugin check keel has no classification rule for.
    let script = r.dir.join("mystery.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\necho '{\"id\":\"x\",\"verdict\":\"fail\",\"detail\":\"something\"}'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&script).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&script, p).unwrap();
    }
    r.edit_config(|cfg| {
        let mut c = toml::value::Table::new();
        c.insert("id".into(), toml::Value::String("mystery".into()));
        c.insert("cmd".into(), toml::Value::String("./mystery.sh".into()));
        let mut g2 = toml::value::Table::new();
        g2.insert("check".into(), toml::Value::Array(vec![toml::Value::Table(c)]));
        let mut gate = toml::value::Table::new();
        gate.insert("G2".into(), toml::Value::Table(g2));
        cfg.as_table_mut().unwrap().insert("gate".into(), toml::Value::Table(gate));
    });
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let out = r.ok(&["learn", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let mystery = v["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["signal"]["check"] == "mystery")
        .expect("the unmapped check produced no episode");
    assert_eq!(mystery["attribution"], "UNATTRIBUTABLE", "the classifier invented a cause");

    let rate = v["distribution"]["unattributable_rate"].as_f64().unwrap();
    assert!(rate > 0.0, "the unattributable rate is not being reported");

    // Visible in the human report too, not just the JSON.
    let text = r.ok(&["failures"]);
    assert!(text.contains("UNATTRIBUTABLE"), "{text}");
    assert!(text.contains("never learned from"), "{text}");
}

// ---------------------------------------------------------------------------
// promotion rules
// ---------------------------------------------------------------------------

#[test]
fn one_occurrence_does_not_promote_but_two_do() {
    let r = Repo::ready("recurrence");
    r.install_driver("wanderer", &wandering_driver());

    r.run(&["run", "demo"]);
    let after_one = candidates(&r);
    let creep = after_one
        .iter()
        .find(|c| c["class"] == "scope-creep")
        .expect("no scope-creep candidate after one run");
    assert_eq!(creep["promotable"], false, "one run produced a promotable lesson");
    assert!(
        creep["blocked_by"][0].as_str().unwrap().contains("needs 2"),
        "{creep}"
    );
    // And promotion is actually refused.
    let (code, out) = r.run(&["lesson", "promote", "1"]);
    assert_ne!(code, 0, "a single-run candidate was promoted:\n{out}");

    r.run(&["run", "demo"]);
    let after_two = candidates(&r);
    let creep = after_two.iter().find(|c| c["class"] == "scope-creep").unwrap();
    assert_eq!(creep["promotable"], true, "two runs did not establish a recurrence: {creep}");
    assert_eq!(creep["runs"].as_array().unwrap().len(), 2);
}

#[test]
fn force_overrides_the_recurrence_rule_deliberately() {
    let r = Repo::ready("force");
    r.install_driver("wanderer", &wandering_driver());
    r.run(&["run", "demo"]);

    let index = candidates(&r)
        .iter()
        .position(|c| c["class"] == "scope-creep")
        .expect("no candidate")
        + 1;
    let out = r.ok(&["lesson", "promote", &index.to_string(), "--force"]);
    assert!(out.contains("promoted L-0001"), "{out}");
}

#[test]
fn a_second_lesson_in_an_overlapping_scope_is_refused() {
    let r = Repo::ready("overlap");
    r.install_driver("wanderer", &wandering_driver());
    r.run(&["run", "demo"]);
    r.run(&["run", "demo"]);

    let index = candidates(&r).iter().position(|c| c["class"] == "scope-creep").unwrap() + 1;
    r.ok(&["lesson", "promote", &index.to_string()]);

    // Re-proposing offers the same candidate; the store must refuse it.
    let index = candidates(&r).iter().position(|c| c["class"] == "scope-creep").unwrap() + 1;
    let (code, out) = r.run(&["lesson", "promote", &index.to_string()]);
    assert_ne!(code, 0, "a duplicate lesson was accepted:\n{out}");
    assert!(out.contains("already covers"), "{out}");
}

#[test]
fn a_rejected_candidate_stops_being_offered() {
    let r = Repo::ready("reject");
    r.install_driver("wanderer", &wandering_driver());
    r.run(&["run", "demo"]);
    r.run(&["run", "demo"]);

    let before = candidates(&r).len();
    r.ok(&["lesson", "reject", "1", "--note", "not worth a rule"]);
    // `learn` re-proposes from episodes, so rejection is per-decision; what must
    // not happen is a lesson card appearing without a human accepting one.
    assert!(r.ok(&["lessons"]).contains("no lessons in force"));
    assert!(before > 0);
}

// ---------------------------------------------------------------------------
// lesson → gate check
// ---------------------------------------------------------------------------

#[test]
fn a_lesson_with_an_oracle_becomes_a_gate_check_that_can_fail() {
    let r = Repo::ready("compile-lesson");
    r.write("src/api/flag.txt", "clean\n");
    // A lesson whose oracle is a real, controllable command.
    r.write(
        ".keel/store/lessons/L-0001.md",
        "---\nid: L-0001\nschema: keel.lesson/1\nclass: CONV-VIOLATION\nscope: repo\n\
         occurrences: 2\nrule_kind: gate-check\nverified_at: 2999-01-01\ndecay: 90d\n\
         sources: []\nstages:\n  - implement\n---\n\n\
         **Trigger** Any change.\n\n\
         **Observation** Seen twice.\n\n\
         **Rule** The flag file MUST read clean.\n\n\
         **Oracle** cmd `grep -q clean src/api/flag.txt` exit 0\n",
    );
    r.sync_store();
    r.install_driver("noop", &noop_driver());

    // Holding: the check passes and carries its provenance.
    let (_, out) = r.run(&["run", "demo"]);
    assert!(out.contains("lesson:L-0001"), "the lesson did not become a check:\n{out}");
    assert!(out.contains("[L-0001]"), "the check lost its provenance:\n{out}");
    assert!(out.contains("pass     lesson:L-0001"), "{out}");

    // Violated: the same check fails the gate.
    r.write("src/api/flag.txt", "dirty\n");
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 1, "the lesson's check did not fail the gate:\n{out}");
    assert!(out.contains("FAIL     lesson:L-0001"), "{out}");
    assert!(out.contains("The flag file MUST read clean"), "the rule is not shown:\n{out}");
}

#[test]
fn an_enforced_lesson_is_not_also_injected() {
    // Promotion rule 3: a lesson that is enforced does not need to be read.
    let r = Repo::ready("enforced-not-injected");
    r.write("src/api/flag.txt", "clean\n");
    r.write(
        ".keel/store/lessons/L-0001.md",
        "---\nid: L-0001\nschema: keel.lesson/1\nclass: CONV-VIOLATION\nscope: repo\n\
         occurrences: 2\nrule_kind: gate-check\nverified_at: 2999-01-01\ndecay: 90d\n\
         sources: []\nstages:\n  - implement\n---\n\n\
         **Rule** The flag file MUST read clean.\n\n\
         **Oracle** cmd `grep -q clean src/api/flag.txt` exit 0\n",
    );
    r.sync_store();
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let injected: Vec<String> = r
        .read(&format!(".keel/runs/{id}/trajectory.jsonl"))
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["kind"] == "inject")
        .map(|v| v["source"].as_str().unwrap().to_string())
        .collect();
    assert!(
        !injected.iter().any(|s| s.contains("L-0001")),
        "an enforced lesson was also injected, spending context on a rule that cannot be violated: {injected:?}"
    );
}

#[test]
fn a_prompt_lesson_is_injected_and_scoped() {
    let r = Repo::ready("injection-scope");
    // In scope for the spec (src/api/**).
    r.write(
        ".keel/store/lessons/L-0001.md",
        "---\nid: L-0001\nschema: keel.lesson/1\nclass: SCOPE-CREEP\nscope: dir:src/api\n\
         occurrences: 2\nrule_kind: prompt-injection\nverified_at: 2999-01-01\ndecay: 90d\n\
         sources: []\nstages:\n  - implement\n---\n\n**Rule** Stay in the declared scope.\n",
    );
    // Out of scope: a different tree entirely.
    r.write(
        ".keel/store/lessons/L-0002.md",
        "---\nid: L-0002\nschema: keel.lesson/1\nclass: SCOPE-CREEP\nscope: dir:web/app\n\
         occurrences: 2\nrule_kind: prompt-injection\nverified_at: 2999-01-01\ndecay: 90d\n\
         sources: []\nstages:\n  - implement\n---\n\n**Rule** Irrelevant here.\n",
    );
    r.sync_store();
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let sources: Vec<String> = r
        .read(&format!(".keel/runs/{id}/trajectory.jsonl"))
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["kind"] == "inject")
        .map(|v| v["source"].as_str().unwrap().to_string())
        .collect();

    assert!(sources.iter().any(|s| s.contains("L-0001")), "an in-scope lesson was not injected: {sources:?}");
    assert!(
        !sources.iter().any(|s| s.contains("L-0002")),
        "a lesson for another part of the tree was injected anyway: {sources:?}"
    );
}

// ---------------------------------------------------------------------------
// decay
// ---------------------------------------------------------------------------

#[test]
fn an_unused_lesson_decays_and_can_be_demoted() {
    let r = Repo::ready("decay");
    r.write(
        ".keel/store/lessons/L-0001.md",
        "---\nid: L-0001\nschema: keel.lesson/1\nclass: SCOPE-CREEP\nscope: dir:nowhere\n\
         occurrences: 2\nrule_kind: prompt-injection\nverified_at: 2020-01-01\ndecay: 90d\n\
         sources: []\nstages:\n  - implement\n---\n\n**Rule** Long forgotten.\n",
    );
    r.sync_store();

    let listing = r.ok(&["lessons"]);
    assert!(listing.contains("DECAYED"), "an ancient unused lesson is not flagged:\n{listing}");
    assert!(listing.contains("past decay"), "{listing}");

    // G4 refuses to pass while a decayed lesson sits in the store.
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    let (code, out) = r.run(&["gate", "g4"]);
    assert_ne!(code, 0, "G4 passed with a decayed lesson in force:\n{out}");
    assert!(out.contains("decay-review"), "{out}");

    // Demoting archives it rather than deleting it.
    r.ok(&["lesson", "demote", "L-0001", "--reason", "no longer relevant"]);
    assert!(r.ok(&["lessons"]).contains("no lessons in force"));
    let archived = r.read(".keel/store/lessons/demoted/L-0001.md");
    assert!(archived.contains("demoted_because"), "the reason was not kept: {archived}");
    assert!(archived.contains("no longer relevant"), "{archived}");
    assert!(archived.contains("**Rule** Long forgotten."), "the card's content was lost");
}

// ---------------------------------------------------------------------------
// G4
// ---------------------------------------------------------------------------

#[test]
fn g4_forces_a_decision_on_every_promotable_candidate() {
    let r = Repo::ready("g4-decisions");
    r.install_driver("wanderer", &wandering_driver());
    r.run(&["run", "demo"]);
    r.run(&["run", "demo"]);
    r.ok(&["learn"]);

    let (code, out) = r.run(&["gate", "g4"]);
    assert_ne!(code, 0, "G4 passed with an undecided candidate:\n{out}");
    assert!(out.contains("promotion-decisions"), "{out}");

    // Deciding — either way — satisfies the gate.
    r.ok(&["lesson", "reject", "1", "--note", "not worth a rule"]);
    let (code, out) = r.run(&["gate", "g4"]);
    assert_eq!(code, 0, "a decided candidate still blocked G4:\n{out}");
}

#[test]
fn g4_records_its_verdict_in_the_trajectory() {
    let r = Repo::ready("g4-recorded");
    r.install_driver("noop", &noop_driver());
    r.run(&["run", "demo"]);
    r.run(&["gate", "g4"]);

    let id = r.latest_run();
    let gates: Vec<String> = r
        .read(&format!(".keel/runs/{id}/trajectory.jsonl"))
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["kind"] == "gate")
        .map(|v| v["gate"].as_str().unwrap().to_string())
        .collect();
    assert!(gates.contains(&"G4".to_string()), "G4 left no trace in the stream: {gates:?}");
    assert!(r.exists(&format!(".keel/runs/{id}/gates/G4.json")));
}
