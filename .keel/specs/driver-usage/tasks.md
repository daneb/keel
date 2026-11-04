---
id: TASKS-0019
slug: driver-usage
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 The driver result accepts a usage object
- criteria: AC-1
- files: src/driver/contract.rs, src/driver/conform.rs, tests/driver_usage.rs
- budget: 50
- exit: `cargo test --test driver_usage driver_result_parses_a_usage_object_and_stays_backward_compatible` exits 0

### T-2 The claude-code driver reports the CLI's own usage
- criteria: AC-2
- files: assets/drivers/**, tests/driver_usage.rs
- depends_on: T-1
- budget: 45
- exit: `cargo test --test driver_usage claude_code_driver_maps_cli_json_usage_into_the_result` exits 0

### T-3 Every driver call records a usage event
- criteria: AC-3
- files: src/trajectory/event.rs, src/trajectory/mod.rs, src/cmd/run.rs, tests/driver_usage.rs
- depends_on: T-1, T-2
- budget: 55
- exit: `cargo test --test driver_usage each_driver_call_appends_a_driver_usage_event` exits 0

### T-4 Missing usage is unreported, never zero
- criteria: AC-4
- files: src/trajectory/event.rs, src/metrics.rs, src/cmd/metrics.rs, tests/driver_usage.rs
- depends_on: T-1, T-3
- budget: 50
- exit: `cargo test --test driver_usage a_driver_that_reports_nothing_is_counted_as_unreported_not_as_zero` exits 0

### T-5 Injected and spent are reported apart
- criteria: AC-5
- files: src/metrics.rs, src/cmd/metrics.rs, src/report/insights.rs, tests/driver_usage.rs
- depends_on: T-1, T-4
- budget: 50
- exit: `cargo test --test driver_usage metrics_prints_injected_and_spent_as_separate_never_summed_figures` exits 0

### T-6 A spec's cost includes the runs that failed
- criteria: AC-6
- files: src/report/insights.rs, src/metrics.rs, tests/driver_usage.rs
- depends_on: T-1, T-5
- budget: 50
- exit: `cargo test --test driver_usage spec_cost_totals_every_run_and_shows_the_share_spent_on_red_runs` exits 0

### T-7 A price table fills in cost only as an estimate
- criteria: AC-7
- files: src/config.rs, src/metrics.rs, tests/driver_usage.rs
- depends_on: T-1, T-6
- budget: 50
- exit: `cargo test --test driver_usage price_table_cost_is_labelled_estimated_and_never_overrides_a_reported_cost` exits 0

### T-8 Reviewer spend is recorded against G2.5
- criteria: AC-8
- files: src/review/mod.rs, tests/driver_usage.rs
- depends_on: T-1, T-7
- budget: 50
- exit: `cargo test --test driver_usage reviewer_usage_is_recorded_against_g2_5` exits 0
