---
id: SPEC-0011
slug: bundle-v1
schema: keel.spec/1
status: approved
scope:
- src/evidence/**
- src/chain/mod.rs
- src/approval.rs
- src/gate/diff.rs
- src/gate/g2.rs
- src/cmd/bundle.rs
- src/cmd/mod.rs
- src/cmd/run.rs
- src/main.rs
- schemas/**
- tests/bundle.rs
budget:
  criteria: 8
  lines: 800
verified_at: 2026-09-24
---

# A bundle an auditor can verify offline

## Context

keel's bundle (SPEC-0003) proves its own members haven't changed since export,
but not that they are the right members. Nothing ties the shipped spec to the
approval that signed it off, or a gate verdict to the chain entry recorded when
it was reached. The bundle also doesn't carry the chain (SPEC-0009) or the diff
that was gated. So an auditor who receives one still has to take the join on
trust.

This spec makes the bundle carry its own proof. It adds the chain and the exact
diff G2 judged, and a verifier that checks every link using only the bundle.
Under a runtime, keel never holds the chain (keel ADR-0001), so `--chain` lets
the runtime supply the one it wrote.

The verifier ships as `keel bundle verify`, not yet a separate `keel-verify`
binary. keel has no library crate, so a second binary means restructuring the
crate first. AC-8 holds the subcommand to the same standard a separate binary
would meet: no repository, no `.keel/`, no network.

## Acceptance criteria

### AC-1 The bundle carries the chain up to its run

WHEN `keel export` runs without `--chain` THE SYSTEM SHALL include
`chain.jsonl` holding `.keel/chain.jsonl` from its first entry through the
run's `run_end` entry, and record the last included hash as `chain_head` in
the manifest.

oracle: test tests/bundle.rs::bundle_carries_the_chain_through_run_end

### AC-2 A runtime's chain can be supplied

WHERE `keel export --chain <file>` is given THE SYSTEM SHALL include that
file's entries through the run's `run_end` entry as `chain.jsonl` in place of
the repository's chain.

oracle: test tests/bundle.rs::a_supplied_chain_replaces_the_local_one

### AC-3 The gated diff is kept

WHEN G2 measures a diff THE SYSTEM SHALL write the full patch it measured,
untracked files included, to the run's evidence as `diff.patch`.

oracle: test tests/bundle.rs::g2_keeps_the_patch_it_judged

### AC-4 An intact bundle verifies

WHEN `keel bundle verify <archive>` is invoked on a bundle whose members,
chain and joins all hold THE SYSTEM SHALL exit 0 and print each check with
its verdict.

oracle: test tests/bundle.rs::an_intact_bundle_verifies

### AC-5 Approvals must match what shipped

IF the latest `approval` chain entry for a stage does not equal the hash of
the spec, plan or tasks shipped in the bundle THEN THE SYSTEM SHALL fail
`keel bundle verify` and name the stage.

oracle: test tests/bundle.rs::a_swapped_spec_fails_its_approval

### AC-6 Verdicts and the trajectory must match the chain

IF a gate result in the bundle has no `gate` chain entry with its SHA-256, or
the run's `run_end` entry does not carry the SHA-256 of the shipped
trajectory THEN THE SYSTEM SHALL fail `keel bundle verify` and name the
member.

oracle: test tests/bundle.rs::a_replaced_verdict_or_trajectory_is_named

### AC-7 The report is machine-readable

WHEN `keel bundle verify --json <archive>` is invoked THE SYSTEM SHALL print
one `keel.bundleverify/1` object listing every check with its verdict and
detail.

oracle: test tests/bundle.rs::json_report_validates_against_the_published_schema

### AC-8 Verification needs only the bundle

WHEN `keel bundle verify` runs in a directory with no repository and no
`.keel/`, with `HOME` unset and proxy variables pointing at an unreachable
address THE SYSTEM SHALL verify the bundle with the same verdict.

oracle: test tests/bundle.rs::verifies_with_nothing_but_the_bundle

## Out of scope

- A separate `keel-verify` binary, and signatures (phase 3).
- `keel report --html` inside the bundle, and `keel serve --bundle`.
- Moor passing its host chain to `keel export --chain`.
- Checking a diff's commits against `push` entries. They sit in the
  runtime's chain and need Moor's side first.
- Leaving the trajectory out of the bundle. It is included today, and
  changing that is a separate disclosure decision.
