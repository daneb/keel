---
id: ADR-0010
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 4
---

# The index is an accelerator, never a dependency — and degradation is labelled

## Context

Claude Code's own designers concluded agentic search beat indexed RAG for their
case. grep wins on zero setup and exact patterns. An index wins on structure and
token cost.

The failure mode is not choosing wrong; it is degrading silently from symbols to
text, which is how an agent ends up confidently wrong about a codebase.

## Decision

Every retrieval query falls through to ripgrep when the index is absent, stale,
or the schema has moved. Every answer carries `source: index | ripgrep`, the
textual outline says "not parsed" in its own body, and the CLI prints the reason
on stderr.

A schema mismatch invalidates the whole index rather than reusing rows written
by an older shape.

## Consequences

Answers vary in quality depending on what is available, and the caller can see
which they got. Nothing ever silently returns a worse answer dressed as a better
one.

Cost: two implementations of each query. Accepted — the fallback is short, and
the alternative is a hard dependency on a build step.
