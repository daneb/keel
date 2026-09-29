---
id: SPEC-0017
slug: worktree-test-older-git
schema: keel.spec/1
status: approved
scope:
- src/worktree.rs
- src/chain/mod.rs
- tests/**
budget:
  criteria: 5
  lines: 160
---

# keel's test suite passes inside a moor sandbox

## Context

keel's own suite fails when it runs inside a moor sandbox, for two reasons
that are both about the tests, not about keel:

1. `worktree::tests::gate_base_prefers_the_fetched_remote_trunk_over_a_stale_local_branch`
   deletes `refs/remotes/origin/HEAD` after a `git fetch`, because git 2.48 and
   later create that ref on fetch. On older git (Debian bookworm, which moor's
   sandboxes and many CI runners use, ships 2.39) the ref never exists, so the
   delete fails and the test panics before checking anything.
2. The tests start the keel binary with the test process's whole environment.
   A moor sandbox always sets `KEEL_CHAIN_SINK`, `KEEL_RUNTIME_ATTESTATION` and
   `GIT_CONFIG_*` (the operator's git identity). So spawned keel sends its chain
   to moor's sink instead of the test repo (the bundle and chain tests fail),
   the operator's name overrides the test repo's `user.name` (the canary
   redaction test fails), and every test run's fixture records land in the
   host's real audit trail.
3. keel's unit tests in `src/` run the approval code in-process, and
   `chain::record` (`src/chain/mod.rs`) sends every entry to the path in
   `KEEL_CHAIN_SINK` whenever that variable is set. So under a moor sandbox
   the unit tests write their fixture approvals (spec `demo`) into the real
   sink, whatever the integration tests do. This is the last leak: with
   `tests/**` fixed, `KEEL_CHAIN_SINK=$(mktemp) cargo test --bin keel` still
   writes six approval entries to that file. `record` needs a way for a test
   to keep its entries in the test repository's own chain, for example a
   sink setting that unit tests pass explicitly, or one that `#[cfg(test)]`
   code does not read from the process environment.

Measured in a moor sandbox: with the worktree test fixed and those variables
cleared, all 541 tests pass. A test suite should not depend on, or write
into, the environment of whoever runs it.

## Acceptance criteria

### AC-1 The worktree test passes when fetch did not create origin/HEAD

WHEN `git fetch` has not created `refs/remotes/origin/HEAD` THE SYSTEM SHALL
run `gate_base_prefers_the_fetched_remote_trunk_over_a_stale_local_branch` to
completion without deleting a ref that does not exist.

oracle: cmd `cargo test --bin keel worktree::tests::gate_base_prefers_the_fetched_remote_trunk_over_a_stale_local_branch` exit 0

### AC-2 The worktree test still exercises the fallback

WHILE that test checks which base the gate uses THE SYSTEM SHALL assert that
`refs/remotes/origin/HEAD` is absent from the test repository.

oracle: cmd `grep -q 'refs/remotes/origin/HEAD' src/worktree.rs` exit 0
oracle: human the test asserts the ref is absent (for example with `git rev-parse --verify --quiet refs/remotes/origin/HEAD` failing) before it calls the gate base

### AC-3 Tests start keel without the runner's KEEL_ and GIT_CONFIG_ variables

WHEN a test starts the keel binary THE SYSTEM SHALL remove `KEEL_CHAIN_SINK`,
`KEEL_RUNTIME_ATTESTATION` and every `GIT_CONFIG_*` variable inherited from the
test process, while keeping any of them the test sets itself.

oracle: cmd `env KEEL_RUNTIME_ATTESTATION=/nonexistent/posture.json GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=Outsider cargo test --quiet --test chain --test bundle --test runtime` exit 0

### AC-4 Tests never write to the runner's chain sink

WHILE the test suite runs THE SYSTEM SHALL write nothing to the file named by
an inherited `KEEL_CHAIN_SINK`.

oracle: cmd `sink=$(mktemp) && KEEL_CHAIN_SINK="$sink" cargo test --quiet && test ! -s "$sink"` exit 0

### AC-5 The full suite passes on git 2.39

WHEN `cargo test` runs with git 2.39 THE SYSTEM SHALL exit 0.

oracle: cmd `cargo test --quiet` exit 0

## Out of scope

- Changing `gate_base`'s candidate order.
- Changing how `KEEL_CHAIN_SINK` behaves for keel run normally (outside
  tests); in a sandbox it must keep sending entries to the sink.
- Requiring a minimum git version.
- How moor sets these variables; setting them is correct for the sandbox.
