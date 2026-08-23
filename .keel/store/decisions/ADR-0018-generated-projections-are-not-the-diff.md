---
id: ADR-0018
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# Generated projections do not count against a change's blast radius

## Context

Adding a lesson re-renders `CLAUDE.md`, `AGENTS.md` and the rest. Those
re-renders were counted against the author's declared scope and line budget, so
a change that touched one store file failed `blast-radius` for files keel itself
had written.

## Decision

Projection outputs join `.keel/**` and lockfiles as *incidental*: excluded from
the diff G2 judges, and from G3's reviewable-size. One rule, defined once, used
by every check that measures a change.

## Consequences

This does not weaken anything. A *hand-edit* to a projection is caught by
`store-drift`, which is a separate check with a separate failure mode and its own
remedy. Excluding generated output from "what did the author change" is the
whole reason the two checks are distinct.

An earlier version excluded lockfiles from the scope check but charged their
thousands of generated lines against the diff budget — two checks disagreeing
about what the change was.
