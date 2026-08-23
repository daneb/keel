---
id: ADR-0016
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# A wave runs in one git worktree per task, and patches are applied in sequence

## Context

Waves say which tasks could proceed together. Actually running them together
needs each agent to have its own tree: two drivers editing one checkout produce
a diff nobody wrote and nobody can review.

Phase 5 shipped waves as a report with execution left serial, and said so. This
is the isolation that reason was waiting for.

## Decision

`keel run --waves` gives each task in a wave its own detached worktree at the
same base commit and runs the wave's drivers concurrently. Their patches are
then applied to the main tree **one at a time, in task order**, so a conflict is
a reported conflict rather than whichever process finished last. Worktrees are
removed on drop.

G1 gained `wave-isolation`: two tasks in the same wave may not claim the same
file. Finding that out before two agents have done their work is considerably
cheaper than after.

## Consequences

A wave needs a commit to branch from, so `--waves` refuses on a repository with
no HEAD; serial runs still work there.

The isolation check immediately failed this repository's own plan, where two
tasks both claimed `src/trajectory/mod.rs`. The previously reported "2 waves,
widths 1/2" had been wrong — those tasks could never have run in parallel. A
smaller number that is true beat a larger one that was not.
