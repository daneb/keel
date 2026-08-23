# Getting started

Ten minutes to a gated change. You need `cargo`, `git`, and — for the last
section — an agent CLI.

```bash
cargo install --path .
```

---

## 1. Set it up

From the root of a git repository with at least one commit:

```bash
keel init          # scaffolds .keel/, seeds the store, builds the map
keel hook install  # a drifted projection now blocks the commit
keel doctor        # is everything in working order?
```

`init` wrote four files worth correcting before anything else. They are the
context every agent will read, and the seeded versions are placeholders:

| File | Say |
| --- | --- |
| `.keel/store/steering/product.md` | What this repo is for, who depends on it, what it deliberately does not do |
| `.keel/store/steering/tech.md` | Stack, and the **exact** build/test/lint commands |
| `.keel/store/steering/conventions.md` | House rules. Short. One line each |
| `.keel/keel.toml` | `[verify]` — the commands G2 runs |

Set `[verify]` now; G2 blocks without it:

```toml
[verify]
build = "cargo build --quiet"
test  = "cargo test --quiet"
lint  = "cargo clippy --all-targets --quiet"
```

Then `keel store render` to push it all into `CLAUDE.md`, `AGENTS.md`,
`.kiro/steering/` and `.github/copilot-instructions.md`.

> **Never edit those four files.** They are generated. `keel store check` will
> catch you, and `keel store reconcile` will rescue the edit into
> `.keel/store/inbox/` — but the store is where the words belong.

---

## 2. Write a spec that can fail

```bash
keel spec new rate-limit --scope 'src/api/**'
```

This scaffolds a spec **and immediately fails G0** — deliberately. The template
is full of placeholders, and a gate that passes its own scaffold is decoration.

Fill in `.keel/specs/rate-limit/spec.md`. Every criterion needs two things:

```markdown
### AC-1 Requests over the limit are rejected

WHEN a client exceeds 100 requests per minute THE SYSTEM SHALL respond with HTTP 429.

oracle: cmd `cargo test --test rate_limit over_limit` exit 0
```

- **An EARS sentence.** One of: `THE SYSTEM SHALL …`, `WHEN … THE SYSTEM SHALL …`,
  `WHILE … THE SYSTEM SHALL …`, `IF … THEN THE SYSTEM SHALL …`,
  `WHERE … THE SYSTEM SHALL …`. Upper case, and never "should".
- **A runnable oracle.** `cmd`, `test`, `schema`, `doctest` — or `human`, which
  is legal but shows up as a cost on the report.

```bash
keel gate g0 rate-limit
```

G0 checks EARS form, an oracle on every criterion, no leftover placeholders, no
vague words ("handle", "appropriate", "robust"), a declared scope and diff
budget, and that your projections are current. When it passes:

```bash
keel approve rate-limit --stage spec
```

---

## 3. Plan it

```bash
keel plan rate-limit
```

keel computes the blast radius **from the import graph** and writes `plan.md`
and a `tasks.md` scaffold. Fill in the approach, the `rollback:` field, and each
task's files, budget and exit condition. Add `depends_on:` where order matters.

```bash
keel tasks                       # the plan as dependency waves
keel gate g1 rate-limit
keel approve rate-limit --stage plan
```

G1 checks every task traces to a criterion and every criterion to a task, that
budgets and exit conditions exist, that the rollback is stated, that no two
tasks in one wave claim the same file, and that the recorded blast radius still
matches a fresh computation.

---

## 4. Do the work, then gate it

**Start here.** Make the change yourself, or with whatever tooling you already
use, then:

```bash
keel run rate-limit --no-driver
```

That runs G2 (build, test, lint, every oracle executed, diff inside scope, line
budget, ratchet), G2.5 (mocks added? tests untouched?) and G3 (evidence
complete, diff reviewable, human verdict). You get a verdict per check with the
evidence behind it.

```bash
keel approve rate-limit --stage merge   # the G3 decision
keel export                             # one verifiable .tar.gz
```

**When you want an agent to do it**, check the driver first — it runs in a
scratch repo, never your tree:

```bash
keel driver list
keel driver check claude-code
keel run rate-limit                     # serial
keel run rate-limit --waves             # one git worktree per task
```

---

## 5. Let it learn

After a few runs:

```bash
keel learn        # classify failures, propose lessons
keel lessons      # what is in force
keel metrics      # pass rates, failure classes, gate theatre
```

A lesson needs **two occurrences in distinct runs** before it can be promoted.
Give it an oracle and it becomes a G2 check that costs no context; without one
it is injected as a prompt.

---

## The five commands you will actually use

```bash
keel doctor       # start every session here
keel status       # is the picture current?
keel run <slug> --no-driver
keel blast 'src/api/**'
keel learn
```

## When something is wrong

| Symptom | Meaning | Fix |
| --- | --- | --- |
| `DRIFT` | A generated file was hand-edited | `keel store reconcile <path>` |
| `stale` | The store moved on | `keel store render` |
| `BLOCKED` | A check could not run — not a failure | Fix the environment |
| `blast-radius` fails | The diff left the declared scope | Widen the scope deliberately, or move the work |
| `spec changed after approval` | You edited after signing off | Re-approve |
| G0 fails on your first spec | Working as designed | Fill in the placeholders |

`blocked` is never the agent's fault and never becomes a lesson. That
distinction is load-bearing — see
[ADR-0002](.keel/store/decisions/ADR-0002-blocked-is-a-verdict.md).

---

Next: [README.md](README.md) for the architecture, [PLAN.md](PLAN.md) for why
any of it is shaped this way.
