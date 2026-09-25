---
id: SPEC-0013
slug: papercuts
schema: keel.spec/1
status: approved
scope:
- src/gate/g25.rs
- src/cover/mod.rs
- assets/drivers/**
- tests/papercuts.rs
budget:
  criteria: 6
  lines: 250
verified_at: 2026-09-25
---

# Papercuts found putting keel cover in front of real PRs

## Context

Running `keel cover` on real pull requests (daneb/keel-cover-demo) and
committing `.keel/` in Moor turned up three problems an outside team would hit
on day one:

- **`test-movement` can block a change forever.** It blocks any change with
  code but no test. Its message says "confirm", but nothing records a
  confirmation, so a config or docs change stays blocked however carefully a
  human looked at it.
- **Blocked bundles are called failed.** `keel cover` reports a bundle that
  verifies as *blocked*, such as one with no chain, as "failed verification".
  It's the right result for coverage, but the wrong word.
- **keel's own driver scripts fail Shellcheck.** Each one sources
  `_common.sh` through a path built at runtime (SC1091), which fails CI in any
  repo that commits `.keel/`.

## Acceptance criteria

### AC-1 A code-only change raises a review flag naming its files

IF code changes with no test file changing THEN THE SYSTEM SHALL block
`test-movement`, write a flag naming the changed files to the spec's
`review-flags.txt`, and tell the reviewer to run `keel approve --stage review`.

oracle: test tests/papercuts.rs::a_code_only_change_is_flagged_for_review

### AC-2 A review sign-off acknowledges it

WHILE the spec's review approval is current for those flags THE SYSTEM SHALL
pass `test-movement` and name who reviewed it.

oracle: test tests/papercuts.rs::a_review_sign_off_clears_test_movement

### AC-3 A different change needs a fresh look

IF the changed files differ from the ones reviewed THEN THE SYSTEM SHALL treat
the review as superseded and block `test-movement` again.

oracle: test tests/papercuts.rs::a_new_change_supersedes_the_review

### AC-4 Blocked bundles are called blocked

IF a bundle's verification is blocked rather than failed THEN THE SYSTEM SHALL
report it from `keel cover` as "verification blocked", naming the blocked
checks.

oracle: test tests/papercuts.rs::a_chainless_bundle_is_reported_blocked

### AC-5 keel's driver scripts pass Shellcheck

THE SYSTEM SHALL ship driver scripts that pass `shellcheck` with no findings
when it is run without `-x`.

oracle: cmd `shellcheck assets/drivers/claude-code assets/drivers/codex assets/drivers/copilot assets/drivers/kiro assets/drivers/_common.sh` exit 0

## Out of scope

- Updating driver scripts that repositories already carry in `.keel/drivers/`
  (Moor excludes `.keel/` from Shellcheck).
- `keel init` still gitignoring `.keel/bundles/`.
- Moor's papercuts: the `unknown` approver in new projects, and the image
  build keeping a stale keel.
