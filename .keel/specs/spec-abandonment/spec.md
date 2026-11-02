---
id: SPEC-0018
slug: spec-abandonment
schema: keel.spec/1
status: approved
scope:
- src/pipeline.rs
- src/spec/mod.rs
- src/cmd/spec.rs
- src/cmd/next.rs
- src/approval.rs
- src/cmd/approve.rs
- src/main.rs
- tests/pipeline.rs
budget:
  criteria: 8
  lines: 300
verified_at: 2026-11-02
---

# Abandon a spec as a terminal pipeline state

## Context

`pipeline::Stage` (src/pipeline.rs) models exactly one terminal state:
`Complete`. `derive_stage` walks gates and approvals and never consults
`SpecFront.status` (src/spec/mod.rs), which already exists, defaults to
`draft`, and is otherwise ignored for lifecycle. So every spec that is not
merged is "open": `keel next` lists it, and downstream drivers (moor's
`guide::pick_active`) point the operator at it forever.

Rejection does not retire a spec. `keel approve --reject` records a decision
on a gate, which sends the spec back for revision (`Standing::Rejected`). There
is no way to say "we are not doing this spec" — the concept of *abandonment* is
absent, distinct from rejection. The visible symptom: a sibling project
accumulated four G0-passed, never-approved specs with no guided way to clear
them, and had to move folders by hand.

This change adds `abandoned` as a terminal state, driven off the existing
`status` field so no new store artefact is needed, gated behind an explicit
command and a reason, and recorded in the audited approval log so it is
attributable and reversible. It is the keel half of the cross-repo change whose
host-side half (`moor drop`) lives in the moor repo's `spec-abandonment` spec.

## Acceptance criteria

### AC-1 An abandoned status makes the stage terminal

WHEN `pipeline::position` evaluates a spec whose `status` is `abandoned` THE
SYSTEM SHALL return the terminal `Abandoned` stage regardless of gate or
approval state.

oracle: cmd `cargo test --test pipeline an_abandoned_status_is_a_terminal_stage` exit 0

### AC-2 keel next omits abandoned specs from the open set

WHEN `keel next --json` runs THE SYSTEM SHALL emit an abandoned spec with
`complete` true, so a consumer filtering on `!complete` never treats it as
open.

oracle: cmd `cargo test --test pipeline keel_next_json_marks_an_abandoned_spec_complete` exit 0

### AC-3 keel spec abandon records the decision with a reason

WHEN `keel spec abandon <slug> --reason "<why>"` is run with a non-empty
reason THE SYSTEM SHALL set the spec status to `abandoned` and append an entry
to the spec's approval log carrying the reason and the operator identity.

oracle: cmd `cargo test --test pipeline abandon_sets_status_and_records_the_reason` exit 0

### AC-4 An empty reason is refused

IF `keel spec abandon <slug>` is run with a blank or whitespace-only reason
THEN THE SYSTEM SHALL exit non-zero and leave the spec status unchanged.

oracle: cmd `cargo test --test pipeline abandon_refuses_an_empty_reason` exit 0

### AC-5 Abandoning an already-complete spec is refused without force

IF `keel spec abandon <slug>` names a spec whose stage is `Complete` THEN THE
SYSTEM SHALL exit non-zero unless `--force` is passed, matching the
completed-spec lock on `keel approve`.

oracle: cmd `cargo test --test pipeline abandoning_a_complete_spec_needs_force` exit 0

### AC-6 Abandonment is reversible to the pre-abandon stage

WHEN a spec's `status` is returned from `abandoned` to `draft` THE SYSTEM
SHALL evaluate its stage from gates and approvals exactly as it would have
before abandonment, so no gate or approval evidence is lost.

oracle: cmd `cargo test --test pipeline clearing_abandoned_status_restores_the_derived_stage` exit 0

### AC-7 The abandoned stage carries a stable wire key

THE SYSTEM SHALL give the `Abandoned` stage a `key()` of `abandoned` and a
`label()` distinct from every other stage, so consumers can match it without
parsing prose.

oracle: cmd `cargo test --test pipeline abandoned_stage_has_a_stable_key_and_label` exit 0

### AC-8 Declaration order keeps Abandoned terminal

THE SYSTEM SHALL order the `Abandoned` variant so `Stage`'s derived `Ord`
ranks it at or after `Complete`, so "furthest along" comparisons never rank an
abandoned spec below an open one.

oracle: cmd `cargo test --test pipeline declaration_order_keeps_abandoned_terminal` exit 0

## Out of scope

- Deleting or garbage-collecting abandoned spec folders. Abandonment is a
  status change; the folder and its evidence stay on disk.
- A bulk-abandon command. This adds the single per-spec verb.
- Any change to what `--reject` means. Reject still sends a spec back for
  revision; abandon is the separate terminal path.
- The host-side `moor drop` verb and studio display, which live in the moor
  repo's `spec-abandonment` spec and depend on AC-2's wire contract.
