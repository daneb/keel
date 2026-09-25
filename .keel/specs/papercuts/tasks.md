---
id: TASKS-0013
slug: papercuts
schema: keel.tasks/1
---

# Tasks

### T-1 test-movement can be reviewed
- criteria: AC-1, AC-2, AC-3
- files: src/gate/g25.rs, tests/papercuts.rs
- budget: 130
- exit: test tests/papercuts.rs::a_new_change_supersedes_the_review

### T-2 Blocked bundles are called blocked
- criteria: AC-4
- files: src/cover/mod.rs, tests/papercuts.rs
- budget: 40
- depends_on: T-1
- exit: test tests/papercuts.rs::a_chainless_bundle_is_reported_blocked

### T-3 Driver scripts pass Shellcheck
- criteria: AC-5
- files: assets/drivers/claude-code, assets/drivers/codex, assets/drivers/copilot, assets/drivers/kiro
- budget: 10
- exit: cmd `shellcheck assets/drivers/claude-code assets/drivers/codex assets/drivers/copilot assets/drivers/kiro assets/drivers/_common.sh` exit 0
