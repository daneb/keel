---
id: ADR-0011
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 0
---

# Curated steering and the generated map are budgeted separately

## Context

PLAN.md suggests ~300 lines total across all steering, including the generated
`structure.md`. But `map.budget_lines` defaults to 400, so on any repository
large enough to fill the map the steering ceiling fires by construction.

A warning that is always on is a warning nobody reads.

## Decision

Two budgets that do not overlap: `store.steering_budget_lines` (150) bounds the
curated docs — product, tech, conventions — and `map.budget_lines` (400) bounds
the generated `structure.md`. `keel status` reports them separately.

## Consequences

This is a deliberate deviation from the figure in PLAN.md, made to preserve its
intent. The total store is larger than the plan's 300; the projection budgets,
which the plan says are what actually bind, are unchanged at 120–200 lines.
