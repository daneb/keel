---
id: ADR-0014
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# Driver adapters share one helper, and a zero budget means no change is expected

## Context

Five adapters were each doing the same five things — read the task, build a
prompt, check the tool exists, diff the tree, emit a result — with five chances
to get the JSON escaping subtly different. One of them did: a backticked hint
inside a double-quoted shell string became command substitution and silently
emptied itself.

Separately, `keel_finish` reported `failed` whenever a tool ran and changed
nothing. That is right for a real task and wrong for the conformance probe,
which asks for nothing — so every conformant driver failed the probe designed to
confirm it was conformant.

## Decision

`.keel/drivers/_common.sh` holds the shared logic; each adapter sources it and
contributes only the invocation of its own tool. If tool-specific logic appears
in the helper, it belongs in the adapter.

A task with `budget_lines: 0` means no change is expected, and changing nothing
is then `ok` rather than `failed`.

## Consequences

An adapter is no longer standalone — it needs `_common.sh` beside it. Accepted:
they live in one directory, and five near-identical scripts drifting apart is
the worse failure.

The zero-budget rule also gave a genuine Python bug: `keel_field` used
`value or ''`, and 0 is falsy, so "no change expected" read as "no budget
stated". Reading a JSON field now checks for `None` explicitly.
