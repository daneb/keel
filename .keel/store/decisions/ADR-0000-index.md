---
id: ADR-0000
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
---

# Decision record index

Architecture decisions for keel, one file each. An ADR records a decision that
had a real alternative and a reason for not taking it — not a description of
what the code does, which the code already provides.

| id | phase | decision |
| --- | --- | --- |
| [ADR-0001](ADR-0001-two-hashes-per-projection.md) | 0 | Projections carry two hashes, not one |
| [ADR-0004](ADR-0004-budgets-are-hard-ceilings.md) | 0 | Budgets are ceilings that include their own truncation notice |
| [ADR-0011](ADR-0011-separate-budgets-for-curated-and-generated.md) | 0 | Curated steering and the generated map are budgeted separately |
| [ADR-0002](ADR-0002-blocked-is-a-verdict.md) | 1 | `blocked` is a first-class verdict with its own exit code |
| [ADR-0003](ADR-0003-approvals-bind-to-a-hash.md) | 1 | An approval records the hash of what was approved |
| [ADR-0009](ADR-0009-blast-radius-is-recomputed.md) | 1 | G1 recomputes the blast radius rather than trusting the plan |
| [ADR-0005](ADR-0005-attribution-before-class.md) | 3 | Attribution before class; `UNATTRIBUTABLE` is a real outcome |
| [ADR-0006](ADR-0006-promotion-needs-distinct-runs.md) | 3 | A lesson needs two occurrences in *distinct* runs |
| [ADR-0007](ADR-0007-enforced-lessons-are-not-injected.md) | 3 | A lesson with an oracle stops being a prompt |
| [ADR-0008](ADR-0008-usage-ledger-outside-the-store.md) | 3 | Lesson usage is recorded outside the store |
| [ADR-0010](ADR-0010-index-is-an-accelerator.md) | 4 | The index is an accelerator; degradation is labelled |
| [ADR-0012](ADR-0012-bench-measures-cost-not-quality.md) | 4 | `keel bench` measures cost, and says so |
| [ADR-0017](ADR-0017-the-reviewer-is-additive.md) | debts | The adversarial reviewer is additive, and can be advisory |
| [ADR-0018](ADR-0018-generated-projections-are-not-the-diff.md) | debts | Generated projections do not count against a blast radius |
| [ADR-0019](ADR-0019-pruning-never-orphans-provenance.md) | debts | Pruning never removes a run a lesson cites |
| [ADR-0013](ADR-0013-conformance-in-a-scratch-repo.md) | 5 | Driver conformance runs in a scratch git repository |
| [ADR-0014](ADR-0014-adapters-share-one-helper.md) | 5 | Adapters share one helper; a zero budget means no change expected |
| [ADR-0015](ADR-0015-local-shadows-shared.md) | 5 | Local shadows shared; a missing required store is loud |
| [ADR-0016](ADR-0016-waves-run-in-worktrees.md) | 5 | A wave runs one worktree per task; patches applied in sequence |
| [ADR-0020](ADR-0020-human-time-is-elapsed-not-effort.md) | 5 | Human-intervention time is elapsed, never effort |
| [ADR-0021](ADR-0021-review-stage-clears-a-blocked-check.md) | field | A fourth approval stage clears G2.5's blocked check, bound to the flagged lines |
| [ADR-0022](ADR-0022-no-driver-diffs-from-the-branch-point.md) | field | `--no-driver` diffs from where the branch left trunk, not from HEAD |

**Not recorded here.** Decisions the plan already fixes (EARS, the gate pipeline
shape, the four attribution codes) are PLAN.md's, not keel's. An ADR that only
restates the plan is noise.
