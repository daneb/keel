---
id: SPEC-0004
slug: g1-usability
schema: keel.spec/1
status: approved
scope:
- src/gate/g1.rs
- src/plan.rs
- tests/phase1.rs
budget:
  criteria: 3
  lines: 150
verified_at: 2026-08-26
---

# G1 usability

## Context

Real-world use of keel against a sibling project exposed three places where G1
demands busywork that does not protect the pipeline:

1. The plan scaffold has a `## Rollback` body section and a `rollback:` front
   matter field. A user fills in the body section; the gate checks only the
   front matter. The scaffold's own structure misleads.
2. Every task must state an exit condition, but when the traced criterion
   already has a runnable oracle the exit is mechanically derivable. Forcing
   the user to re-type it adds no information.
3. Every task must list the files it touches, but config-only changes (linting
   setup, CI flags) touch files that are in scope yet not individually
   enumerable per-task in any useful way.

## Acceptance criteria

### AC-1 Rollback check falls back to the body section

WHEN the plan front matter `rollback` field is empty THE SYSTEM SHALL look for
a `## Rollback` heading in the plan body and pass if that section contains
non-placeholder text.

oracle: cmd `cargo test --test phase1 rollback -- --include-ignored` exit 0

### AC-2 Task exit conditions are pre-filled from oracles

WHEN `keel plan` generates `tasks.md` THE SYSTEM SHALL pre-fill each task's
`exit:` field with the oracle text of its traced criterion when exactly one
runnable oracle exists.

oracle: cmd `cargo test plan::tests::exit_conditions_are_prefilled` exit 0

### AC-3 Tasks may declare files as scope to inherit the spec scope

WHEN a task declares `files: scope` THE SYSTEM SHALL treat it as matching the
spec's declared scope globs for the `task-files-in-scope` check.

oracle: cmd `cargo test --test phase1 files_scope` exit 0

## Out of scope

- Removing the `rollback:` front matter field entirely (it is still the
  preferred location; the body is a fallback).
- Auto-generating file lists from the blast radius.
- Changing the G1 check for task budgets or traceability.
