---
id: SPEC-0022
slug: adrs-for-recent-specs
schema: keel.spec/1
status: approved
scope:
- .keel/store/decisions/ADR-0026-*.md
- .keel/store/decisions/ADR-0027-*.md
- .keel/store/decisions/ADR-0028-*.md
- .keel/store/decisions/ADR-0000-index.md
budget:
  criteria: 6
  lines: 200
---

# Record the three decisions that SPEC-0019, SPEC-0020 and SPEC-0021 asked for

## Context

SPEC-0019 (`driver-usage`), SPEC-0020 (`oracle-sensitivity`) and SPEC-0021
(`g3-human-judgement`) each ended with a "Decision to record" section naming an
ADR. All three shipped (PRs 47, 48 and 49) and none of the ADRs was written:
their scope did not include `.keel/store/decisions/`, so a scope-respecting
build could not create them. `.keel/store/decisions/` stops at ADR-0025, so the
reasons for these rules exist only in three spec files.

Each ADR follows ADR-0020's shape: front matter with `schema: keel.adr/1`,
then Context, Decision and Consequences. The text states what the shipped code
does, which the agent reads from the code and tests named below, not from
memory.

## Acceptance criteria

### AC-1 ADR-0026 records the spent and injected token rule

THE SYSTEM SHALL contain `ADR-0026-spent-tokens-are-reported-injected-tokens-are-estimated.md`,
whose Decision states that spent tokens are reported by the driver, injected
tokens are estimated by keel, and the two are never summed.

oracle: cmd `grep -q "never summed" .keel/store/decisions/ADR-0026-spent-tokens-are-reported-injected-tokens-are-estimated.md` exit 0

### AC-2 ADR-0027 records the oracle sensitivity rule

THE SYSTEM SHALL contain `ADR-0027-an-oracle-must-fail-without-its-change.md`,
whose Decision states that sensitivity is native and blocking while mutation
is a plugin and advisory.

oracle: cmd `grep -q "advisory" .keel/store/decisions/ADR-0027-an-oracle-must-fail-without-its-change.md` exit 0

### AC-3 ADR-0028 records the per-criterion human judgement rule

THE SYSTEM SHALL contain `ADR-0028-a-human-oracle-is-discharged-per-criterion-per-run.md`,
whose Decision states that a human oracle is discharged per criterion and per
run, and that one merge approval no longer stands in for every judgement.

oracle: cmd `grep -q "per criterion" .keel/store/decisions/ADR-0028-a-human-oracle-is-discharged-per-criterion-per-run.md` exit 0

### AC-4 Each ADR names the test that backs it

WHEN an ADR from this change is read THE SYSTEM SHALL show a link to its
backing test file: `tests/driver_usage.rs` for ADR-0026,
`tests/oracle_sensitivity.rs` for ADR-0027 and `tests/g3_judgement.rs` for
ADR-0028.

oracle: cmd `grep -q "tests/driver_usage.rs" .keel/store/decisions/ADR-0026-spent-tokens-are-reported-injected-tokens-are-estimated.md` exit 0
oracle: cmd `grep -q "tests/oracle_sensitivity.rs" .keel/store/decisions/ADR-0027-an-oracle-must-fail-without-its-change.md` exit 0
oracle: cmd `grep -q "tests/g3_judgement.rs" .keel/store/decisions/ADR-0028-a-human-oracle-is-discharged-per-criterion-per-run.md` exit 0

### AC-5 The store stays valid with the new ADRs

WHEN `keel store check` runs after this change THE SYSTEM SHALL exit 0.

oracle: cmd `keel store check` exit 0

### AC-6 The index lists the new ADRs

THE SYSTEM SHALL list ADR-0026, ADR-0027 and ADR-0028 in
`.keel/store/decisions/ADR-0000-index.md`, one table row each with phase
`field`, linking to the file.

oracle: cmd `grep -c "^| \[ADR-00\(26\|27\|28\)\]" .keel/store/decisions/ADR-0000-index.md | grep -qx 3` exit 0

## Out of scope

- Changing any rule. These ADRs record what shipped, they do not revise it.
- Reworking existing ADRs or the store's other sections. The index changes by
  three appended rows only.
