---
id: ADR-0003
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 1
---

# An approval records the hash of what was approved

## Context

"Approved" as a boolean is a stamp applied once and inherited forever. A spec
approved in March and edited in April is not an approved spec, but nothing in a
boolean says so.

## Decision

Every approval records the SHA-256 of the artefact at sign-off. `Standing` is
computed by re-hashing: `Current`, `Superseded`, `Rejected` or `Absent`.
The plan stage hashes `plan.md` and `tasks.md` together; the merge stage hashes
the spec, plan and tasks, because approving a merge approves the whole agreed
shape of the work.

## Consequences

Editing an approved artefact costs a re-approval, which is friction — and is the
point. G1 fails with `spec changed after approval` rather than honouring a
sign-off nobody gave to this text.

The approvals log is append-only JSONL: a decision log you can rewrite is not a
decision log.
