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

- [x] **A second driver, and a way to check any driver.** `codex` is written
      against a deliberately different CLI shape (argv positional, not stdin) and
      the contract did not move. `keel driver check` runs any driver through a
      conformance suite in a scratch repository. It found a real bug on its first
      run: relative adapter paths were resolved against the working directory
      rather than the configuring repository.
- [x] **Task dependency waves.** `depends_on` in `tasks.md`, waves computed
      topologically, cycles and dangling dependencies failing G1, `keel tasks`
      showing the order. Execution stays serial and says so.
- [x] **Cross-repo store.** `[[shared]]` layers another repository's store
      underneath this one — conventions above local, lessons enforced and
      injected, local shadowing by id. A missing required store fails doctor, G0
      and G2 and is stated in the projection.
- [x] **Metrics surface.** `keel metrics` — gate pass rates, failure classes,
      attribution, tokens per run, lesson fires, and the checks that have never
      failed in N runs.
- [x] **`keel doctor`.** Index, projections, verify config, drivers, lessons,
      specs, runs and shared stores, each finding naming the command that fixes
      it.

### Still open in Phase 5

- [ ] **Parallel execution of a wave.** Waves are computed and reported; keel
      runs tasks serially. Real parallelism needs a git worktree per task and a
      merge strategy, neither of which exists.
- [ ] **Drivers for Kiro, Copilot and `pi`.** The conformance suite makes adding
      one cheap to validate; nobody has written them.
- [ ] **Shared stores over git.** `[[shared]]` takes a path — a sibling
      checkout, a submodule, a vendored copy. Fetching and pinning a remote
      store is not built.
- [ ] **Human-intervention minutes per task.** The one metric from PLAN.md §5
      not surfaced: keel records human decisions but not the time they cost.

## Debts

Taken deliberately, recorded so they are not rediscovered as surprises.

### Paid

- [x] **G2.5 is heuristic.** A reviewer now runs as a subprocess in critique
      mode (`keel.reviewrequest/1` → `keel.reviewresult/1`), receiving the diff,
      the conventions, the lessons in force and the criteria. Each finding
      becomes its own gate check with its location. `advisory = true` downgrades
      every finding to a look while you learn whether to trust a reviewer. The
      substring heuristics stay: a reviewer that is not configured must not
      remove the only check there was, and one that cannot run must not pass one.
- [x] **Runs accumulate.** `keel runs --prune --keep N` reports, `--apply`
      removes. Runs cited by a lesson's `sources:` — including demoted lessons —
      are never pruned, because a dangling provenance id is worse than the bytes.
- [x] **`doctest` and `schema` oracle kinds were never exercised.** Both now
      have end-to-end tests that prove they pass on a good input and *fail* on a
      bad one, which is the half that matters.
- [x] **Retrieval quality was entirely unmeasured.** `keel bench` now reports
      recall alongside the ratio: whether the cheap answer named the files a
      correct answer needs. Currently 100% at 14.1×. This is narrow and the
      output says so — it does not measure whether a model would then answer
      correctly.

### Outstanding

- [ ] **Answer quality proper.** Recall says retrieval surfaced the right files.
      Whether a model answers correctly from them needs a task set with
      known-good answers ([ADR-0012](.keel/store/decisions/ADR-0012-bench-measures-cost-not-quality.md)).
- [ ] **One human oracle has never been exercised.** `evidence-bundle` AC-6
      ("a reviewer who did not run the work states what changed") is legal,
      counted, and still awaiting an actual reviewer.
- [ ] **Windows is compiled, not tested.** Process-group kill, `taskkill` and
      the `cmd /C` shell paths are `cfg`'d and unexercised.
- [ ] **Lesson count is thin.** 3 in force against the ≥5 the Phase 3 exit
      criterion wants. The number accrues with use and should not be
      manufactured ([ADR-0006](.keel/store/decisions/ADR-0006-promotion-needs-distinct-runs.md)).
- [ ] **ADRs are not projected.** They live in `.keel/store/decisions/` and are
      deliberately outside the projection budget, so an agent will not see them
      unless it looks. `keel status` reports the count; nothing injects them.

## Spine freeze

From Phase 3 onward these are versioned and additive-only. Changing one is a
design review, not a patch.

`keel.gate/1` · `keel.spec/1` · `keel.plan/1` · `keel.tasks/1` · `keel.lesson/1`
`keel.run/1` · `keel.manifest/1` · `keel.drivertask/1` · `keel.driverresult/1`
`keel.approval/1` · `keel.baseline/1` · `keel.index/2` · trajectory events
