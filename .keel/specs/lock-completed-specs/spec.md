---
id: SPEC-0005
slug: lock-completed-specs
schema: keel.spec/1
status: approved
scope:
- src/cmd/approve.rs
- src/approval.rs
- src/main.rs
- tests/pipeline.rs
budget:
  criteria: 8
  lines: 120
verified_at: 2026-09-10
---

# Lock approvals on completed specs

## Context

`keel approve <slug> --stage <stage>` takes the slug on faith: nothing checks
whether the named spec has already reached `Stage::Complete` (merge approved
and current). A slip — approving or rejecting an earlier stage against an old,
already-merged slug by mistake — silently appends a fresh approval entry to
that spec's log, leaving a finished feature carrying a decision that does not
belong to it. There is no signal that anything went wrong until someone
notices the completed feature's approval history looks off.

Once a spec is complete, its stages are settled; `keel approve` should refuse
to record a new decision against it unless the caller explicitly says they
mean to.

## Acceptance criteria

### AC-1 Approving a completed spec is refused

WHEN `keel approve <slug> --stage <stage>` is run for a spec whose pipeline
stage (`keel::pipeline::stage`) is `Complete` THE SYSTEM SHALL exit non-zero
without appending to `approvals.jsonl`.

oracle: cmd `cargo test --test pipeline approving_a_completed_spec_is_refused` exit 0

### AC-2 The refusal names why

WHEN `keel approve` refuses because the spec is complete THE SYSTEM SHALL
print a message naming the slug and the completed state, so the mistake is
legible without inspecting `approvals.jsonl`.

oracle: cmd `cargo test --test pipeline the_refusal_names_the_completed_slug` exit 0

### AC-3 --force overrides the lock and records the override

WHEN `keel approve <slug> --stage <stage> --force` is run for a completed
spec THE SYSTEM SHALL record the approval as usual and include a note that
the completed-spec lock was overridden.

oracle: cmd `cargo test --test pipeline force_overrides_the_completed_lock` exit 0

### AC-4 A spec that is not yet complete is unaffected

WHEN `keel approve` is run for a spec whose pipeline stage is not `Complete`
THE SYSTEM SHALL record the approval exactly as it did before this change.

oracle: cmd `cargo test --test pipeline an_incomplete_spec_still_approves_normally` exit 0

## Out of scope

- Locking edits to `spec.md`, `plan.md`, or `tasks.md` themselves — only the
  approval log is guarded.
- A `keel reopen` command or any other way to move a spec backward in the
  pipeline; `--force` only lets one more decision through, it does not change
  the spec's recorded stage.
- Retroactively flagging or cleaning up approval entries recorded before this
  change ships.
