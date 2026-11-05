---
id: TASKS-0023
slug: stale-run-after-reapproval
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 A run older than the plan approval does not satisfy the Run stage
- criteria: AC-1
- files: src/pipeline.rs, tests/pipeline.rs
- budget: 55
- exit: test tests/pipeline.rs::a_run_older_than_the_plan_approval_does_not_satisfy_the_run_stage

### T-2 A run older than the spec approval does not satisfy the Run stage
- criteria: AC-2
- files: src/pipeline.rs, tests/pipeline.rs
- depends_on: T-1
- budget: 35
- exit: test tests/pipeline.rs::a_run_older_than_the_spec_approval_does_not_satisfy_the_run_stage

### T-3 A run that started after both approvals still counts
- criteria: AC-3
- files: tests/pipeline.rs
- depends_on: T-2
- budget: 25
- exit: test tests/pipeline.rs::a_run_newer_than_both_approvals_still_satisfies_the_run_stage

### T-4 `keel next --json` names the stale run
- criteria: AC-4
- files: src/pipeline.rs, src/cmd/next.rs, tests/json.rs
- depends_on: T-3
- budget: 50
- exit: test tests/json.rs::next_json_names_a_run_that_predates_the_approvals

### T-5 `keel next` says why the run is needed again
- criteria: AC-5
- files: src/cmd/next.rs, tests/pipeline.rs
- depends_on: T-4
- budget: 50
- exit: test tests/pipeline.rs::keel_next_explains_a_stale_run
