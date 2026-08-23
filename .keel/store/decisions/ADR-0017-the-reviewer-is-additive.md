---
id: ADR-0017
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# The adversarial reviewer is additive, and can be advisory

## Context

G2.5 shipped as substring heuristics over a diff: a vocabulary of mocking calls
and a count of which files changed. That catches the obvious cases and misses
everything that needs the diff to be read, which is most of what an adversarial
pass is for.

The obvious move — replace the heuristics with an agent — has two problems. An
agent that is not configured would leave no check at all, and an agent's
opinion, uncalibrated, would start blocking merges.

## Decision

A reviewer is a subprocess speaking `keel.reviewrequest/1` →
`keel.reviewresult/1`, receiving the diff, the conventions, the lessons in force
and the criteria. Each finding becomes its own gate check carrying its file and
line.

The heuristics **stay**. An unconfigured reviewer reports that the heuristics are
the whole pass; one that cannot run, or that prints something unparseable,
blocks. `advisory = true` downgrades every finding to a look rather than a
refusal.

## Consequences

Two overlapping mechanisms cover the same ground, which is redundancy rather
than duplication: the heuristics need no model and no network, and the reviewer
sees things they cannot.

Advisory mode is the honest default for a new reviewer. A gate that blocks on an
uncalibrated model's opinion gets routed around within a week, and then nothing
is reviewing anything.
