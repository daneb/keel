# keel

A gated harness for AI-assisted delivery. keel is a **conductor, not an agent
loop**: it owns the knowledge store, the structural map of your repository, and
the projections every coding agent reads — then tells you, loudly, when those
have drifted apart.

The full design lives in [PLAN.md](PLAN.md). This repository currently
implements **Phase 0 — Store and map**, **Phase 1 — Spec → Plan → Tasks with
G0/G1**, **Phase 2 — Execution, evidence and G2/G2.5/G3**, and **Phase 3 —
Failure classification and lesson promotion, G4**.

> **Phase 0** — *Every AI session in any tool starts from the same, current,
> budget-bounded picture of this repo — and I can tell when that picture has
> drifted.*
>
> **Phase 1** — *I can turn an idea into a spec whose every criterion is
> falsifiable, and a plan whose blast radius is computed rather than guessed —
> and I never hand a vague spec to an agent by accident.*
>
> **Phase 2** — *I can run a task to completion through a real agent CLI and get
> back a pass/fail verdict with attached evidence I could hand to an auditor —
> and nothing merges without it.*
>
> **Phase 3** — *Recurring mistakes stop recurring, because the second
> occurrence turns into a gate check rather than a paragraph.*

## Install

```bash
cargo install --path .
```

## Use

### The store (Phase 0)

```bash
keel init          # scaffold .keel/, seed steering, build the first map
keel map           # rebuild the symbol index and the generated maps
keel store render  # project the store into CLAUDE.md, AGENTS.md, Kiro, Copilot
keel store check   # report drift, staleness and budget breaches (exit 1 if any)
keel status        # is everything current?
keel hook install  # make a drifted projection block the commit
```

### The pipeline (Phase 1)

```bash
keel spec new <slug>    # scaffold a spec, then run G0 on it
keel gate g0 <slug>     # is this spec buildable?
keel approve <slug>     # record a human decision, against the artefact's hash
keel plan <slug>        # compute the blast radius, scaffold plan.md + tasks.md
keel gate g1 <slug>     # is this plan bounded?
keel blast src/api/**   # what else does this change touch?
```

### Execution and evidence (Phase 2)

```bash
keel run <slug>         # drive an agent, capture everything, run G2/G2.5/G3
keel run --no-driver    # gate the working tree as it stands
keel replay <run>       # the run's trajectory, in sequence order
keel runs               # what has been run
keel approve <slug> --stage merge   # the G3 decision
keel export <run>       # a single verifiable .tar.gz
keel export --verify <bundle>
keel ratchet            # metrics that may improve and must not regress
```

### Learning (Phase 3)

```bash
keel learn [run]        # extract failure episodes, classify, propose lessons
keel failures           # every episode, with the attribution distribution
keel gate g4            # has this run been learned from?
keel lesson promote <n> # accept a candidate (refused on a single occurrence)
keel lesson reject <n>
keel lessons            # what is in force, and what has decayed
keel lesson demote <id> # retire a decayed lesson, keeping why
```

## What it produces

```
.keel/
  keel.toml                       # budgets, adapters, exclusions
  store/
    steering/product.md           # curated — you write these
    steering/tech.md
    steering/conventions.md
    steering/structure.md         # generated, budget-fitted repository map
    map/index.sqlite              # symbol table, imports, call graph, ranks
    map/<dir>/CODEMAP.md          # generated per-directory detail
    lessons/                      # Phase 3
    inbox/                        # hand-edits captured out of projections

CLAUDE.md · AGENTS.md · .kiro/steering/keel.md · .github/copilot-instructions.md
```

The four files at the bottom are **outputs, not inputs**. Each carries a
provenance header with two hashes, which is what lets `keel store check`
distinguish the only two things that can go wrong:

| State | Meaning | Fix |
| --- | --- | --- |
| `stale` | body intact, store has moved on | `keel store render` |
| `DRIFT` | the file itself was hand-edited | `keel store reconcile <path>` |
| `foreign` | the file exists but keel never wrote it | reconcile, or disable the adapter |

`render` **refuses** to overwrite a drifted or foreign file. `reconcile` parks
the edit in `.keel/store/inbox/` for you to fold into steering, then restores
the projection. Nothing a human wrote is ever silently destroyed.

## Gates

A gate is a predicate over artefacts returning `pass | fail | blocked`, with the
evidence attached, written to `.keel/specs/<slug>/gates/G<n>.json` under the
versioned `keel.gate/1` schema.

**G0 — is this spec buildable?** Every criterion in EARS form; every criterion
names a machine-checkable oracle; no scaffold placeholders; no weasel words; the
scope and change budget are declared; the store has not drifted; and the spec is
inside its criterion and line ceilings.

**G1 — is this plan bounded?** Every task traces to a criterion and every
criterion to a task; every task has files, a line budget and an exit condition;
the rollback is stated; and the recorded blast radius still matches a fresh
computation from the map.

**G2 — is this implementation verified?** Build, test and lint green; every
criterion's oracle actually executed; the diff inside the declared scope; the
line budget respected; no baseline metric moved the wrong way.

**G2.5 — adversarial review.** Looks for the two things a green G2 cannot see:
mocks or weakened assertions *added* by this change, and code changed with no
test touched at all. Both report `blocked`, not `fail` — legitimate test doubles
exist, and a check that failed on every one would be routed around within a week.

**G3 — the human decision.** Earlier gates passed, the evidence is complete, the
diff is small enough that a person could genuinely review it, and a human verdict
is recorded against *these* artefacts.

`blocked` is a distinct verdict with its own exit code (3). It means the check
could not run — a missing tool, no index — and it never silently passes, and it
never counts as an agentic failure. A gate with no checks is `blocked`, not
`pass`: an empty gate is a misconfiguration, not a success.

### Oracles

Every acceptance criterion must name one:

```
oracle: cmd `cargo test --test rate_limit` exit 0
oracle: test tests/rate_limit.rs::rejects_over_limit
oracle: schema `schemas/gate.json` validates `.keel/specs/x/gates/G0.json`
oracle: doctest src/lib.rs
oracle: human a reviewer confirms the error message names the file
```

`human` is legal on purpose. The point is not to pretend everything can be
automated — it is to make what cannot be automated show up as a number on the
gate report instead of as a surprise on a Friday afternoon.

### Approvals mean something

An approval records the hash of the artefact as it stood at sign-off. Edit the
spec afterwards and G1 fails with `spec changed after approval` — otherwise
"approved" is a stamp applied once and inherited forever.

## Replay or it didn't happen

Every run writes `.keel/runs/<id>/trajectory.jsonl` — one JSON object per line,
append-only, gapless sequence numbers. Everything that reached the model is in
it: which store documents were injected and at what token cost, what the driver
was asked and what it answered, every command run, every gate verdict with the
path of the result file that backs it, and every human decision.

`keel export` turns a run into one `.tar.gz` containing the trajectory, the gate
results, the evidence those verdicts were reached from, the spec as agreed, the
steering the agent was given, a README that says what happened, and a manifest
with the SHA-256 of every member. `keel export --verify` checks it has not been
edited since.

## The ratchet

A ratchet is a metric that may improve and must not regress — a command printing
a number, plus a direction. It catches what no single review does: one more
warning, one fewer test. In this repository `cargo clippy` exits 0 with warnings,
so lint passes; the ratchet is what notices the count went from 0 to 1.

```toml
[[ratchet]]
id = "clippy-warnings"
cmd = "cargo clippy --all-targets -q 2>&1 | grep -c '^warning' || true"
direction = "down"
```

## Drivers

A driver is a subprocess: `keel.drivertask/1` on stdin, `keel.driverresult/1` on
stdout. That is the whole contract. `.keel/drivers/claude-code` is a ~40-line
shell adapter around `claude --print`; adding another agent is another such
script, not a change to keel.

A driver that cannot start, or that overruns its timeout, is **blocked** — never
a failure. Only a driver that ran and could not do the job is an agentic failure.
Getting this wrong would teach the Phase 3 failure taxonomy to learn from noise.

## Learning at the tail

**Attribution before class, always.** Every failure episode is attributed
`AGENTIC`, `PROCESS`, `HUMAN` or `UNATTRIBUTABLE` *before* anyone asks what kind
of mistake it was. Only `AGENTIC` episodes can become lessons — a blocked check
is an environment fact, and a third of real rejected agentic PRs have no
observable rationale at all. The `UNATTRIBUTABLE` rate is on every G4 report and
in `keel failures`: counted, never learned from.

**A lesson needs two occurrences.** In distinct runs — ten failures in one run
is one mistake, not ten. `--force` overrides it, deliberately and on the record.
This is the direct counter to the failure mode the design warns about: a store
full of confident rules derived from one flaky run.

**A lesson with an oracle stops being a prompt.** It compiles into a G2 check
carrying `from: L-nnnn`, so you can ask why a check exists and get a run id back.
An enforced lesson is *not* injected — spending context restating a rule that
cannot be violated without failing the gate would defeat the point.

```
  FAIL     lesson:L-0002
           expected: Changes MUST leave lint and the house rules clean.
           actual:   expected exit 0, got 101  [L-0002]
```

**Lessons decay.** One with no injection and no gate-fire inside its decay
period goes to demotion review, and G4 fails until it is demoted or re-verified.
Demoting archives the card with the reason, so nobody re-promotes it next
quarter. This is the direct counter to unbounded `CLAUDE.md` growth.

## Budgets are invariants

Every generated artefact has a hard line budget and is fitted to it — the
repository map by binary search over detail level, the projections by
priority-ordered max-min allocation across sections. Trimmed content is never
deleted, only deferred: each cut carries a pointer to the full text. If a budget
cannot be met, the artefact says so in its own body rather than quietly
overrunning.

## Languages

Rust, Python, JavaScript, TypeScript/TSX, Go, Java — via tree-sitter. The index
is an accelerator, never a dependency: unparseable files are still indexed as
metadata, and a grammar whose queries stop compiling is named in the `keel map`
output rather than silently yielding an empty map.

## Performance

`keel map` on 5,705 source files / 76,032 symbols: **~1.25 s** (target: < 5 s).

## Not built yet

The MCP retrieval service, incremental reindexing, task dependency waves, and
drivers beyond `claude-code` — Phases 4–5 in [PLAN.md](PLAN.md). Each phase is
designed to be a complete daily driver on its own; nothing built here is waiting
on them.
