---
id: TASKS-0020
slug: oracle-sensitivity
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 Executable oracles are re-run at the base commit
- criteria: AC-1
- files: src/config.rs, src/worktree.rs, src/gate/oracle_exec.rs, src/gate/g2.rs, tests/oracle_sensitivity.rs
- budget: 60
- exit: `cargo test --test oracle_sensitivity executable_oracles_are_rerun_in_a_base_worktree_with_test_files_carried_over` exits 0

### T-2 An oracle that passes at the base fails the check
- criteria: AC-2
- files: src/gate/g2.rs, tests/oracle_sensitivity.rs
- depends_on: T-1
- budget: 60
- exit: `cargo test --test oracle_sensitivity an_oracle_passing_without_the_change_fails_sensitivity` exits 0

### T-3 Guard criteria must hold before and after
- criteria: AC-3
- files: src/spec/oracle.rs, src/gate/g2.rs, tests/oracle_sensitivity.rs
- depends_on: T-2
- budget: 60
- exit: `cargo test --test oracle_sensitivity a_guard_oracle_must_pass_at_the_base_and_on_the_change` exits 0

### T-4 A sensitivity run that cannot complete is blocked
- criteria: AC-4
- files: src/worktree.rs, src/gate/oracle_exec.rs, src/gate/g2.rs, tests/oracle_sensitivity.rs
- depends_on: T-3
- budget: 60
- exit: `cargo test --test oracle_sensitivity an_unbuildable_base_blocks_sensitivity_rather_than_passing_it` exits 0

### T-5 Oracles keel cannot isolate are counted, not passed
- criteria: AC-5
- files: src/gate/oracle_exec.rs, src/gate/g2.rs, tests/oracle_sensitivity.rs
- depends_on: T-4
- budget: 60
- exit: `cargo test --test oracle_sensitivity oracles_keel_cannot_isolate_are_listed_as_not_assessed` exits 0

### T-6 A configured mutation tool reports surviving mutants
- criteria: AC-6
- files: src/config.rs, src/gate/g2.rs, schemas/mutationresult.json, tests/fixtures/mutation/**, tests/oracle_sensitivity.rs
- depends_on: T-5
- budget: 60
- exit: both `cargo test --test oracle_sensitivity survived_mutants_in_scope_become_mutation_findings` exits 0 and the schema oracle (`schemas/mutationresult.json` validates `tests/fixtures/mutation/cargo-mutants-sample.json`) passes

### T-7 Mutation findings are advisory until configured to block
- criteria: AC-7
- files: src/config.rs, src/gate/g2.rs, tests/oracle_sensitivity.rs
- depends_on: T-6
- budget: 60
- exit: `cargo test --test oracle_sensitivity mutation_is_advisory_by_default_and_blocks_only_when_configured` exits 0

### T-8 A cargo-mutants adapter ships with keel
- criteria: AC-8
- files: assets/mutation/cargo-mutants, src/driver/builtin.rs, tests/oracle_sensitivity.rs
- depends_on: T-7
- budget: 60
- exit: `cargo test --test oracle_sensitivity the_cargo_mutants_adapter_emits_mutationresult_and_blocks_when_absent` exits 0

### T-9 Existing fixtures are tagged as guards
- criteria: AC-3
- files: tests/support.rs, tests/cover.rs
- depends_on: T-8
- budget: 40
- exit: `cargo test --quiet` exits 0
