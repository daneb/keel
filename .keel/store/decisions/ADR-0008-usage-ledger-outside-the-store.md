---
id: ADR-0008
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 3
---

# Lesson usage is recorded outside the store

## Context

Decay needs to know when each lesson was last injected or fired. The obvious
home is the lesson's own front matter.

But the store hash feeds every projection, so writing usage into the store would
mark `CLAUDE.md` stale on every single run.

## Decision

Usage lives in `.keel/lesson-usage.json`, outside `store/`. It is runtime data
*about* the store, not part of it.

## Consequences

The ledger is committed, so decay is a property of the team's usage rather than
one laptop's. It churns on every run, which will conflict on a busy branch — a
small JSON conflict, resolved by taking the later date.

The alternative — ignoring it — would mean a lesson never decays if you switch
machines, which defeats the mechanism.
