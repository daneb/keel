---
id: TASKS-0015
slug: runtime-fixes
schema: keel.tasks/1
---

# Tasks

### T-1 gVisor from a pinned, verified tarball
- criteria: AC-1, AC-2
- files: runtime/action.yml, tests/runtime_fixes.rs
- budget: 70
- exit: test tests/runtime_fixes.rs::gvisor_comes_from_a_pinned_verified_tarball

### T-2 Skip the runtime's own bundle commit
- criteria: AC-3
- files: runtime/action.yml, tests/runtime_fixes.rs
- budget: 60
- depends_on: T-1
- exit: test tests/runtime_fixes.rs::the_action_skips_its_own_bundle_commit
