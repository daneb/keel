---
id: TASKS-0016
slug: next-json-approval-standing
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 Approval stages carry their standing
- criteria: AC-1
- files: scope
- budget: 25
- exit: `cargo test --test json next_json_reports_approval_standing` exits 0

### T-2 A rejection names who and why
- criteria: AC-2
- files: scope
- budget: 25
- exit: `cargo test --test json next_json_reports_who_rejected_and_why` exits 0

### T-3 A rejected or stale approval names its re-check command
- criteria: AC-3
- files: scope
- budget: 25
- exit: `cargo test --test json next_json_names_the_recheck_command` exits 0

### T-4 Other stages are unchanged
- criteria: AC-4
- files: scope
- budget: 25
- exit: `cargo test --test json next_json_omits_approval_outside_approval_stages` exits 0

### T-5 Existing fields keep their values
- criteria: AC-5
- files: scope
- budget: 25
- exit: `cargo test --test json next_json` exits 0

### T-6 The change is recorded for tools that read the schema
- criteria: AC-6
- files: scope
- budget: 25
- exit: `grep -q 'approval.recheck' CHANGELOG.md` exits 0

