---
id: ADR-0006
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 3
---

# A lesson needs two occurrences in *distinct* runs

## Context

The strongest failure mode of a learning harness is a store full of confident
rules derived from one flaky run.

But "two occurrences" is ambiguous: ten failing checks inside a single run look
like ten occurrences and are one mistake.

## Decision

Promotion requires the same `(class, scope)` signature in at least two distinct
run ids. Occurrences within a run are counted and displayed, but do not satisfy
the rule. `--force` overrides it, deliberately and on the record.

Candidates are keyed by `(class, scope)` — not by the check that caught them —
because `blast-radius` and `line-budget` both catch scope creep and keying on
the check produced two identical lessons for one mistake.

## Consequences

A genuinely one-off catastrophe does not become a lesson without `--force`.
Accepted: the cost of a missing rule is lower than the cost of a store nobody
trusts.

`keel learn` therefore counts across every run, not just the named one — a
recurrence is by definition invisible from inside a single run.
