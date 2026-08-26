---
id: TASKS-0004
slug: g1-usability
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 Rollback check falls back to the body section
- criteria: AC-1
- files: src/gate/g1.rs, tests/phase1.rs
- budget: 55
- exit: `cargo test --test phase1 rollback` passes

### T-2 Task exit conditions are pre-filled from oracles
- criteria: AC-2
- files: src/plan.rs
- budget: 40
- exit: `cargo test plan::tests::exit_conditions_are_prefilled` passes

### T-3 Tasks may declare files as scope to inherit the spec scope
- criteria: AC-3
- files: src/gate/g1.rs, tests/phase1.rs
- budget: 55
- depends_on: T-1
- exit: `cargo test --test phase1 files_scope` passes
