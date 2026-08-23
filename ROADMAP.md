# Roadmap

What is built, what is deferred and why, and what is deliberately not planned.
The design is [PLAN.md](PLAN.md); the decisions are in
[`.keel/store/decisions/`](.keel/store/decisions/ADR-0000-index.md).

Updated 2026-08-23.

## Built

All five phases of PLAN.md. Each was designed to be a complete daily driver on
its own, and each is.

| Phase | Sufficiency contract | Evidence it holds |
| --- | --- | --- |
| **0** Store and map | Every session starts from the same current, budget-bounded picture | 1.25 s for 5,705 files; drift caught and refused |
| **1** Spec → Plan → Tasks, G0/G1 | Every criterion falsifiable; blast radius computed, not guessed | 3 specs through both gates |
| **2** Execution, evidence, G2/G2.5/G3 | A verdict with evidence an auditor could read | `keel export --verify` |
| **3** Failure classification, G4 | Recurring mistakes become gate checks | 3 lessons, 2 enforced |
| **4** Retrieval service | Agents work from symbols, and the drop is measured | 14.6× at 100% recall |
| **5** Breadth | New tools, checks and repos plug in without touching the spine | 4 drivers, 0 schema changes |

**359 tests · 0 clippy warnings · macOS.**

## Deferred

Each of these is deferred for a reason, not forgotten. The reason is the useful
part.

### Shared stores fetched over git

`[[shared]]` takes a filesystem path — a sibling checkout, a **git submodule**,
or a vendored copy. Cross-repo governance therefore already works over git; what
is missing is keel doing the fetching and pinning itself.

*Deferred because* a submodule pin is visible in your repository and reviewed
like any other change, which is the property a bank wants anyway. Build the
fetcher only if submodules prove awkward across many repositories.

### Answer quality, as distinct from recall

`keel bench` measures **recall** — did retrieval surface the files a correct
answer needs (currently 100%). It does not measure whether a model then answers
correctly, which needs a task set with known-good answers.

*Deferred because* keel never prevents reading: `keel source` pulls any body, and
outline/symbol are a cheap first step rather than a ceiling. The failure mode is
an extra retrieval call, not a wrong answer. The published 83%-vs-92% quality gap
describes indexed RAG *without* a fallback; keel always has one.

### The benchmark's task set rots

Tasks name symbols by hand, so a rename makes one silently read as bad
retrieval. `keel bench` now fails loudly when a task falls through to ripgrep —
but somebody still has to fix the task. Caught exactly this way when
`store_hash` became `store_hash_with_shared`.

*Deferred because* the tension is real: [ADR-0012](.keel/store/decisions/ADR-0012-bench-measures-cost-not-quality.md)
fixed the task set precisely so it could not be tuned until it flattered. Failing
loudly keeps both properties at the cost of occasional maintenance.

### Drivers for other agents

`claude-code`, `codex`, `copilot` and `kiro` ship. Copilot and Kiro are verified
against the real CLIs — 6/6 conformance and a real task end to end.

*Deferred because* `keel driver check` makes adding one cheap to validate, so the
next driver is written when somebody needs that agent. `pi` deliberately not
written.

### Human effort, as distinct from elapsed time

`keel metrics` reports wall-clock from run start to human decision, and how many
finished runs asked for a person and never got one.

*Deferred because* a real effort number needs the operator to record their own
time, which keel has no way to ask for and no business assuming. Elapsed and
honest beats precise-looking and wrong.

### One human oracle has never been exercised

`evidence-bundle` AC-6 — "a reviewer who did not run the work states what
changed" — is legal, counted, and still awaiting an actual reviewer.

*Deferred because* it is discharged by a person reading a bundle, not by code.

### Lesson count is thin

Three in force against the ≥5 the Phase 3 exit criterion wants. Four were
promoted from real data and one was demoted.

*Deferred because* it accrues with use and
[must not be manufactured](.keel/store/decisions/ADR-0006-promotion-needs-distinct-runs.md).

## Not planned

- **Windows.** A macOS tool for a macOS user. The `cfg(windows)` branches stay
  compiled and unexercised — a decision, not an omission.
- **Reimplementing what the agent already does.** Model adapters, tool
  registries, inference loops. PLAN.md §1: drivers stay thin, and keel is a
  conductor.
- **Automatic lesson promotion without human sign-off.** G4 forces the decision;
  it does not make it.

## Known weak spots

Worth saying plainly before trusting this on real work.

- **G2's green path is under-exercised.** 11% pass across 18 runs, because keel
  was developed inside keel and nearly every run exceeded its own declared
  scope. On a real repository with a real spec that inverts — but the red path
  has far more evidence behind it than the green one.
- **`G2/store-drift` has never failed** in 12+ runs, and keel's own metrics flag
  it as possible gate theatre. Probably correctly always-true here; it is the
  check to scrutinise first.
- **Single operator, single repository, one session.** Every number above is
  from keel measuring itself. The failure taxonomy's 64 episodes are from
  induced faults, not a quarter of unplanned work.

## Spine freeze

Versioned and additive-only from Phase 3 onward. Changing one is a design
review, not a patch.

`keel.gate/1` · `keel.spec/1` · `keel.plan/1` · `keel.tasks/1` · `keel.lesson/1`
`keel.run/1` · `keel.manifest/1` · `keel.drivertask/1` · `keel.driverresult/1`
`keel.reviewrequest/1` · `keel.reviewresult/1` · `keel.approval/1`
`keel.baseline/1` · `keel.adr/1` · `keel.index/2` · trajectory events
