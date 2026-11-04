---
id: TASKS-0022
slug: adrs-for-recent-specs
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 ADR-0026 records the spent and injected token rule
- criteria: AC-1
- files: .keel/store/decisions/ADR-0026-spent-tokens-are-reported-injected-tokens-are-estimated.md
- budget: 45
- exit: `grep -q "never summed" .keel/store/decisions/ADR-0026-spent-tokens-are-reported-injected-tokens-are-estimated.md` exits 0

### T-2 ADR-0027 records the oracle sensitivity rule
- criteria: AC-2
- files: .keel/store/decisions/ADR-0027-an-oracle-must-fail-without-its-change.md
- budget: 45
- exit: `grep -q "advisory" .keel/store/decisions/ADR-0027-an-oracle-must-fail-without-its-change.md` exits 0

### T-3 ADR-0028 records the per-criterion human judgement rule
- criteria: AC-3
- files: .keel/store/decisions/ADR-0028-a-human-oracle-is-discharged-per-criterion-per-run.md
- budget: 45
- exit: `grep -q "per criterion" .keel/store/decisions/ADR-0028-a-human-oracle-is-discharged-per-criterion-per-run.md` exits 0

### T-4 Each ADR names the test that backs it
- criteria: AC-4
- files: .keel/store/decisions/ADR-0026-spent-tokens-are-reported-injected-tokens-are-estimated.md, .keel/store/decisions/ADR-0027-an-oracle-must-fail-without-its-change.md, .keel/store/decisions/ADR-0028-a-human-oracle-is-discharged-per-criterion-per-run.md
- depends_on: T-1, T-2, T-3
- budget: 35
- exit: `grep -q "tests/driver_usage.rs"` on ADR-0026, `grep -q "tests/oracle_sensitivity.rs"` on ADR-0027 and `grep -q "tests/g3_judgement.rs"` on ADR-0028 all exit 0

### T-5 The store stays valid with the new ADRs
- criteria: AC-5
- files: .keel/store/decisions/ADR-0026-spent-tokens-are-reported-injected-tokens-are-estimated.md, .keel/store/decisions/ADR-0027-an-oracle-must-fail-without-its-change.md, .keel/store/decisions/ADR-0028-a-human-oracle-is-discharged-per-criterion-per-run.md, .keel/store/decisions/ADR-0000-index.md
- depends_on: T-4, T-6
- budget: 20
- exit: `keel store check` exits 0

### T-6 The index lists the new ADRs
- criteria: AC-6
- files: .keel/store/decisions/ADR-0000-index.md
- depends_on: T-1, T-2, T-3
- budget: 6
- exit: `grep -c "^| \[ADR-00\(26\|27\|28\)\]" .keel/store/decisions/ADR-0000-index.md | grep -qx 3` exits 0
