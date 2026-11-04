---
id: ADR-0027
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-11-04
phase: 5
---

# An oracle must fail without the change it claims to verify

## Context

G0 proves each criterion has a parseable oracle and G2 proves it passes. Neither
proves the oracle tests anything: `assert!(true)` passes both. An agent that
writes the code and its tests has every incentive to write such oracles.

## Decision

Sensitivity is native to keel and blocking. G2 re-runs each executable oracle in
a worktree at the base commit with only the run's test files carried over; an
oracle that still passes there fails `G2/oracle-sensitivity`. Criteria declared
`oracle: guard` must pass both at the base and on the change. A sensitivity run
that cannot complete is blocked, not passed, and oracles keel cannot isolate are
listed as not assessed.

Mutation is a plugin and advisory by default, following ADR-0017: survived
mutants in scope become findings, and block only when configured to.

## Consequences

Sensitivity is language-agnostic and cheap, so it can block. Mutation is costlier
and uncalibrated, and a blocking check of that kind gets routed around, so it
advises until it has earned trust. Backed by
[tests/oracle_sensitivity.rs](../../../tests/oracle_sensitivity.rs).
