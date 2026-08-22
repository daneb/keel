---
id: TASKS-0001
slug: trajectory-log
schema: keel.tasks/1
---

# Tasks

### T-1 Event model and append-only writer
- criteria: AC-1, AC-2, AC-3
- files: src/trajectory/mod.rs, src/trajectory/event.rs
- budget: 70
- exit: `cargo test --test trajectory` exits 0 with the three writer tests green

### T-2 Gate and injection events
- criteria: AC-4, AC-5
- depends_on: T-1
- files: src/trajectory/mod.rs
- budget: 40
- exit: a G0 run appends a `gate` event and `keel replay` shows it

### T-3 Replay and corrupt-line handling
- criteria: AC-6, AC-7
- depends_on: T-1
- files: src/cmd/run.rs, src/trajectory/mod.rs
- budget: 45
- exit: `keel replay` prints events in seq order; a corrupt line exits non-zero naming the line
