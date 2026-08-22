---
id: TASKS-0002
slug: agent-driver
schema: keel.tasks/1
---

# Tasks

### T-1 Driver contract types and validation
- criteria: AC-1, AC-3
- files: src/driver/mod.rs, src/driver/contract.rs
- budget: 60
- exit: `cargo test --test driver` exits 0; an invalid result names its field

### T-2 Subprocess execution, timeout and blocked handling
- criteria: AC-2, AC-6
- files: src/driver/mod.rs
- budget: 55
- exit: an unstartable driver and a timed-out driver both record `blocked`

### T-3 Bundled claude-code driver and trajectory wiring
- criteria: AC-4, AC-5
- files: src/driver/claude_code.rs, src/cmd/run.rs
- budget: 45
- exit: a claude-code round trip appends driver_call and driver_result events
