---
id: TASKS-0018
slug: spec-abandonment
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 An abandoned status makes the stage terminal
- criteria: AC-1
- files: src/pipeline.rs, tests/pipeline.rs
- budget: 50
- exit: `cargo test --test pipeline an_abandoned_status_is_a_terminal_stage` exits 0

### T-2 keel next omits abandoned specs from the open set
- criteria: AC-2
- files: src/cmd/next.rs, tests/pipeline.rs
- budget: 30
- depends_on: T-1
- exit: `cargo test --test pipeline keel_next_json_marks_an_abandoned_spec_complete` exits 0

### T-3 keel spec abandon records the decision with a reason
- criteria: AC-3
- files: src/cmd/spec.rs, src/main.rs, src/approval.rs, src/cmd/approve.rs, tests/pipeline.rs
- budget: 70
- depends_on: T-2
- exit: `cargo test --test pipeline abandon_sets_status_and_records_the_reason` exits 0

### T-4 An empty reason is refused
- criteria: AC-4
- files: src/cmd/spec.rs, tests/pipeline.rs
- budget: 20
- depends_on: T-3
- exit: `cargo test --test pipeline abandon_refuses_an_empty_reason` exits 0

### T-5 Abandoning an already-complete spec is refused without force
- criteria: AC-5
- files: src/cmd/spec.rs, tests/pipeline.rs
- budget: 30
- depends_on: T-4
- exit: `cargo test --test pipeline abandoning_a_complete_spec_needs_force` exits 0

### T-6 Abandonment is reversible to the pre-abandon stage
- criteria: AC-6
- files: tests/pipeline.rs
- budget: 25
- depends_on: T-5
- exit: `cargo test --test pipeline clearing_abandoned_status_restores_the_derived_stage` exits 0

### T-7 The abandoned stage carries a stable wire key
- criteria: AC-7
- files: src/pipeline.rs, tests/pipeline.rs
- budget: 40
- depends_on: T-6
- exit: `cargo test --test pipeline abandoned_stage_has_a_stable_key_and_label` exits 0

### T-8 Declaration order keeps Abandoned terminal
- criteria: AC-8
- files: tests/pipeline.rs
- budget: 30
- depends_on: T-7
- exit: `cargo test --test pipeline declaration_order_keeps_abandoned_terminal` exits 0

