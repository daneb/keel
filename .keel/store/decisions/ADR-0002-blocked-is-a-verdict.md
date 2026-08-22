---
id: ADR-0002
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 1
---

# `blocked` is a first-class verdict with its own exit code

## Context

A gate check can fail for two unrelated reasons: the thing being checked is
wrong, or the check could not run at all (missing tool, no index, no network).

Folding "could not run" into `fail` tells the Phase 3 failure taxonomy that the
agent broke something it did not break. Folding it into `pass` is the gate
theatre this whole design exists to prevent.

## Decision

Three verdicts — `pass` (0), `fail` (1), `blocked` (3) — distinct on the wire
and in the exit code. A gate with no checks at all is `blocked`, not `pass`.
A driver that cannot start or overruns its timeout is `blocked`, never `failed`.
Classification maps every blocked signal to `PROCESS`, which can never become a
lesson.

## Consequences

Callers must handle three outcomes. CI needs to decide whether blocked stops the
line — usually yes, but for a different reason than a failure.

The alternative, two verdicts, was tried implicitly in the first G3 and produced a
real bug: a blocked G2 was reported as a failed one. That is exactly the
mis-teaching this ADR prevents.
