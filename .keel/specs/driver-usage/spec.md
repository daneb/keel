---
id: SPEC-0019
slug: driver-usage
schema: keel.spec/1
status: approved
scope:
- src/driver/contract.rs
- src/driver/conform.rs
- assets/drivers/**
- src/trajectory/event.rs
- src/trajectory/mod.rs
- src/cmd/run.rs
- src/review/**
- src/config.rs
- src/metrics.rs
- src/cmd/metrics.rs
- src/report/insights.rs
- tests/driver_usage.rs
budget:
  criteria: 8
  lines: 450
verified_at: 2026-11-03
---

# Record what the agent actually spent, not only what keel injected

## Context

`keel metrics` reports `tokens_total` and `tokens_per_run`, but every number
behind them comes from `estimate_tokens()` over what keel itself put in front
of the model: `Inject` events and the `DriverCall` prompt. That is the
smallest part of a run's cost. A headless agent reads files, calls tools,
retries and writes output, and none of that is visible to keel.

`keel.driverresult/1` already carries an optional `tokens` field, and
`contract.rs` parses it, but nothing records it. The `claude-code` driver
runs `claude --print`, which returns plain text, so even the tool's own usage
report is discarded.

This spec makes cost a measured number in the same spirit as ADR-0012 and
ADR-0020: what is reported is labelled reported, what is estimated is
labelled estimated, and the two are never added together.

## Acceptance criteria

### AC-1 The driver result accepts a usage object

WHEN a driver emits a `keel.driverresult/1` document with a `usage` object
holding `input_tokens`, `output_tokens`, `cache_read_tokens`,
`cache_write_tokens` and `cost_usd` THE SYSTEM SHALL parse it, and a driver
result without `usage` SHALL still pass `keel driver check`.

oracle: cmd `cargo test --test driver_usage driver_result_parses_a_usage_object_and_stays_backward_compatible` exit 0

### AC-2 The claude-code driver reports the CLI's own usage

WHEN the `claude-code` driver runs a task THE SYSTEM SHALL invoke
`claude --print --output-format json` and copy the CLI's reported token
counts and `total_cost_usd` into `usage`, while the agent's text output is
still written to the run record as before.

oracle: cmd `cargo test --test driver_usage claude_code_driver_maps_cli_json_usage_into_the_result` exit 0

### AC-3 Every driver call records a usage event

WHEN a driver result arrives during `keel run` THE SYSTEM SHALL append a
`driver_usage` event to `trajectory.jsonl` carrying the driver id, the task
id, the reported counts and `source: reported`.

oracle: cmd `cargo test --test driver_usage each_driver_call_appends_a_driver_usage_event` exit 0

### AC-4 Missing usage is unreported, never zero

IF a driver result carries no `usage` THEN THE SYSTEM SHALL record the call
with `source: unreported` and exclude it from spent-token totals, and
`keel metrics` SHALL print the count of unreported calls next to the total.

oracle: cmd `cargo test --test driver_usage a_driver_that_reports_nothing_is_counted_as_unreported_not_as_zero` exit 0

### AC-5 Injected and spent are reported apart

WHEN `keel metrics` or `keel report` prints token figures THE SYSTEM SHALL
print `injected (estimated by keel)` and `spent (reported by the driver)` as
two separate lines and SHALL never print their sum.

oracle: cmd `cargo test --test driver_usage metrics_prints_injected_and_spent_as_separate_never_summed_figures` exit 0

### AC-6 A spec's cost includes the runs that failed

WHEN `keel report` summarises a spec THE SYSTEM SHALL total spent tokens and
cost across every run of that spec, and SHALL show the share spent on runs
whose final G2 verdict was `fail` or `blocked`.

oracle: cmd `cargo test --test driver_usage spec_cost_totals_every_run_and_shows_the_share_spent_on_red_runs` exit 0

### AC-7 A price table fills in cost only as an estimate

WHERE `[[price]]` entries map a driver to per-million-token rates and a
driver reports tokens without `cost_usd` THE SYSTEM SHALL compute the cost,
label it `estimated from price table dated <date>` using the entry's
`as_of` field, and keep driver-reported costs unchanged.

oracle: cmd `cargo test --test driver_usage price_table_cost_is_labelled_estimated_and_never_overrides_a_reported_cost` exit 0

### AC-8 Reviewer spend is recorded against G2.5

WHEN a reviewer's `keel.reviewresult/1` carries a `usage` object THE SYSTEM
SHALL record it as a `driver_usage` event attributed to gate `G2.5`, so a
model-backed reviewer's cost appears beside the agent's.

oracle: cmd `cargo test --test driver_usage reviewer_usage_is_recorded_against_g2_5` exit 0

## Out of scope

- A token or cost budget that blocks a run. Measure first; a ceiling under
  ADR-0004 is a separate spec once there is data to set it from.
- Usage for `copilot` and `kiro`. Their drivers emit no `usage` until their
  CLIs expose one, which AC-4 makes visible rather than silent.
- `codex` usage. Its JSON event stream is the obvious source, but the field
  names have to be confirmed against the real CLI under `keel driver check`
  before a criterion can name them.
- Changing `keel bench`, which measures retrieval cost by design (ADR-0012).

## Decision to record

An ADR, "Spent tokens are reported, injected tokens are estimated, and the two
are never summed", alongside ADR-0012 and ADR-0020.
