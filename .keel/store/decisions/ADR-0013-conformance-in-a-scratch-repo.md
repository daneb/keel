---
id: ADR-0013
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# Driver conformance runs in a scratch git repository

## Context

"New tools plug in without touching the spine" is unfalsifiable until a new
driver can be checked against the contract. But checking one means invoking a
real coding agent, and doing that against live work is an unpleasant way to
learn what an agent does when it is confused.

## Decision

`keel driver check` probes a driver in a throwaway repository created for the
run and removed after it. The probe task explicitly asks for no changes and
budgets zero lines, so a conformant driver has nothing to do but reply.

The scratch is a real git repository, initialised and committed, because every
adapter reports what it changed by asking git. Probing one somewhere git knows
nothing about would test the adapter under conditions it never meets, and the
`reports-changes` probe would be vacuously true.

## Consequences

Conformance costs one agent invocation, which is cheap because the probe asks
for nothing. It cannot tell you whether an agent can program — only whether the
adapter speaks the protocol.

The suite found a real bug on its first run: a relative adapter path
(`.keel/drivers/x`) was resolved against the working directory rather than the
repository that configured it, which made every driver unreachable from a
scratch. That bug was invisible in normal use, where the two are the same.
