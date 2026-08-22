---
id: ADR-0004
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 0
---

# Budgets are ceilings that include their own truncation notice

## Context

Every generated artefact declares a line or token budget. The first
implementation rendered 181 lines against a budget of 180 — the "… N more lines"
pointer was added *after* fitting.

A ceiling you can exceed by announcing that you exceeded it is not a ceiling.

## Decision

Budget is enforced as a hard invariant. The truncation note is paid for out of
the budget before content is taken, and every fitting path ends with a final
clamp. Tests assert the invariant across budgets 0..12, where the off-by-ones
live.

## Consequences

Very small budgets produce an artefact that is almost entirely pointer. That is
correct: it says "this does not fit" rather than silently overrunning.

The same rule now applies in four places — the repository map, per-directory
CODEMAPs, projection sections and retrieval answers — and the same off-by-one was
found and fixed in three of them.
