---
id: ADR-0019
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# Pruning runs never removes one a lesson cites

## Context

`.keel/runs/` grows without bound. But a lesson card's `sources:` point at the
runs it was derived from, and that citation is what answers "why does this rule
exist?".

## Decision

`keel runs --prune --keep N` reports; `--apply` removes. Any run cited by a
lesson — including a *demoted* lesson, whose provenance is what stops it being
re-promoted next quarter — is never removed, and the report says which ones were
kept and why.

## Consequences

The audit trail is bounded but not uniformly: a repository with many lessons
keeps more history, which is the correct bias. A dangling provenance id is worse
than the bytes, because it converts an answerable question into an unanswerable
one.
