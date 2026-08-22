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

**Not recorded here.** Decisions the plan already fixes (EARS, the gate pipeline
shape, the four attribution codes) are PLAN.md's, not keel's. An ADR that only
restates the plan is noise.
