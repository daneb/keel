---
id: TASKS-0009
slug: evidence-chain
schema: keel.tasks/1
---

# Tasks

### T-1 Chain entry format and in-process append
- criteria: AC-2
- files: src/chain/mod.rs, schemas/chain.json, tests/chain.rs
- budget: 110
- exit: test tests/chain.rs::verify_accepts_an_intact_chain

### T-2 Verify names the broken entry and checks the head anchor
- criteria: AC-3, AC-4
- files: src/chain/mod.rs, tests/chain.rs
- budget: 110
- depends_on: T-1
- exit: test tests/chain.rs::verify_names_each_tamper

### T-3 `keel chain verify` and `keel chain head`
- criteria: AC-2, AC-4
- files: src/cmd/chain.rs, src/cmd/mod.rs, src/main.rs, tests/chain.rs
- budget: 80
- depends_on: T-2
- exit: test tests/chain.rs::verify_rejects_a_recomputed_tail

### T-4 Sink mode, writer marking and redaction
- criteria: AC-5, AC-6, AC-7
- files: src/chain/mod.rs, src/config.rs, tests/chain.rs
- budget: 130
- depends_on: T-3
- exit: test tests/chain.rs::canary_secret_absent_and_chain_verifies

### T-5 Hook the three evidence writers
- criteria: AC-1, AC-8
- files: src/approval.rs, src/gate/mod.rs, src/trajectory/mod.rs, tests/chain.rs
- budget: 150
- depends_on: T-4
- exit: test tests/chain.rs::approval_gate_and_run_entries_link
