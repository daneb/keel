---
id: SPEC-0016
slug: next-json-approval-standing
schema: keel.spec/1
status: approved
scope:
- src/cmd/next.rs
- tests/json.rs
- CHANGELOG.md
budget:
  criteria: 6
  lines: 150
---

# keel next --json reports approval standing

## Context

`keel next --json` (`keel.next/1`) gives each spec's `stage` and the one
`command` that advances it. At an approval stage it cannot say whether the
approval is still waiting, was rejected, or went stale because the artefact
changed since it was approved. `keel next`'s text output already knows all
three (`pipeline::Position` carries each approval's `Standing`), including
who rejected it and why. The JSON drops that.

A tool driving keel, such as moor, reads the JSON, not the text. After a
rejection it tells the operator to approve or reject again, because the
JSON says nothing happened. The fix belongs in keel, which decides what
comes next; the driving tool should only render it.

The schema is additive-only once shipped, so this adds fields and changes
none.

## Acceptance criteria

### AC-1 Approval stages carry their standing

WHEN a spec's stage is `spec_approval`, `plan_approval` or `merge_approval`
THE SYSTEM SHALL include an `approval` object in that spec's `keel next --json`
entry, with `stage` set to `spec`, `plan` or `merge` and `standing` set to
`absent`, `rejected` or `superseded`.

oracle: cmd `cargo test --test json next_json_reports_approval_standing` exit 0

### AC-2 A rejection names who and why

WHEN the latest decision for that approval is a rejection THE SYSTEM SHALL set
`approval.standing` to `rejected`, `approval.by` to the name recorded on the
rejection, and `approval.note` to its note, or to null when the rejection has
no note.

oracle: cmd `cargo test --test json next_json_reports_who_rejected_and_why` exit 0

### AC-3 A rejected or stale approval names its re-check command

WHEN `approval.standing` is `rejected` or `superseded` THE SYSTEM SHALL set
`approval.recheck` to `keel gate g0 <slug>` for a spec approval,
`keel gate g1 <slug>` for a plan approval, and `keel run <slug>` for a merge
approval.

oracle: cmd `cargo test --test json next_json_names_the_recheck_command` exit 0

### AC-4 Other stages are unchanged

WHEN a spec's stage is not an approval stage THE SYSTEM SHALL omit the
`approval` field from that spec's entry.

oracle: cmd `cargo test --test json next_json_omits_approval_outside_approval_stages` exit 0

### AC-5 Existing fields keep their values

THE SYSTEM SHALL keep `schema` as `keel.next/1` and keep the values of
`blockers`, `slug`, `stage`, `command` and `complete` exactly as they are
before this change, for every stage and standing.

oracle: cmd `cargo test --test json next_json` exit 0

### AC-6 The change is recorded for tools that read the schema

WHEN this change ships THE SYSTEM SHALL describe the `approval` object and its
`stage`, `standing`, `by`, `note` and `recheck` fields in `CHANGELOG.md`.

oracle: cmd `grep -q 'approval.recheck' CHANGELOG.md` exit 0

## Out of scope

- Changing `keel next`'s text output, which already reports all of this.
- A new schema version: this is additive to `keel.next/1`.
- How a driving tool renders a rejection, or how the operator revises a plan.
  Those are the tool's concern (moor's, for moor).
- Reporting a `current` standing: a spec whose approval is current has already
  moved past that approval stage. Likewise a `superseded` merge approval
  already returns the spec to `run`, so at `merge_approval` the standing is
  only ever `absent` or `rejected`.
