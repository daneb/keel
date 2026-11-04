---
id: SPEC-0021
slug: g3-human-judgement
schema: keel.spec/1
status: approved
scope:
- src/gate/g3.rs
- src/approval.rs
- src/cmd/approve.rs
- src/cmd/review.rs
- src/cmd/mod.rs
- src/main.rs
- src/run.rs
- src/metrics.rs
- src/evidence/**
- tests/g3_judgement.rs
budget:
  criteria: 7
  lines: 480
verified_at: 2026-11-03
---

# G3 discharges every human oracle and records what the reviewer saw

## Context

G3 passes on four checks: earlier gates green, the diff within
`max_reviewable_lines`, the evidence complete, and one `merge` approval bound
to the artefact hash. Two gaps follow.

**Human oracles are never discharged.** G2's `oracle-coverage` counts `human`
oracles as "awaiting human judgement" and moves on. G3 accepts a merge
approval without asking about any of them, so a criterion whose only oracle
is human can ship with nobody having judged it. ROADMAP.md already records
one (`evidence-bundle` AC-6) that has never been exercised.

**The approval does not say what was reviewed.** A merge approval proves a
named person said yes to a hash. It does not record what they were shown,
and nothing distinguishes a ten-second yes on 680 lines from a real review.

This spec makes each human oracle its own recorded judgement, gives the
reviewer one brief to decide from, and makes fast approvals visible in
`keel metrics` under the same honesty rule as ADR-0020.

## Acceptance criteria

### AC-1 G3 fails while a human oracle is unjudged

WHEN G3 runs and a criterion has a `human` oracle with no judgement recorded
against the current run's evidence THE SYSTEM SHALL fail check
`G3/human-oracles` and name each unjudged criterion with the judgement text
from its oracle.

oracle: cmd `cargo test --test g3_judgement g3_fails_while_any_human_oracle_lacks_a_judgement` exit 0

### AC-2 A judgement is recorded per criterion and bound to evidence

WHEN a person runs `keel judge <slug> <criterion> --pass` or `--fail` with
an optional `--note` THE SYSTEM SHALL append the judgement to the spec's
approval log and the evidence chain, bound to the hash of the run's diff and
`oracles.json`.

oracle: cmd `cargo test --test g3_judgement keel_judge_records_a_hash_bound_judgement_in_the_chain` exit 0

### AC-3 A judgement on changed evidence is superseded

IF the run's diff or `oracles.json` changes after a judgement is recorded
THEN THE SYSTEM SHALL treat that judgement as superseded, report both hashes,
and fail `G3/human-oracles` until the criterion is judged again.

oracle: cmd `cargo test --test g3_judgement a_judgement_is_superseded_when_the_evidence_changes` exit 0

### AC-4 A failed judgement fails G3 and returns to the agent

IF a criterion's latest judgement is `fail` THEN THE SYSTEM SHALL fail
`G3/human-oracles` with the reviewer's note, and record a failure episode
for G4 with attribution left for classification.

oracle: cmd `cargo test --test g3_judgement a_failed_judgement_fails_g3_and_records_a_failure_episode` exit 0

### AC-5 The reviewer gets one brief, and the approval records it

WHEN a person runs `keel review <slug>` THE SYSTEM SHALL print each
criterion's oracle outcome and sensitivity result, declared and computed
blast radius, G2.5 findings, reviewable lines and spent tokens, and write
the brief to the run's evidence; a later `merge` approval SHALL record the
brief's hash.

oracle: cmd `cargo test --test g3_judgement keel_review_writes_a_brief_and_the_merge_approval_records_its_hash` exit 0

### AC-6 Approval speed is reported as a signal, never as proof

WHEN `keel metrics` runs THE SYSTEM SHALL report, per merge approval, the
seconds from the brief being written to the approval and the reviewable
lines, flag approvals under `[review] min_seconds_per_100_lines` (default 30)
as `fast`, and print that this measures elapsed time, not attention.

oracle: cmd `cargo test --test g3_judgement metrics_flags_fast_approvals_and_states_it_measures_elapsed_time` exit 0

### AC-7 Bundles carry judgements and verify them offline

WHEN `keel export` builds a bundle THE SYSTEM SHALL include every judgement
and the review brief, and `keel bundle verify` SHALL fail a bundle whose
judgement hashes do not match its diff and `oracles.json`.

oracle: cmd `cargo test --test g3_judgement bundle_verify_rejects_judgements_that_do_not_match_the_evidence` exit 0

## Out of scope

- Blocking a fast approval. AC-6 makes it visible; deciding a threshold that
  refuses a person is a policy decision for after the data exists.
- A second required reviewer, or approvals by role. The identity model stays
  as ADR-0024 defines it.
- A goldfish-style fresh-context model review at G3. That belongs behind the
  existing reviewer contract at G2.5 (ADR-0017), as its own spec.

## Decision to record

An ADR, "A human oracle is discharged per criterion, per run", recording why
one merge approval no longer stands in for every human judgement.
