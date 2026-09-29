---
id: TASKS-0017
slug: worktree-test-older-git
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 The worktree test passes when fetch did not create origin/HEAD
- criteria: AC-1
- files: scope
- budget: 28
- exit: `cargo test --bin keel worktree::tests::gate_base_prefers_the_fetched_remote_trunk_over_a_stale_local_branch` exits 0

### T-2 The worktree test still exercises the fallback
- criteria: AC-2
- files: scope
- budget: 28
- exit: `grep -q 'refs/remotes/origin/HEAD' src/worktree.rs` exits 0

### T-3 Tests start keel without the runner's KEEL_ and GIT_CONFIG_ variables
- criteria: AC-3
- files: scope
- budget: 28
- exit: `env KEEL_RUNTIME_ATTESTATION=/nonexistent/posture.json GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=Outsider cargo test --quiet --test chain --test bundle --test runtime` exits 0

### T-4 Tests never write to the runner's chain sink
- criteria: AC-4
- files: scope
- budget: 28
- exit: `sink=$(mktemp) && KEEL_CHAIN_SINK="$sink" cargo test --quiet && test ! -s "$sink"` exits 0

### T-5 The full suite passes on git 2.39
- criteria: AC-5
- files: scope
- budget: 28
- exit: `cargo test --quiet` exits 0

