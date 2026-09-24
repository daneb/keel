---
id: TASKS-0011
slug: bundle-v1
schema: keel.tasks/1
---

# Tasks

### T-1 G2 keeps the patch it judged
- criteria: AC-3
- files: src/gate/diff.rs, src/gate/g2.rs, tests/bundle.rs
- budget: 80
- exit: test tests/bundle.rs::g2_keeps_the_patch_it_judged

### T-2 The chain rides in the bundle
- criteria: AC-1, AC-2
- files: src/evidence/mod.rs, src/evidence/manifest.rs, src/cmd/run.rs, src/main.rs, schemas/manifest.json, tests/bundle.rs
- budget: 140
- depends_on: T-1
- exit: test tests/bundle.rs::a_supplied_chain_replaces_the_local_one

### T-3 Verifier: members, chain, verdicts, trajectory
- criteria: AC-4, AC-6
- files: src/evidence/verify.rs, src/evidence/mod.rs, src/chain/mod.rs, src/cmd/bundle.rs, src/cmd/mod.rs, src/main.rs, tests/bundle.rs
- budget: 150
- depends_on: T-2
- exit: test tests/bundle.rs::a_replaced_verdict_or_trajectory_is_named

### T-4 Approvals must match what shipped
- criteria: AC-5
- files: src/approval.rs, src/evidence/verify.rs, tests/bundle.rs
- budget: 110
- depends_on: T-3
- exit: test tests/bundle.rs::a_swapped_spec_fails_its_approval

### T-5 JSON report, and nothing but the bundle
- criteria: AC-7, AC-8
- files: src/cmd/bundle.rs, schemas/bundleverify.json, tests/bundle.rs
- budget: 120
- depends_on: T-4
- exit: test tests/bundle.rs::verifies_with_nothing_but_the_bundle
