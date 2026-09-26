---
id: SPEC-0014
slug: ci-runtime
schema: keel.spec/1
status: approved
scope:
- src/runtime/**
- src/cmd/runtime.rs
- src/cmd/mod.rs
- src/main.rs
- runtime/**
- docs/decisions/**
- tests/ci_runtime.rs
- schemas/posture.json
budget:
  criteria: 8
  lines: 700
verified_at: 2026-09-25
---

# GitHub Actions as a runtime: CI builds and attests the bundle

## Context

ADR-0002 (`docs/decisions/0002-github-actions-runtime.md`) makes GitHub Actions
keel's second runtime, after Moor. The runner step is the host; keel runs in a
hardened container on it, under gVisor where available; the host chain starts
from the repository's and the runner appends to it; the bundle is committed
back to the pull request so `keel cover` counts it. This spec is gate-only:
no coding agent runs in CI.

The host's work is done by keel itself, run on the runner outside the
container: two runtime-neutral subcommands, and a composite action that drives
Docker around them.

## Acceptance criteria

### AC-1 The host folds keel's sink into its chain

WHEN `keel runtime fold --sink <file> --chain <file> --writer <name>` runs THE
SYSTEM SHALL append each new sink payload to the chain as a `keel.chain/1`
entry with that writer, keep an unparseable line as `sink_malformed`, and
append nothing for lines already folded.

oracle: test tests/ci_runtime.rs::fold_appends_payloads_once_with_the_writer

### AC-2 The host attests the container from outside

WHEN `keel runtime attest --inspect <file> --chain <file> --out <file>` runs
THE SYSTEM SHALL write a `keel.posture/1` attestation derived from the
`docker inspect` output, and append an `attest` entry carrying its SHA-256 to
the chain.

oracle: test tests/ci_runtime.rs::attest_derives_posture_from_inspect_output

### AC-3 Kernel isolation is proven only by gVisor

IF the inspected container's runtime is not `runsc` THEN THE SYSTEM SHALL mark
`kernel.isolated` as `unproven`, and SHALL mark it `proven` only when it is.

oracle: test tests/ci_runtime.rs::kernel_isolated_is_proven_only_under_runsc

### AC-4 The run's identity is recorded, as a claim

WHERE the `GITHUB_REPOSITORY`, `GITHUB_WORKFLOW`, `GITHUB_RUN_ID` and
`GITHUB_SHA` variables are set THE SYSTEM SHALL include them in the
attestation's `runtime_identity`, marked as claimed by the runner, as an
optional field `schemas/posture.json` allows.

oracle: test tests/ci_runtime.rs::run_identity_is_recorded_as_a_claim

### AC-5 The container is hardened and the host's files are out of reach

THE SYSTEM SHALL start keel's container with a read-only root filesystem, all
capabilities dropped, `no-new-privileges`, a non-root user, the network named
by the `network` input (default `bridge`), and `runsc` when installed. It
SHALL mount no path under `$RUNNER_TEMP` except the sink directory and the
read-only attestation.

oracle: test tests/ci_runtime.rs::the_action_hardens_the_container_and_keeps_host_files_out

### AC-6 The chain starts from the repository's, verified

WHEN the action starts THE SYSTEM SHALL copy the repository's
`.keel/chain.jsonl` to `$RUNNER_TEMP`, stop the job if `keel chain verify`
fails on it, and append to that copy.

oracle: test tests/ci_runtime.rs::the_action_starts_from_the_verified_repository_chain

### AC-7 The bundle is built, verified and committed back

WHEN the gated run ends THE SYSTEM SHALL fold the sink, run `keel export
--chain` on the host chain, stop the job unless `keel bundle verify` passes,
and commit the bundle under `.keel/bundles/` to the pull request's branch.

oracle: test tests/ci_runtime.rs::the_action_builds_verifies_and_commits_the_bundle

### AC-8 It works on a real pull request

WHEN the action runs on a pull request in a repository with an approved spec
THE SYSTEM SHALL push a bundle commit that `keel cover` then reports as
covering the pull request's head.

oracle: human a maintainer runs the action on a pull request in daneb/keel-cover-demo and sees keel cover report covered on the bundle commit

## Out of scope

- Running a coding agent in CI (a driver, model API keys, egress to them).
- Signing: GitHub OIDC tokens, build-provenance attestations, cosign.
- Pull requests from forks, which can't be given `contents: write`.
- Changing Moor, which keeps its own implementation of the same frozen formats.
