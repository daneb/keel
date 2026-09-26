---
id: SPEC-0015
slug: runtime-fixes
schema: keel.spec/1
status: approved
scope:
- runtime/action.yml
- tests/runtime_fixes.rs
budget:
  criteria: 4
  lines: 150
verified_at: 2026-09-26
---

# Runtime fixes from the first real CI run: gVisor's tarball, and no self-loop

## Context

The runtime Action's first real run (daneb/keel-cover-demo#4) gated a pull
request and pushed a bundle that `keel cover` reported as covering the head,
which is AC-8 of SPEC-0014. It also found two bugs:

- **gVisor never installed.** gVisor no longer publishes a standalone
  `runsc`. Each release is a `gvisor.tar.zstd` with a `.sha512` beside it,
  in a dated folder, and the `latest/<arch>/runsc` path from its install docs
  returns 404. The Action fell back to runc, and `kernel.isolated` was
  honestly attested as unproven, but gVisor never ran.
- **The Action would loop.** Its bundle commit, pushed by
  `github-actions[bot]`, starts the runtime workflow again, which would gate
  that commit and push another bundle. Only GitHub's approval rule for that
  actor stopped it.

## Acceptance criteria

### AC-1 gVisor comes from a pinned release tarball, verified

WHEN the Action installs gVisor THE SYSTEM SHALL download `gvisor.tar.zstd`
and its `.sha512` from the release named by the `gvisor-version` input
(default `20260921.0`) for the runner's architecture, check the digest,
extract `runsc`, and register it with Docker.

oracle: test tests/runtime_fixes.rs::gvisor_comes_from_a_pinned_verified_tarball

### AC-2 The pinned release exists

THE SYSTEM SHALL pin a gVisor release whose x86_64 tarball can be downloaded.

oracle: cmd `curl -sfI https://storage.googleapis.com/gvisor/releases/release/20260921.0/x86_64/gvisor.tar.zstd` exit 0

### AC-3 The Action skips its own bundle commit

IF the head commit was authored by `github-actions[bot]` and changes only
files under `.keel/bundles/` THEN THE SYSTEM SHALL skip every later step and
say the head is the runtime's own bundle commit.

oracle: test tests/runtime_fixes.rs::the_action_skips_its_own_bundle_commit

## Out of scope

- Signing, forks, and running an agent in CI.
- Workflows that already use v0.10.0. They need `@v0.10.1` to pick up these
  fixes.
