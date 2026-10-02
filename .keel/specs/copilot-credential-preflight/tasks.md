---
id: TASKS-0010
slug: copilot-credential-preflight
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 Add the credential preflight helper and reject a classic PAT
- criteria: AC-1
- files: scope
- budget: 35
- exit: `COPILOT_GITHUB_TOKEN=ghp_x .keel/drivers/copilot < /dev/null 2>&1 | grep -qi blocked` exits 0

### T-2 A rejected or missing credential names copilot /login as the fix
- criteria: AC-2
- files: scope
- budget: 10
- exit: `.keel/drivers/copilot < /dev/null 2>&1 | grep -q "copilot /login"` exits 0

### T-3 A missing credential blocks before invoking the agent
- criteria: AC-3
- files: scope
- budget: 10
- exit: `env -u COPILOT_GITHUB_TOKEN -u GH_TOKEN -u GITHUB_TOKEN .keel/drivers/copilot < /dev/null 2>&1 | grep -qi blocked` exits 0

### T-4 A device-flow (gho_) token passes preflight and runs the agent
- criteria: AC-4
- files: scope
- budget: 10
- exit: with a real gho_ token set, a reviewer confirms the driver reaches `copilot -p` rather than returning from preflight

### T-5 Enterprise data-residency host is honoured
- criteria: AC-5
- files: scope
- budget: 10
- exit: a reviewer confirms against the driver source that COPILOT_GH_HOST/GH_HOST reach the Copilot CLI unaltered by the preflight

### T-6 Preflight runs before any worktree is created
- criteria: AC-6
- files: scope
- budget: 10
- exit: a reviewer confirms the preflight is called at the top of the driver, before any `.keel/worktrees/` entry is created, so a no-credential --waves build adds no worktree

