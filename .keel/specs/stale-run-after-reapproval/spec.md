---
id: SPEC-0023
slug: stale-run-after-reapproval
schema: keel.spec/1
status: approved
scope:
- src/pipeline.rs
- src/cmd/next.rs
- tests/pipeline.rs
- tests/json.rs
budget:
  criteria: 5
  lines: 250
---

# A run that predates the current approvals does not satisfy the Run stage

## Context

`pipeline::passing_run` returns the newest run of a spec whose G2 passed. It
never looks at when the spec or plan was last approved. So after a spec is
widened and re-approved, then its plan is re-approved, the old run still counts:
`keel next` jumps to "approve the merge" for a change that was never rebuilt,
and `moor go` reports "nothing to run" because keel says the Run stage is done.
This happened on `adrs-for-recent-specs`, where run 007 passed, the scope was
then widened, and the pipeline skipped the rebuild. The agent had to be asked
for the new task by hand and G2 run outside the pipeline.

`g1_stale` already applies the same idea one stage earlier: a G1 pass older than
the current spec approval no longer counts. This spec extends that rule to the
run, and makes `keel next --json` say so, so a consumer such as moor reads it
instead of guessing.

## Acceptance criteria

### AC-1 A run older than the plan approval does not satisfy the Run stage

WHEN the newest passing run of a spec started before the current plan approval
THE SYSTEM SHALL report the spec's stage as `run`, not `merge-approval`.

oracle: test tests/pipeline.rs::a_run_older_than_the_plan_approval_does_not_satisfy_the_run_stage

### AC-2 A run older than the spec approval does not satisfy the Run stage

WHEN the newest passing run of a spec started before the current spec approval
THE SYSTEM SHALL report the spec's stage as `run`, not `merge-approval`.

oracle: test tests/pipeline.rs::a_run_older_than_the_spec_approval_does_not_satisfy_the_run_stage

### AC-3 A run that started after both approvals still counts

WHEN the newest passing run of a spec started after both the current spec
approval and the current plan approval THE SYSTEM SHALL report the spec's stage
as `merge-approval`.

oracle: test tests/pipeline.rs::a_run_newer_than_both_approvals_still_satisfies_the_run_stage

### AC-4 `keel next --json` names the stale run

WHEN a spec has a passing run that predates its current approvals THE SYSTEM
SHALL include `"stale_run": "<run id>"` in that spec's entry of `keel next --json`
and leave the `keel.next/1` schema string unchanged.

oracle: test tests/json.rs::next_json_names_a_run_that_predates_the_approvals

### AC-5 `keel next` says why the run is needed again

WHEN a spec has a passing run that predates its current approvals THE SYSTEM
SHALL print a `keel next` line that names the stale run and the command
`keel run <slug>`.

oracle: test tests/pipeline.rs::keel_next_explains_a_stale_run

## Out of scope

- Whether the delivered diff matches what the merge approval covered. That is a
  separate gap and needs its own decision.
- Changing how abandoned specs report `complete`. That is pinned by an existing
  test and consumers already filter on `stage`.
- Any change to moor.
