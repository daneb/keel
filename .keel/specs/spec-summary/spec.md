---
id: SPEC-0008
slug: spec-summary
schema: keel.spec/1
status: approved
scope:
- assets/ui/app.js
- assets/ui/app.css
- assets/ui/index.html
- tests/serve.rs
budget:
  criteria: 6
  lines: 220
verified_at: 2026-09-10
---

# A per-spec summary: tokens, events, fails, gates exercised

## Context

The detail view goes straight from the timeline (`#spine`) to the Checks and
Evidence tabs, which render every run's own events/tokens/failing-checks
line by line. There is nowhere that answers, at a glance, "how much has this
feature cost, how many gates has it exercised, and is anything currently
failing" — an operator has to read down through however many runs a spec has
accumulated to add that up themselves.

`Report`/`SpecReport` already carries everything this needs: `spec.gates`
(G0/G1, the spec's own), `spec.runs[].gates` (G2/G2.5/G3/G4 per attempt) and
`spec.runs[].events`/`.tokens`. No wire-format change.

## Acceptance criteria

### AC-1 A summary sits between the timeline and the tabs

WHEN the detail view renders a spec THE SYSTEM SHALL render a summary section
in `#spec-summary`, positioned after `#timeline-detail` and before `.tabs`.

oracle: cmd `cargo test --test serve a_summary_section_sits_between_the_timeline_and_the_tabs` exit 0

### AC-2 The summary totals tokens and events across every run

WHEN the summary renders THE SYSTEM SHALL show the sum of `tokens` and the
sum of `events` across every run in `spec.runs`.

oracle: cmd `cargo test --test serve the_summary_totals_tokens_and_events_across_every_run` exit 0

### AC-3 The summary counts currently-open fails, not historical ones

WHEN the summary renders THE SYSTEM SHALL count failing checks from
`spec.gates` (G0/G1) plus the latest run's own gates only, not every
run's history, so a spec that is passing now does not read as failing
because an earlier attempt once failed.

oracle: cmd `cargo test --test serve the_summary_counts_currently_open_fails_not_every_historical_one` exit 0

### AC-4 Every gate the spec has ever produced a result for gets a badge

WHEN the summary renders THE SYSTEM SHALL show one badge per distinct gate id
found across `spec.gates` and every run's `gates`, coloured by that gate's
latest produced verdict.

oracle: cmd `cargo test --test serve every_exercised_gate_gets_a_badge_at_its_latest_verdict` exit 0

### AC-5 The tabs remain where the drill-down detail lives

WHEN an operator wants a specific check, event or evidence file THE SYSTEM
SHALL keep that detail behind the existing Checks/Evidence tabs, unchanged in
position relative to the new summary.

oracle: cmd `cargo test --test serve the_summary_does_not_replace_the_checks_and_evidence_tabs` exit 0

### AC-6 The summary stays inside the no-innerHTML and no-network rules

WHEN this change is built THE SYSTEM SHALL do so without `.innerHTML`,
`.outerHTML` or `insertAdjacentHTML`, and without any request to a network
origin other than this page's own.

oracle: cmd `cargo test --test serve the_page_never_assigns_disk_content_to_inner_html && cargo test --test serve the_page_loads_nothing_from_the_network` exit 0

## Out of scope

- Per-run summaries (the Evidence/Checks tabs already break runs out
  individually) — this is one summary for the whole spec.
- Any change to `Report`/`SpecReport`'s JSON shape.
- Reformatting the Checks/Evidence panels themselves.
