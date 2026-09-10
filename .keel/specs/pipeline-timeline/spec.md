---
id: SPEC-0006
slug: pipeline-timeline
schema: keel.spec/1
status: approved
scope:
- assets/ui/app.js
- assets/ui/app.css
- assets/ui/index.html
- tests/serve.rs
budget:
  criteria: 6
  lines: 300
verified_at: 2026-09-10
---

# Pipeline timeline visualization

## Context

`keel serve`'s detail view already renders `#spine` (`assets/ui/app.js`,
`renderSpine`) as one badge per pipeline stage, styled `.node` in
`assets/ui/app.css`. It answers "which stage is this spec at" but nothing
more: the badges are not connected, so the sequence does not read as a
journey, and clicking a badge does nothing — an operator who wants to know
*when* a stage was approved, *who* approved it, or *why* a gate failed has to
switch to the Checks tab and search, rather than seeing it at the step itself.

This change turns the badge row into a connected, clickable timeline: stages
read left-to-right as a sequence rather than a scatter of labels, and each
stage discloses its own detail — gate verdict and check counts, or approval
standing (by/at, or why it is absent, rejected or superseded) — in place. All
of that data already reaches the browser today in `/api/overview`'s
`SpecReport` (`gates`, `approvals`); nothing on the Rust side changes.

## Acceptance criteria

### AC-1 Stage nodes are connected by a visible track

WHEN the detail view renders a spec's pipeline THE SYSTEM SHALL connect each
`.node` in `#spine` to its neighbour with a visible connector, so the row
reads as one continuous track rather than separate badges.

oracle: cmd `cargo test --test serve timeline_nodes_are_connected_by_a_track` exit 0

### AC-2 A clicked stage node discloses its own detail

WHEN an operator clicks a stage node in `#spine` THE SYSTEM SHALL render that
stage's detail into `#timeline-detail`: the matching gate's verdict and check
counts for a gate stage, or the matching approval's standing for an approval
stage.

oracle: cmd `cargo test --test serve clicking_a_stage_node_renders_its_detail` exit 0

### AC-3 A second click on the same node collapses its detail

WHEN an operator clicks the stage node whose detail is already open THE
SYSTEM SHALL hide `#timeline-detail` rather than re-rendering the same
content.

oracle: cmd `cargo test --test serve clicking_the_open_node_again_collapses_it` exit 0

### AC-4 Opening a different node's detail replaces the previous one

WHEN an operator clicks a stage node while a different node's detail is open
THE SYSTEM SHALL replace `#timeline-detail`'s content with the newly clicked
stage's detail, so exactly one stage's detail is shown at a time.

oracle: cmd `cargo test --test serve opening_another_node_replaces_the_open_detail` exit 0

### AC-5 The timeline stacks vertically on a narrow viewport

WHERE the viewport is narrower than 700px THE SYSTEM SHALL lay `#spine` out
as a vertical column instead of a horizontal row, matching the breakpoint
`assets/ui/app.css` already uses to stack `#rail` on narrow screens.

oracle: cmd `cargo test --test serve the_timeline_stacks_vertically_under_700px` exit 0

### AC-6 The timeline stays inside the no-innerHTML and no-network rules

WHEN the timeline is built THE SYSTEM SHALL do so without `.innerHTML`,
`.outerHTML` or `insertAdjacentHTML`, and without any request to a network
origin other than this page's own.

oracle: cmd `cargo test --test serve the_page_never_assigns_disk_content_to_inner_html && cargo test --test serve the_page_loads_nothing_from_the_network` exit 0

## Out of scope

- Any change to the Rust-side `Report`/`SpecReport` JSON shape — every field
  the timeline needs (`gates`, `approvals`, `stage`) is already served.
- A run-level timeline (G2/G2.5/G3/G4 across multiple attempts); this covers
  only the spec-level spine (`spec` through `complete`).
- Touching the Overview landing page's own layout.
