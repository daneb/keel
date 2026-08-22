//! End-to-end tests for Phase 1: spec → G0 → plan → G1, over the real binary.
//!
//! These encode the Phase 1 exit criteria from PLAN.md §5: a spec whose every
//! criterion is falsifiable, a plan whose blast radius is computed rather than
//! guessed, and no way to hand a vague spec to an agent by accident.

use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_keel");

/// Unique per call, not merely per nanosecond: the clock is coarse enough on
/// some platforms that two tests starting together share a directory, which is
/// a flake that costs an afternoon to diagnose.
fn unique_dir(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "keel-p1-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(name: &str) -> Self {
        let dir = unique_dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git/hooks")).unwrap();
        let r = Self { dir };
        r.write("Cargo.toml", "[package]\nname = \"demo\"\n");
        r.write("src/main.rs", "mod api;\nfn main() { api::serve(); }\n");
        r.write("src/api/mod.rs", "use crate::core::Guard;\npub fn serve() {}\n");
        r.write("src/core.rs", "pub struct Guard;\n");
        r.ok(&["init", "--yes"]);
        r
    }

    fn write(&self, rel: &str, content: &str) -> &Self {
        let p = self.dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
        self
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join(rel)).unwrap()
    }

    fn keel(&self, args: &[&str]) -> Output {
        Command::new(BIN).args(args).current_dir(&self.dir).output().expect("running keel")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.keel(args);
        assert!(
            out.status.success(),
            "keel {args:?} failed ({}):\n{}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        let out = self.keel(args);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.code().unwrap_or(-1), text)
    }

    /// A spec that passes G0, parameterised so tests can break one thing.
    fn write_spec(&self, slug: &str, criteria: &str) {
        self.write(
            &format!(".keel/specs/{slug}/spec.md"),
            &format!(
                "---\n\
                 id: SPEC-0001\n\
                 slug: {slug}\n\
                 schema: keel.spec/1\n\
                 status: draft\n\
                 scope:\n  - \"src/api/**\"\n\
                 budget:\n  criteria: 6\n  lines: 120\n\
                 ---\n\n\
                 # Demo\n\n\
                 ## Acceptance criteria\n\n{criteria}"
            ),
        );
    }

    fn good_criteria(&self) -> &'static str {
        "### AC-1 Requests over the limit are rejected\n\n\
         WHEN a client exceeds 100 requests per minute THE SYSTEM SHALL respond with HTTP 429.\n\n\
         oracle: cmd `cargo test --test limit` exit 0\n\n\
         ### AC-2 The limit is configurable\n\n\
         WHERE `rate_limit.rpm` is set THE SYSTEM SHALL use that value.\n\n\
         oracle: test tests/limit.rs::respects_config\n"
    }

    fn write_tasks(&self, slug: &str, body: &str) {
        self.write(
            &format!(".keel/specs/{slug}/tasks.md"),
            &format!("---\nid: TASKS-0001\nslug: {slug}\nschema: keel.tasks/1\n---\n\n# Tasks\n\n{body}"),
        );
    }

    fn good_tasks(&self) -> &'static str {
        "### T-1 Add the limiter\n\
         - criteria: AC-1, AC-2\n\
         - files: src/api/mod.rs\n\
         - budget: 60\n\
         - exit: `cargo test --test limit` exits 0\n"
    }

    /// Fill in the rollback that `keel plan` deliberately leaves empty.
    fn set_rollback(&self, slug: &str, text: &str) {
        let p = format!(".keel/specs/{slug}/plan.md");
        let content = self.read(&p);
        let replaced = content
            .replace("rollback: ''", &format!("rollback: '{text}'"))
            .replace("rollback: \"\"", &format!("rollback: '{text}'"));
        assert_ne!(replaced, content, "no empty rollback field found in:\n{content}");
        self.write(&p, &replaced);
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ---------------------------------------------------------------------------
// G0
// ---------------------------------------------------------------------------

#[test]
fn the_scaffolded_template_fails_its_own_gate() {
    // The template ships with `<trigger>` placeholders. A gate that passes its
    // own scaffold is documentation, not a gate (PLAN.md §6, gate theatre).
    let r = Repo::new("template");
    let (code, out) = r.run(&["spec", "new", "demo", "--scope", "src/api/**"]);
    assert_eq!(code, 1, "the unfilled template passed G0:\n{out}");
    assert!(out.contains("no-placeholders"), "{out}");
}

#[test]
fn a_criterion_without_an_oracle_fails_g0() {
    let r = Repo::new("no-oracle");
    r.write_spec(
        "demo",
        "### AC-1 It works\n\nTHE SYSTEM SHALL respond with HTTP 429.\n",
    );
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("oracle-presence"), "{out}");
    assert!(out.contains("AC-1"), "the failure must name the criterion:\n{out}");
}

#[test]
fn prose_that_is_not_ears_fails_g0() {
    let r = Repo::new("not-ears");
    r.write_spec(
        "demo",
        "### AC-1 It works\n\nThe system should reject requests over the limit.\n\n\
         oracle: cmd `cargo test` exit 0\n",
    );
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("ears-conformance"), "{out}");
}

#[test]
fn weasel_words_fail_g0() {
    let r = Repo::new("vague");
    r.write_spec(
        "demo",
        "### AC-1 It works\n\nTHE SYSTEM SHALL handle errors appropriately.\n\n\
         oracle: cmd `cargo test` exit 0\n",
    );
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("ambiguity"), "{out}");
}

#[test]
fn a_good_spec_passes_g0_and_records_its_evidence() {
    let r = Repo::new("good");
    r.write_spec("demo", r.good_criteria());
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 0, "{out}");

    let evidence = r.read(".keel/specs/demo/gates/G0.json");
    let v: serde_json::Value = serde_json::from_str(&evidence).unwrap();
    assert_eq!(v["schema"], "keel.gate/1");
    assert_eq!(v["gate"], "G0");
    assert_eq!(v["verdict"], "pass");
    assert!(v["run"].as_str().unwrap().len() > 8, "no run id recorded");
    assert!(
        v["checks"].as_array().unwrap().iter().any(|c| c["id"] == "oracle-presence"),
        "checks were not itemised in the evidence"
    );
}

#[test]
fn human_oracles_are_legal_but_counted() {
    let r = Repo::new("human");
    r.write_spec(
        "demo",
        "### AC-1 The message is actionable\n\n\
         THE SYSTEM SHALL name the offending file in the error message.\n\n\
         oracle: human a reviewer confirms the message names the file\n",
    );
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 0, "a named human oracle is legal:\n{out}");
    assert!(out.contains("human-cost"), "{out}");
    assert!(out.contains("AC-1"), "the human cost must name the criterion:\n{out}");
}

#[test]
fn store_drift_fails_g0() {
    // A spec cannot be trusted while the context every agent reads has drifted.
    let r = Repo::new("drift");
    r.write_spec("demo", r.good_criteria());
    assert_eq!(r.run(&["gate", "g0", "demo"]).0, 0);

    let claude = r.read("CLAUDE.md");
    r.write("CLAUDE.md", &format!("{claude}\n- a hand edit\n"));
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("store-drift"), "{out}");
}

// ---------------------------------------------------------------------------
// plan + blast radius
// ---------------------------------------------------------------------------

#[test]
fn the_plan_records_a_blast_radius_computed_from_the_map() {
    let r = Repo::new("blast");
    r.write_spec("demo", r.good_criteria());
    r.ok(&["gate", "g0", "demo"]);
    let out = r.ok(&["plan", "demo"]);
    assert!(out.contains("blast radius"), "{out}");

    let plan = r.read(".keel/specs/demo/plan.md");
    // src/api/mod.rs is imported by src/main.rs, so main must appear at depth 1.
    assert!(plan.contains("src/api/mod.rs"), "{plan}");
    assert!(plan.contains("src/main.rs"), "the importer was not computed:\n{plan}");
}

#[test]
fn re_running_plan_preserves_human_prose() {
    let r = Repo::new("replan");
    r.write_spec("demo", r.good_criteria());
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["plan", "demo"]);

    let p = ".keel/specs/demo/plan.md";
    let edited = r.read(p).replace(
        "_How the change is made. Name the seam you are cutting at._",
        "We cut at the router boundary.",
    );
    r.write(p, &edited);

    r.ok(&["plan", "demo"]);
    assert!(
        r.read(p).contains("We cut at the router boundary."),
        "re-running `keel plan` ate the design prose"
    );
}

#[test]
fn blast_reports_transitive_importers_at_depth() {
    let r = Repo::new("depth");
    let one = r.ok(&["blast", "src/core.rs", "--depth", "1"]);
    assert!(one.contains("src/api/mod.rs"), "{one}");
    assert!(!one.contains("src/main.rs"), "main is two hops away:\n{one}");

    let two = r.ok(&["blast", "src/core.rs", "--depth", "2"]);
    assert!(two.contains("src/main.rs"), "{two}");
}

// ---------------------------------------------------------------------------
// G1
// ---------------------------------------------------------------------------

fn ready_for_g1(name: &str) -> Repo {
    let r = Repo::new(name);
    r.write_spec("demo", r.good_criteria());
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    r.ok(&["plan", "demo"]);
    r.set_rollback("demo", "git revert the merge");
    r.write_tasks("demo", r.good_tasks());
    r
}

#[test]
fn a_complete_plan_passes_g1() {
    let r = ready_for_g1("g1-good");
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&r.read(".keel/specs/demo/gates/G1.json")).unwrap();
    assert_eq!(v["verdict"], "pass");
}

#[test]
fn an_uncovered_criterion_fails_g1() {
    let r = ready_for_g1("g1-uncovered");
    r.write_tasks(
        "demo",
        "### T-1 Add the limiter\n- criteria: AC-1\n- files: src/api/mod.rs\n\
         - budget: 60\n- exit: tests pass\n",
    );
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("criteria-covered"), "{out}");
    assert!(out.contains("AC-2"), "{out}");
}

#[test]
fn a_task_outside_the_declared_scope_fails_g1() {
    let r = ready_for_g1("g1-scope");
    r.write_tasks(
        "demo",
        "### T-1 Add the limiter\n- criteria: AC-1, AC-2\n\
         - files: src/api/mod.rs, src/core.rs\n- budget: 60\n- exit: tests pass\n",
    );
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("task-files-in-scope"), "{out}");
    assert!(out.contains("src/core.rs"), "{out}");
}

#[test]
fn tasks_exceeding_the_spec_budget_fail_g1() {
    let r = ready_for_g1("g1-budget");
    r.write_tasks(
        "demo",
        "### T-1 Add the limiter\n- criteria: AC-1, AC-2\n- files: src/api/mod.rs\n\
         - budget: 130\n- exit: tests pass\n",
    );
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("total-budget"), "{out}");
}

#[test]
fn a_missing_rollback_fails_g1() {
    let r = ready_for_g1("g1-rollback");
    let p = ".keel/specs/demo/plan.md";
    r.write(p, &r.read(p).replace("rollback: 'git revert the merge'", "rollback: ''"));
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rollback-stated"), "{out}");
}

#[test]
fn g1_requires_g0_to_have_passed_first() {
    let r = Repo::new("g1-order");
    r.write_spec("demo", r.good_criteria());
    // Plan needs a spec but we deliberately never run G0.
    r.ok(&["plan", "demo"]);
    r.set_rollback("demo", "git revert");
    r.write_tasks("demo", r.good_tasks());
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("g0-passed"), "{out}");
}

#[test]
fn g1_requires_a_recorded_human_approval() {
    let r = Repo::new("g1-approval");
    r.write_spec("demo", r.good_criteria());
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["plan", "demo"]);
    r.set_rollback("demo", "git revert");
    r.write_tasks("demo", r.good_tasks());
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("spec-approved"), "{out}");
}

#[test]
fn editing_the_spec_after_approval_supersedes_it() {
    // Otherwise "approved" is a stamp applied once and inherited forever.
    let r = ready_for_g1("g1-supersede");
    assert_eq!(r.run(&["gate", "g1", "demo"]).0, 0);

    let p = ".keel/specs/demo/spec.md";
    r.write(p, &r.read(p).replace("HTTP 429", "HTTP 503"));

    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("spec-approved"), "{out}");
    assert!(out.contains("changed after approval"), "{out}");
}

#[test]
fn a_stale_blast_radius_fails_g1() {
    // A radius computed against last week's import graph is a guess wearing a
    // computation's clothes.
    let r = ready_for_g1("g1-stale");
    assert_eq!(r.run(&["gate", "g1", "demo"]).0, 0);

    // A new file starts importing the scope, widening the real radius.
    r.write("src/extra.rs", "use crate::api::serve;\npub fn go() { serve(); }\n");
    r.write("src/main.rs", "mod api;\nmod extra;\nfn main() { api::serve(); }\n");
    r.ok(&["map"]);

    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "the recorded radius went stale unnoticed:\n{out}");
    assert!(out.contains("blast-radius-current"), "{out}");
    assert!(out.contains("src/extra.rs"), "{out}");

    // Re-planning recomputes it and the gate is satisfied again.
    r.ok(&["plan", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    assert_eq!(r.run(&["gate", "g1", "demo"]).0, 0);
}

// ---------------------------------------------------------------------------
// gate contract
// ---------------------------------------------------------------------------

#[test]
fn blocked_is_distinct_from_failed_on_the_wire() {
    // No index => the blast radius cannot be computed. That is "I could not
    // look", not "you broke it", and it must not share an exit code with fail.
    let r = ready_for_g1("blocked");
    std::fs::remove_file(r.dir.join(".keel/store/map/index.sqlite")).unwrap();
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 3, "blocked must not share an exit code with fail:\n{out}");
    assert!(out.contains("BLOCKED"), "{out}");

    let v: serde_json::Value =
        serde_json::from_str(&r.read(".keel/specs/demo/gates/G1.json")).unwrap();
    assert_eq!(v["verdict"], "blocked");
}

#[test]
fn an_external_check_plugin_can_fail_a_gate() {
    let r = Repo::new("plugin");
    r.write_spec("demo", r.good_criteria());

    let script = r.dir.join("check.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\necho '{\"id\":\"x\",\"verdict\":\"fail\",\"detail\":\"the lesson says no\"}'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&script).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&script, p).unwrap();
    }

    let cfg = r.read(".keel/keel.toml");
    r.write(
        ".keel/keel.toml",
        &format!("{cfg}\n[[gate.G0.check]]\nid = \"house-rule\"\ncmd = \"./check.sh\"\nfrom = \"lesson:L-0001\"\n"),
    );

    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 1, "a plugin check could not fail the gate:\n{out}");
    assert!(out.contains("house-rule"), "{out}");
    assert!(out.contains("lesson:L-0001"), "provenance was dropped:\n{out}");
}

#[test]
fn a_plugin_that_cannot_run_blocks_rather_than_fails() {
    let r = Repo::new("plugin-missing");
    r.write_spec("demo", r.good_criteria());
    let cfg = r.read(".keel/keel.toml");
    r.write(
        ".keel/keel.toml",
        &format!("{cfg}\n[[gate.G0.check]]\nid = \"absent\"\ncmd = \"./definitely-not-here\"\n"),
    );
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 3, "a missing tool must block, not fail:\n{out}");
    assert!(out.contains("BLOCKED"), "{out}");
}
