---
id: ADR-0007
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 3
---

# A lesson with an oracle stops being a prompt

## Context

A lesson can act two ways: injected into the agent's context, or compiled into a
gate check. The first costs tokens on every run forever. The second costs
nothing and cannot be ignored.

The initial implementation set `rule_kind: both` for lessons with oracles,
which spent context restating a rule that could not be violated without failing
G2.

## Decision

A promoted lesson with an oracle gets `rule_kind: gate-check` and is *not*
injected. Only `prompt-injection` lessons reach the prompt, filtered by scope
and stage. The compiled check carries `from: L-nnnn` so "why does this check
exist?" answers with a lesson id, which answers with run ids.

## Consequences

Enforced rules become invisible to the agent — it simply cannot land a change
that violates them. That is the intent: enforcement beats exhortation, and the
context budget is finite.

A lesson whose rule cannot be expressed as a runnable check stays a prompt, and
G4 reports the enforced-versus-prompt split so the balance stays visible.
