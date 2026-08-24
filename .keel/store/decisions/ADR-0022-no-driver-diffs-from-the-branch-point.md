---
id: ADR-0022
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-24
phase: 5
---

# `--no-driver` diffs from where the branch left trunk, not from HEAD

## Context

`Run::create` stamped every run's `base_commit` as HEAD, unconditionally.
Correct for a driver run — the agent's work is uncommitted, so HEAD is where
it started from. Wrong for `--no-driver`, whose own help text says it gates
"the working tree as it stands" — which in practice means work already
committed on a branch, since that is the ordinary shape of a pull request.

Diffing a committed branch against its own HEAD compares a tree with itself:
an empty diff, every diff-based check passing because there was nothing left
to look at. Found gating keel's own change against a real feature branch in a
second project — `blast-radius` and `line-budget` both reported zero against
a 694-line change, both green.

## Decision

`gate_base` computes the base a gate should diff against. With a driver
(`branch_point = false`) nothing changes — HEAD, as before. Without one, it
walks trunk candidates (`origin/HEAD`'s symbolic ref, then `main`, `master`,
`origin/main`, `origin/master`) and takes the merge-base with the first one
that yields a commit different from HEAD. `--base <ref>` overrides either
way, resolved and validated before anything else runs.

The alternative was requiring `--base` explicitly, always, with no inference.
Rejected: the common case — a feature branch cut cleanly from trunk — should
not need an operator to already know the exact commit gate wants named, and
`--waves` already had this exact problem solved for choosing a worktree's
starting point (`base_commit`, unchanged, still HEAD-based, still correct for
its case: waves fold agent patches into a live tree that starts at HEAD).

## Consequences

A repository with no `main`, `master`, or reachable `origin/HEAD` — or one
where the current branch's name happens to match every trunk candidate
checked — falls back to HEAD, silently reintroducing the exact bug this
fixes. Rare in practice (git init defaults to `main`; most clones carry an
`origin/HEAD`), but not impossible, and there is no check that names the
fallback as having happened. A repository with an unusual layout should
default to `--base` explicitly rather than trust the inference.

`reviewable-size`'s file count was fixed in the same pass: it was counting
keel's own untracked run evidence as part of the diff under review, because
it did not apply the same `is_incidental_for` filter its own line count did.
Not a design decision — a plain bug, listed here because it was found and
fixed alongside this one and the CHANGELOG entry covers both.
