//! End-to-end tests for Phase 5: breadth.
//!
//! The sufficiency contract is *"new tools, new checks and new repos plug in
//! without touching the spine"*, so these tests are mostly about things keel
//! does not know about in advance: an arbitrary driver, another repository's
//! store, a task graph it did not author.

mod support;

use support::{Repo, noop_driver};

fn install_named_driver(r: &Repo, id: &str, script: &str) {
    let path = r.dir.join(format!(".keel/drivers/{id}"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&path).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&path, p).unwrap();
    }
    let id = id.to_string();
    r.edit_config(|cfg| {
        let mut d = toml::value::Table::new();
        d.insert("id".into(), toml::Value::String(id.clone()));
        d.insert("cmd".into(), toml::Value::String(format!(".keel/drivers/{id}")));
        d.insert("default".into(), toml::Value::Boolean(true));
        d.insert("timeout_secs".into(), toml::Value::Integer(10));
        cfg.as_table_mut()
            .unwrap()
            .insert("driver".into(), toml::Value::Array(vec![toml::Value::Table(d)]));
    });
}

// ---------------------------------------------------------------------------
// driver conformance
// ---------------------------------------------------------------------------

#[test]
fn a_conformant_driver_passes_every_probe() {
    let r = Repo::ready("conform-good");
    install_named_driver(&r, "null", &noop_driver());
    let (code, out) = r.run(&["driver", "check", "null"]);
    assert_eq!(code, 0, "{out}");
    for probe in ["executable", "reads-task", "emits-result", "status", "no-side-effects"] {
        assert!(out.contains(probe), "probe {probe} missing:\n{out}");
    }
    assert!(!out.contains("FAIL"), "{out}");
}

#[test]
fn conformance_runs_in_a_scratch_repo_not_the_users_tree() {
    // A conformance run invokes a real agent. Doing that against live work is
    // an unpleasant way to learn what a driver does when confused.
    let r = Repo::ready("conform-scratch");
    install_named_driver(
        &r,
        "vandal",
        "#!/bin/sh\ncat > /dev/null\n\
         echo 'destroyed' > \"$KEEL_REPO/src/api/mod.rs\"\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[]}'\n",
    );
    let before = r.read("src/api/mod.rs");
    r.run(&["driver", "check", "vandal"]);
    assert_eq!(r.read("src/api/mod.rs"), before, "conformance edited the real repository");
}

#[test]
fn a_driver_that_ignores_the_no_change_probe_fails_conformance() {
    let r = Repo::ready("conform-sideeffects");
    install_named_driver(
        &r,
        "messy",
        "#!/bin/sh\ncat > /dev/null\n\
         echo 'x' > \"$KEEL_REPO/uninvited.txt\"\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[]}'\n",
    );
    let (code, out) = r.run(&["driver", "check", "messy"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no-side-effects"), "{out}");
    assert!(out.contains("uninvited.txt"), "{out}");
}

#[test]
fn a_driver_that_lies_about_its_output_fails_conformance() {
    let r = Repo::ready("conform-liar");
    install_named_driver(&r, "liar", "#!/bin/sh\ncat > /dev/null\necho 'not json at all'\n");
    let (code, out) = r.run(&["driver", "check", "liar"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("emits-result"), "{out}");
}

#[test]
fn an_uninstalled_driver_blocks_rather_than_fails() {
    // An adapter for a tool you have not installed says nothing about whether
    // the contract holds.
    let r = Repo::ready("conform-absent");
    r.edit_config(|cfg| {
        let mut d = toml::value::Table::new();
        d.insert("id".into(), toml::Value::String("ghost".into()));
        d.insert("cmd".into(), toml::Value::String("definitely-not-installed".into()));
        d.insert("default".into(), toml::Value::Boolean(true));
        d.insert("timeout_secs".into(), toml::Value::Integer(5));
        cfg.as_table_mut()
            .unwrap()
            .insert("driver".into(), toml::Value::Array(vec![toml::Value::Table(d)]));
    });
    let (code, out) = r.run(&["driver", "check", "ghost"]);
    assert_eq!(code, 3, "an unreachable driver was reported as non-conformant:\n{out}");
    assert!(out.contains("BLOCKED"), "{out}");
}

#[test]
fn a_relative_adapter_path_works_from_any_directory() {
    // The bug conformance found: `.keel/drivers/x` is relative to the repo that
    // configured it, not to wherever the child is started.
    let r = Repo::ready("conform-relpath");
    install_named_driver(&r, "rel", &noop_driver());
    assert_eq!(r.run(&["driver", "check", "rel"]).0, 0);
    // And the same adapter still works for a real run, where cwd is the repo.
    r.ok(&["approve", "demo", "--stage", "merge"]);
    let (_, out) = r.run(&["run", "demo"]);
    assert!(out.contains("driver ok"), "{out}");
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

#[test]
fn doctor_is_healthy_on_a_healthy_repo_and_names_the_fix_otherwise() {
    let r = Repo::ready("doctor");
    install_named_driver(&r, "null", &noop_driver());
    let (code, out) = r.run(&["doctor"]);
    assert_eq!(code, 0, "a freshly set-up repo was not healthy:\n{out}");
    assert!(out.contains("healthy"), "{out}");

    // Break the index; doctor must say which command fixes it.
    std::fs::remove_file(r.dir.join(".keel/store/map/index.sqlite")).unwrap();
    let (code, out) = r.run(&["doctor"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("keel map"), "the fix is not named:\n{out}");
}

#[test]
fn doctor_notices_a_hand_edited_projection() {
    let r = Repo::ready("doctor-drift");
    let claude = r.read("CLAUDE.md");
    r.write("CLAUDE.md", &format!("{claude}\n- a hand edit\n"));
    let (code, out) = r.run(&["doctor"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("reconcile"), "the fix is not named:\n{out}");
}

// ---------------------------------------------------------------------------
// metrics
// ---------------------------------------------------------------------------

#[test]
fn metrics_aggregate_across_runs_and_flag_gate_theatre() {
    let r = Repo::ready("metrics");
    install_named_driver(&r, "null", &noop_driver());
    r.ok(&["approve", "demo", "--stage", "merge"]);
    for _ in 0..3 {
        r.run(&["run", "demo"]);
    }

    let out = r.ok(&["metrics", "--threshold", "2", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["runs"].as_u64().unwrap() >= 3, "{v}");
    assert!(v["gate_verdicts"]["G2"].is_object(), "no G2 verdicts aggregated: {v}");
    assert!(v["tokens_total"].as_u64().unwrap() > 0, "no token accounting");

    // PLAN.md §6: a check that never fails in N runs is theatre worth looking at.
    let theatre = v["never_failed"].as_array().unwrap();
    assert!(!theatre.is_empty(), "nothing flagged at threshold 2: {v}");

    let text = r.ok(&["metrics", "--threshold", "2"]);
    assert!(text.contains("gate theatre"), "{text}");
    assert!(text.contains("deleted or tightened"), "{text}");
}

// ---------------------------------------------------------------------------
// dependency waves
// ---------------------------------------------------------------------------

#[test]
fn tasks_are_grouped_into_dependency_waves() {
    let r = Repo::ready("waves");
    r.write(
        ".keel/specs/demo/tasks.md",
        "---\nid: TASKS-0001\nslug: demo\nschema: keel.tasks/1\n---\n\n# Tasks\n\n\
         ### T-1 Foundation\n- criteria: AC-1\n- files: src/api/mod.rs\n- budget: 30\n- exit: tests pass\n\n\
         ### T-2 Depends on the foundation\n- criteria: AC-2\n- files: src/api/mod.rs\n- budget: 30\n\
         - depends_on: T-1\n- exit: tests pass\n\n\
         ### T-3 Independent\n- criteria: AC-1\n- files: src/api/mod.rs\n- budget: 30\n- exit: tests pass\n",
    );
    let out = r.ok(&["tasks", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let waves = v["waves"].as_array().unwrap();
    assert_eq!(waves.len(), 2, "{v}");
    assert_eq!(waves[0].as_array().unwrap().len(), 2, "T-1 and T-3 are independent");
    assert_eq!(waves[1][0]["id"], "T-2");

    let text = r.ok(&["tasks"]);
    assert!(text.contains("wave 1"), "{text}");
    // Honesty about what keel will not do.
    assert!(text.contains("one at a time"), "{text}");
    assert!(text.contains("worktree"), "{text}");
}

#[test]
fn a_dependency_cycle_fails_g1() {
    let r = Repo::ready("waves-cycle");
    r.write(
        ".keel/specs/demo/tasks.md",
        "---\nid: TASKS-0001\nslug: demo\nschema: keel.tasks/1\n---\n\n# Tasks\n\n\
         ### T-1 A\n- criteria: AC-1\n- files: src/api/mod.rs\n- budget: 30\n- exit: x\n- depends_on: T-2\n\n\
         ### T-2 B\n- criteria: AC-2\n- files: src/api/mod.rs\n- budget: 30\n- exit: y\n- depends_on: T-1\n",
    );
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("task-dependencies"), "{out}");
    assert!(out.contains("cycle"), "{out}");
}

#[test]
fn a_dependency_on_a_missing_task_fails_g1() {
    let r = Repo::ready("waves-dangling");
    r.write(
        ".keel/specs/demo/tasks.md",
        "---\nid: TASKS-0001\nslug: demo\nschema: keel.tasks/1\n---\n\n# Tasks\n\n\
         ### T-1 A\n- criteria: AC-1, AC-2\n- files: src/api/mod.rs\n- budget: 30\n- exit: x\n\
         - depends_on: T-9\n",
    );
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("T-1 → T-9"), "{out}");
}

// ---------------------------------------------------------------------------
// cross-repo store
// ---------------------------------------------------------------------------

/// A platform store next to the repository under test.
fn platform_store(r: &Repo, conventions: &str, lesson_rule: &str) -> String {
    let root = r.dir.join("platform-store");
    std::fs::create_dir_all(root.join("steering")).unwrap();
    std::fs::create_dir_all(root.join("lessons")).unwrap();
    std::fs::write(
        root.join("steering/conventions.md"),
        format!("# Platform conventions\n\n- {conventions}\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("lessons/L-9001.md"),
        format!(
            "---\nid: L-9001\nschema: keel.lesson/1\nclass: CONV-VIOLATION\nscope: repo\n\
             occurrences: 9\nrule_kind: prompt-injection\nverified_at: 2026-01-01\ndecay: 365d\n\
             sources: []\nstages:\n  - implement\n---\n\n**Rule** {lesson_rule}\n"
        ),
    )
    .unwrap();
    "platform-store".to_string()
}

fn configure_shared(r: &Repo, path: &str, required: bool) {
    let path = path.to_string();
    r.edit_config(|cfg| {
        let mut t = toml::value::Table::new();
        t.insert("id".into(), toml::Value::String("platform".into()));
        t.insert("path".into(), toml::Value::String(path.clone()));
        t.insert("required".into(), toml::Value::Boolean(required));
        cfg.as_table_mut()
            .unwrap()
            .insert("shared".into(), toml::Value::Array(vec![toml::Value::Table(t)]));
    });
}

#[test]
fn shared_conventions_and_lessons_reach_the_projections() {
    let r = Repo::ready("shared-reach");
    let path = platform_store(&r, "Logs MUST be structured JSON.", "Config MUST come from the platform client.");
    configure_shared(&r, &path, true);
    r.ok(&["store", "render"]);

    let claude = r.read("CLAUDE.md");
    assert!(claude.contains("Logs MUST be structured JSON"), "shared conventions did not project:\n{claude}");
    assert!(claude.contains("shared"), "shared content is not marked as shared:\n{claude}");

    let lessons = r.ok(&["lessons"]);
    assert!(lessons.contains("L-9001"), "{lessons}");
    assert!(lessons.contains("shared:platform"), "provenance was lost:\n{lessons}");
}

#[test]
fn a_local_lesson_shadows_a_shared_one() {
    let r = Repo::ready("shared-shadow");
    let path = platform_store(&r, "X", "the platform rule");
    configure_shared(&r, &path, true);
    r.write(
        ".keel/store/lessons/L-9001.md",
        "---\nid: L-9001\nschema: keel.lesson/1\nclass: CONV-VIOLATION\nscope: repo\n\
         occurrences: 2\nrule_kind: prompt-injection\nverified_at: 2026-01-01\ndecay: 365d\n\
         sources: []\nstages:\n  - implement\n---\n\n**Rule** here we do it differently\n",
    );
    let out = r.ok(&["lessons"]);
    assert!(out.contains("here we do it differently"), "{out}");
    assert!(!out.contains("the platform rule"), "the shared rule was not shadowed:\n{out}");
    assert!(!out.contains("shared:platform"), "the shadowed card kept shared provenance:\n{out}");
}

#[test]
fn a_shared_lesson_is_not_this_repositorys_to_demote() {
    let r = Repo::ready("shared-demote");
    let path = platform_store(&r, "X", "a platform rule");
    configure_shared(&r, &path, true);
    let (code, out) = r.run(&["lesson", "demote", "L-9001"]);
    assert_ne!(code, 0, "a consumer deleted somebody else's rule:\n{out}");
    assert!(out.contains("shared store"), "{out}");
    assert!(out.contains("Shadow it"), "the alternative is not offered:\n{out}");
}

#[test]
fn a_missing_required_shared_store_fails_loudly_everywhere() {
    // The property that matters most: a governance rule must not stop applying
    // quietly, because everyone downstream still believes it is in force.
    let r = Repo::ready("shared-missing");
    configure_shared(&r, "no-such-directory", true);
    r.ok(&["store", "render"]);

    let (code, out) = r.run(&["doctor"]);
    assert_ne!(code, 0, "doctor was happy with a missing required store:\n{out}");
    assert!(out.contains("not in force"), "{out}");

    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 1, "G0 passed with a missing required store:\n{out}");
    assert!(out.contains("shared-stores"), "{out}");

    // And the projection itself says so, for an agent that only reads that.
    r.ok(&["store", "render"]);
    assert!(
        r.read("CLAUDE.md").contains("did not load"),
        "the projection did not admit the rules were absent"
    );
}

#[test]
fn an_optional_shared_store_blocks_rather_than_fails() {
    let r = Repo::ready("shared-optional");
    configure_shared(&r, "no-such-directory", false);
    // Configuring a store changes the store hash, so re-render first — an
    // unrelated store-drift failure would mask what this test is about.
    r.ok(&["store", "render"]);
    let (code, out) = r.run(&["gate", "g0", "demo"]);
    assert_eq!(code, 3, "an optional store was treated as required:\n{out}");
    assert!(out.contains("shared-stores"), "{out}");
}

#[test]
fn changing_a_shared_store_marks_projections_stale() {
    // Otherwise a platform rule changes and reaches nobody.
    let r = Repo::ready("shared-stale");
    let path = platform_store(&r, "original rule", "a rule");
    configure_shared(&r, &path, true);
    r.ok(&["store", "render"]);
    assert_eq!(r.run(&["store", "check"]).0, 0);

    std::fs::write(
        r.dir.join("platform-store/steering/conventions.md"),
        "# Platform conventions\n\n- a NEW platform rule\n",
    )
    .unwrap();

    let (code, out) = r.run(&["store", "check"]);
    assert_eq!(code, 1, "a shared-store change left projections looking current:\n{out}");
    assert!(out.contains("stale"), "{out}");

    r.ok(&["store", "render"]);
    assert!(r.read("CLAUDE.md").contains("a NEW platform rule"));
}

// ---------------------------------------------------------------------------
// wave execution in worktrees
// ---------------------------------------------------------------------------

/// A driver that writes a marker into whichever file its task names.
///
/// It reads the task from stdin, so it proves each worktree got its own task
/// and wrote into its own checkout.
fn per_task_driver() -> String {
    "#!/bin/sh\n\
     task=$(cat)\n\
     id=$(printf '%s' \"$task\" | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"task\"])')\n\
     f=$(printf '%s' \"$task\" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d[\"prompt\"].split(\"- files: \")[1].split(chr(10))[0].strip())')\n\
     mkdir -p \"$(dirname \"$KEEL_REPO/$f\")\"\n\
     printf 'pub fn serve() { /* %s */ }\\n' \"$id\" > \"$KEEL_REPO/$f\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[]}'\n"
        .to_string()
}

fn two_wave_tasks(r: &Repo) {
    r.write(
        ".keel/specs/demo/tasks.md",
        "---\nid: TASKS-0001\nslug: demo\nschema: keel.tasks/1\n---\n\n# Tasks\n\n\
         ### T-1 First\n- criteria: AC-1\n- files: src/api/one.rs\n- budget: 30\n- exit: tests pass\n\n\
         ### T-2 Second\n- criteria: AC-2\n- files: src/api/two.rs\n- budget: 30\n- exit: tests pass\n",
    );
}

#[test]
fn a_wave_runs_each_task_in_its_own_worktree_and_all_patches_land() {
    let r = Repo::ready("waves-run");
    two_wave_tasks(&r);
    install_named_driver(&r, "per-task", &per_task_driver());
    r.ok(&["gate", "g1", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);

    let (_, out) = r.run(&["run", "demo", "--waves"]);
    assert!(out.contains("1 wave(s)") || out.contains("wave 1"), "{out}");

    // Both tasks' work reached the main tree, which is the whole point.
    assert!(r.exists("src/api/one.rs"), "T-1's file did not land:\n{out}");
    assert!(r.exists("src/api/two.rs"), "T-2's file did not land:\n{out}");
    assert!(r.read("src/api/one.rs").contains("T-1"), "{}", r.read("src/api/one.rs"));
    assert!(r.read("src/api/two.rs").contains("T-2"), "{}", r.read("src/api/two.rs"));
}

#[test]
fn worktrees_are_cleaned_up_after_a_wave() {
    let r = Repo::ready("waves-cleanup");
    two_wave_tasks(&r);
    install_named_driver(&r, "per-task", &per_task_driver());
    r.ok(&["gate", "g1", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);
    r.run(&["run", "demo", "--waves"]);

    let listed = r.git(&["worktree", "list"]);
    assert_eq!(listed.lines().count(), 1, "a worktree leaked:\n{listed}");
    let root = r.dir.join(".keel/worktrees");
    let leftover = std::fs::read_dir(&root).map(|d| d.count()).unwrap_or(0);
    assert_eq!(leftover, 0, "worktree directories were left behind");
}

#[test]
fn a_blocked_driver_stops_the_wave_before_any_patch_is_applied() {
    let r = Repo::ready("waves-blocked");
    two_wave_tasks(&r);
    r.edit_config(|cfg| {
        let mut d = toml::value::Table::new();
        d.insert("id".into(), toml::Value::String("ghost".into()));
        d.insert("cmd".into(), toml::Value::String("definitely-not-installed".into()));
        d.insert("default".into(), toml::Value::Boolean(true));
        d.insert("timeout_secs".into(), toml::Value::Integer(5));
        cfg.as_table_mut()
            .unwrap()
            .insert("driver".into(), toml::Value::Array(vec![toml::Value::Table(d)]));
    });
    r.ok(&["gate", "g1", "demo"]);

    let (code, out) = r.run(&["run", "demo", "--waves"]);
    assert_eq!(code, 3, "a blocked driver did not block the run:\n{out}");
    assert!(out.contains("no patches applied"), "{out}");
    assert!(!r.exists("src/api/one.rs"), "a patch landed despite the wave blocking");
}

#[test]
fn a_conflicting_patch_stops_and_says_what_landed() {
    // Two tasks writing the same file is caught by G1 first; forcing it past
    // that must still fail safely rather than silently keeping one side.
    let r = Repo::ready("waves-conflict");
    r.write(
        ".keel/specs/demo/tasks.md",
        "---\nid: TASKS-0001\nslug: demo\nschema: keel.tasks/1\n---\n\n# Tasks\n\n\
         ### T-1 First\n- criteria: AC-1\n- files: src/api/mod.rs\n- budget: 30\n- exit: x\n\n\
         ### T-2 Second\n- criteria: AC-2\n- files: src/api/mod.rs\n- budget: 30\n- exit: y\n",
    );
    // G1 refuses this plan, which is the designed behaviour.
    let (code, out) = r.run(&["gate", "g1", "demo"]);
    assert_eq!(code, 1, "G1 allowed two tasks to claim one file in a wave:\n{out}");
    assert!(out.contains("wave-isolation"), "{out}");
    assert!(out.contains("src/api/mod.rs"), "{out}");
    assert!(out.contains("depends_on"), "the fix is not named:\n{out}");
}

#[test]
fn wave_execution_records_every_task_in_the_trajectory() {
    let r = Repo::ready("waves-trajectory");
    two_wave_tasks(&r);
    install_named_driver(&r, "per-task", &per_task_driver());
    r.ok(&["gate", "g1", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);
    r.run(&["run", "demo", "--waves"]);

    let id = r.latest_run();
    let events: Vec<serde_json::Value> = r
        .read(&format!(".keel/runs/{id}/trajectory.jsonl"))
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let calls: Vec<&serde_json::Value> =
        events.iter().filter(|e| e["kind"] == "driver_call").collect();
    assert_eq!(calls.len(), 2, "not every task was recorded: {calls:#?}");
    let tasks: Vec<&str> = calls.iter().map(|c| c["task"].as_str().unwrap()).collect();
    assert!(tasks.contains(&"T-1") && tasks.contains(&"T-2"), "{tasks:?}");

    // And the patch applications are on the record too.
    let applies = events
        .iter()
        .filter(|e| e["kind"] == "command" && e["cmd"].as_str().unwrap_or("").starts_with("apply"))
        .count();
    assert_eq!(applies, 2, "patch applications were not recorded");
}

#[test]
fn waves_need_a_passing_g1() {
    let r = Repo::bare("waves-nog1");
    r.write_spec();
    r.write_tasks();
    let (code, out) = r.run(&["run", "demo", "--waves"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("G1"), "{out}");
}

// ---------------------------------------------------------------------------
// human intervention time
// ---------------------------------------------------------------------------

#[test]
fn metrics_report_the_elapsed_time_to_a_human_decision() {
    let r = Repo::ready("metrics-human");
    install_named_driver(&r, "null", &noop_driver());
    // The real sequence: a run happens, G3 asks for a person, the person
    // decides, and the decision is recorded against that run.
    r.run(&["run", "demo"]);
    r.ok(&["approve", "demo", "--stage", "merge"]);

    let v: serde_json::Value = serde_json::from_str(&r.ok(&["metrics", "--json"])).unwrap();
    assert!(v["human_decisions"].as_u64().unwrap() > 0, "no human decisions counted: {v}");
    assert!(v["human_minutes_total"].as_f64().is_some(), "no elapsed time reported: {v}");

    let text = r.ok(&["metrics"]);
    assert!(text.contains("human decision"), "{text}");
    // It must not be presented as effort.
    assert!(text.contains("not effort"), "the proxy is not labelled:\n{text}");
}
