---
id: ADR-0001
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 0
---

# Projections carry two hashes, not one

## Context

A projection (`CLAUDE.md`, `AGENTS.md`, …) can diverge from the store in two
completely different ways: the store moved on, or a human edited the generated
file. A single content hash detects "these differ" but cannot say which.

The two cases want opposite responses. Stale wants re-rendering. A hand-edit
wants *not* re-rendering, because re-rendering destroys the human's work.

## Decision

Every projection header carries `store=<hash>` and `body=<hash>`. A body
mismatch is `DRIFT`; a store mismatch with an intact body is `stale`.
`keel store render` refuses to overwrite a `DRIFT` or `foreign` file, and
`keel store reconcile` parks the edit in `store/inbox/` for a human to fold in.

## Consequences

Two hashes cost 24 characters in a header. In exchange, the single-store design
survives contact with four tools that each want to own their own file — which is
the failure mode that kills this pattern in practice.

Reconcile deliberately does not merge automatically: a projection cannot be
reverse-mapped to its sources, and guessing would lose the content the mechanism
exists to protect.
