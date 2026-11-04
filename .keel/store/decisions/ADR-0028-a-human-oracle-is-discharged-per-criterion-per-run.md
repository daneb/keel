---
id: ADR-0028
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-11-04
phase: 5
---

# A human oracle is discharged per criterion, per run

## Context

G3 accepted one `merge` approval and never asked about any `human` oracle, so a
criterion whose only oracle was human could ship unjudged. The approval also
recorded no account of what the reviewer was shown.

## Decision

A human oracle is discharged per criterion and per run. `keel judge` records a
pass or fail for one criterion, bound to the hash of the run's diff and
`oracles.json`. G3 fails `G3/human-oracles` while any human oracle lacks a
current judgement; a judgement on changed evidence is superseded, and a failed
one fails G3 and records a failure episode. One merge approval no longer stands
in for every judgement. `keel review` writes a brief, and the merge approval
records its hash.

`keel metrics` flags fast approvals and states that it measures elapsed time,
under the same rule as ADR-0020.

## Consequences

Reviewing costs one command per human criterion, which is the cost the oracle
declared when it chose `human`. Backed by
[tests/g3_judgement.rs](../../../tests/g3_judgement.rs).
