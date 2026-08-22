# Roadmap

State of [PLAN.md](PLAN.md), and the debts taken along the way. Updated
2026-08-22.

## Done

| Phase | Sufficiency contract | Evidence |
| --- | --- | --- |
| **0** Store and map | Every AI session starts from the same, current, budget-bounded picture | `keel status`; 1.25s for 5,705 files |
| **1** Spec → Plan → Tasks, G0/G1 | Every criterion falsifiable; blast radius computed, not guessed | 3 specs through G0+G1 |
| **2** Execution, evidence, G2/G2.5/G3 | A verdict with evidence an auditor could read | `keel export --verify` |
| **3** Failure classification, G4 | Recurring mistakes become gate checks | 3 lessons, 2 enforced |
| **4** Retrieval service | Agents work from symbols, and the cost drop is measured | `keel bench` → 14.0× |

All three Phase-2/3 specs are `status: implemented` with their oracles passing.

## Phase 5 — breadth (ongoing, not a milestone)

- [ ] **Drivers beyond `claude-code`.** PLAN.md §6 says the driver interface
      should be proven against ≥2 tools *before* Phase 5 — it has been proven
      against one. Kiro, Copilot, Codex, `pi`. The contract is deliberately
      thin; if a second driver needs a contract change, that is the finding.
- [ ] **Task dependency waves.** Independent tasks in parallel, dependent ones
      sequential. `tasks.md` has no `depends_on` field yet.
- [ ] **Cross-repo store.** Shared conventions and lessons as a submodule or
      package, with local override precedence. This is the piece that scales to
      a portfolio and turns keel into a governance instrument.
- [ ] **Metrics surface.** Gate pass rates, failure-class distribution, lesson
      hit rate, tokens/task, human-intervention minutes/task. `keel failures`
      and `keel bench` are the first two; nothing aggregates across time.
- [ ] **`keel doctor`.** Health of index, drift, decayed lessons, orphaned specs.
      Today these are spread across `status`, `lessons` and `store check`.

## Debts

Taken deliberately, recorded so they are not rediscovered as surprises.

- [ ] **Answer quality is unmeasured.** `keel bench` measures token cost only,
      and says so ([ADR-0012](.keel/store/decisions/ADR-0012-bench-measures-cost-not-quality.md)).
      A quality number needs a task set with known-correct answers.
- [ ] **G2.5 is heuristic.** Added mocks and weakened assertions are caught by
      substring match; scope creep by the diff. A reviewer plugin — a driver run
      in critique mode — is the intended implementation and is not built.
- [ ] **One human oracle has never been exercised.** `evidence-bundle` AC-6
      ("a reviewer who did not run the work states what changed") is legal,
      counted, and still awaiting an actual reviewer.
- [ ] **`doctest` and `schema` oracle kinds are barely used.** Both parse and
      execute; no spec in this repo currently depends on either, so neither has
      been exercised in anger. (`schema` was used and then withdrawn — its
      target only exists inside an archive.)
- [ ] **Windows is compiled, not tested.** Process-group kill, `taskkill` and
      the `cmd /C` shell paths are `cfg`'d and unexercised.
- [ ] **Runs accumulate.** `.keel/runs/` grows without bound and nothing prunes
      or archives it. Lesson `sources:` point into it, so pruning must not be
      naive.
- [ ] **Lesson count is thin.** 3 in force against the ≥5 the Phase 3 exit
      criterion wants. Four were promoted from real data and one was demoted;
      the number accrues with use and should not be manufactured
      ([ADR-0006](.keel/store/decisions/ADR-0006-promotion-needs-distinct-runs.md)).
- [ ] **ADRs are not projected.** They live in `.keel/store/decisions/` and are
      deliberately outside the projection budget, so an agent will not see them
      unless it looks. `keel status` reports the count; nothing injects them.

## Spine freeze

From Phase 3 onward these are versioned and additive-only. Changing one is a
design review, not a patch.

`keel.gate/1` · `keel.spec/1` · `keel.plan/1` · `keel.tasks/1` · `keel.lesson/1`
`keel.run/1` · `keel.manifest/1` · `keel.drivertask/1` · `keel.driverresult/1`
`keel.approval/1` · `keel.baseline/1` · `keel.index/2` · trajectory events
