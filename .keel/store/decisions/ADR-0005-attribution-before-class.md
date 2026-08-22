---
id: ADR-0005
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 3
---

# Attribution is decided before class, and UNATTRIBUTABLE is a real outcome

## Context

Peralta et al. inspected 353 rejected agentic PRs: only 35.7% were clear agentic
failures. 31.2% were workflow-driven and 33.1% had no observable rationale.

A harness that distils lessons from all three buckets is training on noise.

## Decision

Every failure episode is attributed `AGENTIC` / `PROCESS` / `HUMAN` /
`UNATTRIBUTABLE` *before* anything asks what kind of mistake it was. Only
`AGENTIC` episodes can become lesson candidates. A gate check with no
classification rule is `UNATTRIBUTABLE`, never guessed into a class.

The unattributable rate appears on every G4 report and in `keel failures`, and
G4 fails above `learn.max_unattributable_rate` (default 0.5).

## Consequences

Some real agentic failures will be filed as unattributable until the taxonomy is
extended. That is the correct direction to be wrong in: the alternative is a
lesson store full of rules derived from flaky infrastructure.

The rate is also a health signal for the classifier itself. If it climbs, the
taxonomy has stopped explaining anything and should be extended, not overridden.
