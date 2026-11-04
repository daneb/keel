---
id: TASKS-0021
slug: g3-human-judgement
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 G3 fails while a human oracle is unjudged
- criteria: AC-1
- files: src/gate/g3.rs, src/approval.rs, tests/g3_judgement.rs
- budget: 60
- exit: `cargo test --test g3_judgement g3_fails_while_any_human_oracle_lacks_a_judgement` exits 0

### T-2 A judgement is recorded per criterion and bound to evidence
- criteria: AC-2
- files: src/approval.rs, src/evidence/mod.rs, src/evidence/manifest.rs, src/cmd/review.rs, src/cmd/mod.rs, src/main.rs, tests/g3_judgement.rs
- depends_on: T-1
- budget: 70
- exit: `cargo test --test g3_judgement keel_judge_records_a_hash_bound_judgement_in_the_chain` exits 0

### T-3 A judgement on changed evidence is superseded
- criteria: AC-3
- files: src/gate/g3.rs, src/approval.rs, src/evidence/mod.rs, tests/g3_judgement.rs
- depends_on: T-2
- budget: 50
- exit: `cargo test --test g3_judgement a_judgement_is_superseded_when_the_evidence_changes` exits 0

### T-4 A failed judgement fails G3 and returns to the agent
- criteria: AC-4
- files: src/gate/g3.rs, src/run.rs, tests/g3_judgement.rs
- depends_on: T-3
- budget: 60
- exit: `cargo test --test g3_judgement a_failed_judgement_fails_g3_and_records_a_failure_episode` exits 0

### T-5 The reviewer gets one brief, and the approval records it
- criteria: AC-5
- files: src/cmd/review.rs, src/cmd/approve.rs, src/approval.rs, tests/g3_judgement.rs
- depends_on: T-4
- budget: 70
- exit: `cargo test --test g3_judgement keel_review_writes_a_brief_and_the_merge_approval_records_its_hash` exits 0

### T-6 Approval speed is reported as a signal, never as proof
- criteria: AC-6
- files: src/metrics.rs, tests/g3_judgement.rs
- depends_on: T-5
- budget: 60
- exit: `cargo test --test g3_judgement metrics_flags_fast_approvals_and_states_it_measures_elapsed_time` exits 0

### T-7 Bundles carry judgements and verify them offline
- criteria: AC-7
- files: src/evidence/mod.rs, src/evidence/manifest.rs, tests/g3_judgement.rs
- depends_on: T-6
- budget: 70
- exit: `cargo test --test g3_judgement bundle_verify_rejects_judgements_that_do_not_match_the_evidence` exits 0
