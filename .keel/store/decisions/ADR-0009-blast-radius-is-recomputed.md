---
id: ADR-0009
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 1
---

# G1 recomputes the blast radius rather than trusting the plan

## Context

`keel plan` computes the impact set from the import graph and writes it into
`plan.md`. A week later the graph has moved.

A radius computed against last week's imports is a guess wearing a
computation's clothes.

## Decision

G1's `blast-radius-current` recomputes from the live index and fails when the
recorded set differs, naming what appeared and what vanished. The declared scope
— not the computed radius — is what G2 enforces the diff against: the radius says
what breaks, the scope says what may be touched.

## Consequences

Re-planning is required after the graph moves under a spec, which is friction
proportional to how stale the plan actually is.

Scope globs that match no indexed file are reported rather than failed: for an
additive change that is correct, and keel cannot distinguish it from a typo.
