---
id: TASKS-0007
slug: timeline-clarity
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 A stage backed by exactly one gate names it consistently
- criteria: AC-1
- files: assets/ui/app.js, assets/ui/app.css, tests/serve.rs
- budget: 33
- exit: `cargo test --test serve gate_backed_stage_labels_name_their_gate_consistently` exits 0

### T-2 "Here" and "open" are visually distinct
- depends_on: T-1
- criteria: AC-2
- files: assets/ui/app.js, assets/ui/app.css, tests/serve.rs
- budget: 33
- exit: `cargo test --test serve here_and_expanded_are_styled_differently` exits 0

### T-3 A stage flagged done that the pipeline has since left is called out
- depends_on: T-2
- criteria: AC-3
- files: assets/ui/app.js, assets/ui/app.css, tests/serve.rs
- budget: 33
- exit: `cargo test --test serve a_current_merge_approval_past_the_pipelines_stage_is_flagged` exits 0

### T-4 The spine's default text and borders clear a higher contrast floor
- depends_on: T-3
- criteria: AC-4
- files: assets/ui/app.js, assets/ui/app.css, tests/serve.rs
- budget: 33
- exit: `cargo test --test serve pending_nodes_use_higher_contrast_tokens` exits 0

### T-5 The spine reads as the page's primary element, not an afterthought
- depends_on: T-4
- criteria: AC-5
- files: assets/ui/app.js, assets/ui/app.css, tests/serve.rs
- budget: 33
- exit: `cargo test --test serve the_spine_nodes_are_larger_than_before` exits 0

### T-6 The timeline stays inside the no-innerHTML and no-network rules
- depends_on: T-5
- criteria: AC-6
- files: assets/ui/app.js, assets/ui/app.css, tests/serve.rs
- budget: 33
- exit: `cargo test --test serve the_page_never_assigns_disk_content_to_inner_html && cargo test --test serve the_page_loads_nothing_from_the_network` exits 0

