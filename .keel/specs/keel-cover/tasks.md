---
id: TASKS-0012
slug: keel-cover
schema: keel.tasks/1
---

# Tasks

### T-1 Content hash, and G2 records it
- criteria: AC-1, AC-2
- files: src/cover/mod.rs, src/gate/g2.rs, src/main.rs, tests/cover.rs
- budget: 150
- exit: test tests/cover.rs::content_hash_is_the_same_committed_or_not

### T-2 Coverage decision and the keel cover command
- criteria: AC-3, AC-4, AC-5, AC-6
- files: src/cover/mod.rs, src/cmd/cover.rs, src/cmd/mod.rs, src/main.rs, tests/cover.rs
- budget: 150
- depends_on: T-1
- exit: test tests/cover.rs::an_uncovered_change_names_each_reason

### T-3 JSON report and schema
- criteria: AC-7
- files: src/cmd/cover.rs, schemas/cover.json, tests/cover.rs
- budget: 80
- depends_on: T-2
- exit: test tests/cover.rs::json_report_validates_against_the_published_schema

### T-4 The GitHub Action
- criteria: AC-8
- files: action.yml, tests/cover.rs
- budget: 90
- depends_on: T-3
- exit: test tests/cover.rs::the_action_runs_keel_cover_on_the_pr_head
