---
id: TASKS-0009
slug: test-movement-docs-aware
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 A docs-only change does not block test-movement
- criteria: AC-1
- files: scope
- budget: 40
- exit: `cargo test --lib gate::g25::tests::docs_only_change_does_not_block_test_movement` exits 0

### T-2 Code changed with no test still blocks
- criteria: AC-2
- files: scope
- budget: 40
- exit: `cargo test --lib gate::g25::tests::code_without_tests_is_flagged_for_a_look` exits 0

### T-3 Docs alongside code do not mask the missing test
- criteria: AC-3
- files: scope
- budget: 40
- exit: `cargo test --lib gate::g25::tests::docs_do_not_mask_a_missing_test` exits 0

### T-4 Code changed with a test present still passes
- criteria: AC-4
- files: scope
- budget: 40
- exit: `cargo test --lib gate::g25::tests::code_with_tests_passes` exits 0

### T-5 A source file that adds inline tests counts as test movement
- criteria: AC-5
- files: scope
- budget: 40
- exit: `cargo test --bin keel gate::g25::tests::inline_rust_tests_count_as_test_movement` exits 0

