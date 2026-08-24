---
id: ADR-0021
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-24
phase: 5
---

# A fourth approval stage clears G2.5's blocked check, bound to the exact lines flagged

## Context

G2.5's `test-invalidation` heuristic flags added mocks and assertions that
cannot fail. It reports `blocked`, deliberately, not `fail` — a legitimate
test double exists, and failing the gate on every one would get the check
routed around within a week (renamed rather than fixed).

But nothing closed the loop. A human could look at the flagged lines, decide
they were fine, and the check would still report `blocked` on every later run
of the same gate — with no way to distinguish "nobody has looked" from
"someone looked and it's fine." Found running keel against a real second
project for the first time: the block just sat there.

## Decision

`keel approve --stage review <slug>` records the decision, using the same
per-stage approval mechanism spec/plan/merge already use — but bound to the
SHA-256 of `review-flags.txt`, the exact lines G2.5 flagged this run, not to
the spec. A newly added mock changes that file's hash, which supersedes the
acknowledgement automatically, the same guarantee editing a spec supersedes
its sign-off (ADR-0003). `test-invalidation` checks `approval::standing` for
the review stage before reporting blocked; if current, it passes and names
who approved it.

Two alternatives were rejected. Binding the acknowledgement to the *merge*
stage instead would conflate two different decisions — "this specific test
double is fine" and "the whole change is acceptable" — and a merge approval
made before a later mock was added would not naturally re-flag it. Loosening
the heuristic itself (an allowlist of accepted stub patterns, or excluding
matches inside test files) was rejected for the same reason keel does not
reword its own comments to dodge its lint: narrowing what a check can see is
how a check stops meaning anything.

## Consequences

The approval binds to a *generated* file (`review-flags.txt`, written by G2.5
itself), where every other stage binds to something a human authored. That is
a real asymmetry: `store-drift` never has an opinion about it, because
nothing about the review flags is meant to be curated, only accepted or not.
Acceptable here because the flags are a pointer to a diff already under
review, not a store document — but worth remembering if a fifth stage is ever
added, so the pattern does not quietly become "approvals bind to whatever a
check last wrote."
