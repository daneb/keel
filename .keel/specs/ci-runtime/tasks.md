---
id: TASKS-0014
slug: ci-runtime
schema: keel.tasks/1
---

# Tasks

### T-1 keel runtime fold
- criteria: AC-1
- files: src/runtime/host.rs, src/runtime/mod.rs, src/cmd/runtime.rs, src/cmd/mod.rs, src/main.rs, tests/ci_runtime.rs
- budget: 130
- exit: test tests/ci_runtime.rs::fold_appends_payloads_once_with_the_writer

### T-2 keel runtime attest
- criteria: AC-2, AC-3, AC-4
- files: src/runtime/host.rs, src/cmd/runtime.rs, schemas/posture.json, tests/ci_runtime.rs
- budget: 150
- depends_on: T-1
- exit: test tests/ci_runtime.rs::kernel_isolated_is_proven_only_under_runsc

### T-3 The runtime action
- criteria: AC-5, AC-6, AC-7
- files: runtime/action.yml, tests/ci_runtime.rs
- budget: 150
- depends_on: T-2
- exit: test tests/ci_runtime.rs::the_action_builds_verifies_and_commits_the_bundle

### T-4 A real pull request
- criteria: AC-8
- files: docs/decisions/0002-github-actions-runtime.md
- budget: 20
- depends_on: T-3
- exit: human a maintainer runs the action on a pull request in daneb/keel-cover-demo and sees keel cover report covered on the bundle commit
