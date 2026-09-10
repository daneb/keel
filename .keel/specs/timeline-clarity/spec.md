---
id: SPEC-0007
slug: timeline-clarity
schema: keel.spec/1
status: approved
scope:
- assets/ui/app.js
- assets/ui/app.css
- tests/serve.rs
budget:
  criteria: 6
  lines: 200
verified_at: 2026-09-10
---

# Timeline clarity: labels, contrast, stale-approval flag

## Context

Real use of the pipeline timeline (`assets/ui/app.js`, `renderSpine`) against a
real project surfaced three problems:

1. `STAGES` labels most stages in plain English (`spec`, `plan`,
   `approve plan`, `done`) but two — `plan_gate` — is labelled bare `G1`, with
   no equivalent for the `spec` stage's own gate (`G0`). The inconsistency
   reads as arbitrary rather than informative.
2. A stage node that is "where the pipeline is now" (`.here`) and a stage
   node whose detail panel happens to be open (`[aria-expanded="true"]`)
   render with the *same* brass outline. When they land on two different
   nodes at once — routine, since clicking a node to inspect it does not move
   the pipeline — nothing distinguishes which highlight means what.
3. An approval can be `current` (its artefact hash still matches) while a
   *later* gate run regressed the spec away from `Stage::Complete` — recorded
   history and present reality disagreeing is exactly the situation
   `lock-completed-specs` (SPEC-0005) now prevents going forward, but data
   written before that fix still exists, and the timeline reports the stale
   approval as if nothing were wrong: `stageDetail`'s `complete` and
   `merge_approval` branches render "approved"/"completed" off the approval
   standing alone, with no reference to `spec.stage`.

Separately: the timeline's nodes are small (11px, 4px/9px padding) next to a
Checks panel that can run to hundreds of lines for a failing run, so the
timeline reads as an afterthought above the panel that actually dominates the
page, and its default (pending) text colour is close enough to its
background to be hard to read at a glance.

## Acceptance criteria

### AC-1 A stage backed by exactly one gate names it consistently

WHEN the spine renders the `spec` or `plan_gate` stage THE SYSTEM SHALL
include that stage's backing gate id in its label — `spec (G0)` and
`plan (G1)` — rather than naming the gate for only one of the two.

oracle: cmd `cargo test --test serve gate_backed_stage_labels_name_their_gate_consistently` exit 0

### AC-2 "Here" and "open" are visually distinct

WHEN a stage node is the pipeline's current stage (`.here`) and a different
stage node's detail is open (`[aria-expanded="true"]`) THE SYSTEM SHALL style
them with different visual treatments, so an operator can tell which node
carries which meaning without reading the label.

oracle: cmd `cargo test --test serve here_and_expanded_are_styled_differently` exit 0

### AC-3 A stage flagged done that the pipeline has since left is called out

WHEN `spec.stage` is not `complete` but the `merge` approval's standing is
`current` THE SYSTEM SHALL mark the `merge_approval` and `complete` nodes
with a distinct class in the spine and, when either is open, explain the
discrepancy in `#timeline-detail`.

oracle: cmd `cargo test --test serve a_current_merge_approval_past_the_pipelines_stage_is_flagged` exit 0

### AC-4 The spine's default text and borders clear a higher contrast floor

WHERE a stage node carries no status colour (not `.done`, `.bad`, `.here` or
`.stale`) THE SYSTEM SHALL render its text and border from `--ink`-mixed
tokens rather than plain `--muted`, so the pending state is legible at 11px
without relying on colour alone.

oracle: cmd `cargo test --test serve pending_nodes_use_higher_contrast_tokens` exit 0

### AC-5 The spine reads as the page's primary element, not an afterthought

WHEN the spine renders THE SYSTEM SHALL size its nodes larger than the
previous 11px/4px-9px treatment — at least 12px text and 6px/12px padding —
so it holds visual weight against a long Checks panel below it.

oracle: cmd `cargo test --test serve the_spine_nodes_are_larger_than_before` exit 0

### AC-6 The timeline stays inside the no-innerHTML and no-network rules

WHEN this change is built THE SYSTEM SHALL do so without `.innerHTML`,
`.outerHTML` or `insertAdjacentHTML`, and without any request to a network
origin other than this page's own.

oracle: cmd `cargo test --test serve the_page_never_assigns_disk_content_to_inner_html && cargo test --test serve the_page_loads_nothing_from_the_network` exit 0

## Out of scope

- Reformatting the Checks/Evidence panels themselves (long check detail
  lines, wrapping) — this only changes the spine's own visual weight.
- A global palette or contrast pass over the rest of the app; the contrast
  fix here is scoped to the spine's own default tokens.
- Any change to `Report`/`SpecReport`'s JSON shape.
