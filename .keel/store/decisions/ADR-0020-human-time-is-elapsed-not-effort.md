---
id: ADR-0020
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# Human-intervention time is reported as elapsed, never as effort

## Context

PLAN.md §5 asks for human-intervention minutes per task. keel records when a run
started and when a person decided; it cannot see how long they spent thinking.

## Decision

`keel metrics` reports wall-clock elapsed from run start to human decision, and
the count of finished runs where G3 asked for a person and none answered. The
output states in full that this is elapsed time and not effort — a decision made
the next morning counts the night.

## Consequences

The number is large and noisy, and is useful mainly as a trend and for spotting
runs that stalled waiting on someone. Reporting it as "effort" would have made
it precise-looking and wrong, which is worse than noisy and honest.

A real effort number needs the operator to record their own time, which keel has
no way to ask for and no business assuming.
