---
id: TASKS-0008
slug: spec-summary
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 A summary sits between the timeline and the tabs
- criteria: AC-1
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 36
- exit: `cargo test --test serve a_summary_section_sits_between_the_timeline_and_the_tabs` exits 0

### T-2 The summary totals tokens and events across every run
- depends_on: T-1
- criteria: AC-2
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 36
- exit: `cargo test --test serve the_summary_totals_tokens_and_events_across_every_run` exits 0

### T-3 The summary counts currently-open fails, not historical ones
- depends_on: T-2
- criteria: AC-3
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 36
- exit: `cargo test --test serve the_summary_counts_currently_open_fails_not_every_historical_one` exits 0

### T-4 Every gate the spec has ever produced a result for gets a badge
- depends_on: T-3
- criteria: AC-4
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 36
- exit: `cargo test --test serve every_exercised_gate_gets_a_badge_at_its_latest_verdict` exits 0

### T-5 The tabs remain where the drill-down detail lives
- depends_on: T-4
- criteria: AC-5
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 36
- exit: `cargo test --test serve the_summary_does_not_replace_the_checks_and_evidence_tabs` exits 0

### T-6 The summary stays inside the no-innerHTML and no-network rules
- depends_on: T-5
- criteria: AC-6
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 36
- exit: `cargo test --test serve the_page_never_assigns_disk_content_to_inner_html && cargo test --test serve the_page_loads_nothing_from_the_network` exits 0

