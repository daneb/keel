---
id: TASKS-0010
slug: runtime-contract
schema: keel.tasks/1
---

# Tasks

### T-1 Posture schema and attestation parsing
- criteria: AC-6
- files: src/runtime/mod.rs, schemas/posture.json, tests/runtime.rs
- budget: 140
- exit: test tests/runtime.rs::malformed_attestation_names_the_field

### T-2 Posture gate verdicts
- criteria: AC-2, AC-3, AC-4, AC-5, AC-7
- files: src/runtime/mod.rs, src/config.rs, tests/runtime.rs
- budget: 150
- depends_on: T-1
- exit: test tests/runtime.rs::unproven_property_blocks_before_the_driver

### T-3 Preflight in both run paths, attest entry first
- criteria: AC-1, AC-8
- files: src/cmd/run.rs, src/runtime/mod.rs, tests/runtime.rs
- budget: 150
- depends_on: T-2
- exit: test tests/runtime.rs::attest_entry_precedes_the_driver
