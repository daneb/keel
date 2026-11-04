---
id: ADR-0026
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-11-04
phase: 5
---

# Spent tokens are reported, injected tokens are estimated, and the two are never summed

## Context

`keel metrics` used to report tokens that were all `estimate_tokens()` over what
keel put in front of the model. That is the smallest part of a run's cost: the
agent's own file reads, tool calls, retries and output are invisible to keel.

## Decision

Spent tokens are reported by the driver in the `usage` object of
`keel.driverresult/1` and recorded as `driver_usage` events with
`source: reported`. Injected tokens are estimated by keel. `keel metrics` and
`keel report` print them as two lines, `injected (estimated by keel)` and
`spent (reported by the driver)`, and the two are never summed.

A driver that reports no usage is recorded as `source: unreported`, counted
beside the total, and excluded from it. Missing is never zero. A price-table
cost is labelled estimated and never overrides a reported cost.

## Consequences

Operators get a real spend figure without keel claiming precision it lacks.
Drivers that do not report usage make the spent total a lower bound, and the
unreported count says by how much. Backed by [tests/driver_usage.rs](../../../tests/driver_usage.rs).
