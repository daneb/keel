---
id: SPEC-0009
slug: evidence-chain
schema: keel.spec/1
status: approved
scope:
- src/chain/**
- src/cmd/chain.rs
- src/cmd/mod.rs
- src/main.rs
- src/config.rs
- src/approval.rs
- src/gate/mod.rs
- src/trajectory/mod.rs
- tests/chain.rs
- schemas/**
budget:
  criteria: 8
  lines: 600
verified_at: 2026-09-23
---

# One hash-chained evidence log

## Context

Evidence is split between keel's approvals, gate verdicts and trajectory and
Moor's host-side audit chain; an auditor has to trust both and the join
between them. ADR-0001 (`docs/decisions/0001-one-chain-runtime-writes.md`)
decides that keel owns one chain format and its verifier, while the runtime's
host process is the only writer whenever a runtime is present. This spec is
the keel side of that decision.

## Acceptance criteria

### AC-1 Keel's own events enter the chain, linked

WHEN keel records an approval, a gate verdict, or a run start THE SYSTEM SHALL
append one `keel.chain/1` entry carrying the SHA-256 of the entry before it.

oracle: test tests/chain.rs::approval_gate_and_run_entries_link
oracle: test tests/chain.rs::every_entry_validates_against_the_published_schema

### AC-2 An intact chain verifies

WHEN `keel chain verify` is invoked on a chain whose every entry links to its
predecessor THE SYSTEM SHALL exit 0 and print the head hash.

oracle: test tests/chain.rs::verify_accepts_an_intact_chain

### AC-3 Tampering names the entry

IF a chain entry is edited, deleted, reordered or inserted THEN THE SYSTEM
SHALL exit non-zero from `keel chain verify` and print the sequence number of
the first entry that does not link.

oracle: test tests/chain.rs::verify_names_each_tamper

### AC-4 A rewritten tail fails against its anchor

WHEN `keel chain verify --head <hash>` is invoked and the last entry's hash
differs from `<hash>` THE SYSTEM SHALL exit non-zero and print both hashes.

oracle: test tests/chain.rs::verify_rejects_a_recomputed_tail

### AC-5 Under a runtime, keel never holds the pen

WHERE `KEEL_CHAIN_SINK` is set THE SYSTEM SHALL write each entry payload to
that sink and SHALL NOT open the chain file for writing.

oracle: test tests/chain.rs::sink_mode_never_writes_the_chain_file

### AC-6 Self-written chains say so

WHILE `KEEL_CHAIN_SINK` is unset THE SYSTEM SHALL mark every entry it appends
with `writer: "in-process"` and report the chain as self-attested from
`keel chain verify`.

oracle: test tests/chain.rs::in_process_chain_reports_self_attested

### AC-7 Declared secrets are redacted before hashing

WHEN keel builds a chain entry THE SYSTEM SHALL replace the value of every
secret named in `chain.secrets` with `[REDACTED]` before computing the entry
hash.

oracle: test tests/chain.rs::canary_secret_absent_and_chain_verifies

### AC-8 The trajectory is committed to, not copied

WHEN a run ends THE SYSTEM SHALL append a `run_end` entry carrying the SHA-256
of the run's `trajectory.jsonl` and SHALL NOT copy trajectory content into the
chain.

oracle: test tests/chain.rs::run_end_carries_trajectory_hash_only

## Out of scope

- Runtime attestation and the `keel.runtime/1` contract, including posture
  checks that report `blocked` (spec `runtime-contract`).
- Moor's side of the sink: reading payloads and appending on the host.
- Egress and push entries; they are runtime-written.
- Bundle changes, the offline verifier and signatures (spec `bundle-v1`).
- The PR check.
- Authenticating the approver named in `keel.approval/1`.
