---
id: TASKS-0006
slug: pipeline-timeline
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 Stage nodes are connected by a visible track
- criteria: AC-1
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 41
- exit: `cargo test --test serve timeline_nodes_are_connected_by_a_track` exits 0

### T-2 A clicked stage node discloses its own detail
- depends_on: T-1
- criteria: AC-2
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 41
- exit: `cargo test --test serve clicking_a_stage_node_renders_its_detail` exits 0

### T-3 A second click on the same node collapses its detail
- depends_on: T-2
- criteria: AC-3
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 41
- exit: `cargo test --test serve clicking_the_open_node_again_collapses_it` exits 0

### T-4 Opening a different node's detail replaces the previous one
- depends_on: T-3
- criteria: AC-4
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 41
- exit: `cargo test --test serve opening_another_node_replaces_the_open_detail` exits 0

### T-5 The timeline stacks vertically on a narrow viewport
- depends_on: T-4
- criteria: AC-5
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 41
- exit: `cargo test --test serve the_timeline_stacks_vertically_under_700px` exits 0

### T-6 The timeline stays inside the no-innerHTML and no-network rules
- depends_on: T-5
- criteria: AC-6
- files: assets/ui/app.js, assets/ui/app.css, assets/ui/index.html, tests/serve.rs
- budget: 41
- exit: `cargo test --test serve the_page_never_assigns_disk_content_to_inner_html && cargo test --test serve the_page_loads_nothing_from_the_network` exits 0

