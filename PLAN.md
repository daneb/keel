# keel — A Gated Harness for Daily AI-Assisted Delivery

**Status:** design draft v0.1
**Working name:** `keel` (a stack of stones left by the previous traveller to mark the trail for the next one — which is exactly what the knowledge store is). Rename freely: `cairn`, `trestle`, `keel` all fit the plumb lineage.

---

## 1. What this is, and what it is not

**keel is a conductor, not another agent loop.**

It does not implement a model adapter, a tool registry, or an inference loop. Claude Code, Kiro, Copilot, Codex and `pi` already do that, and they will keep out-shipping any loop you write alone. keel sits *above* them:

```
                  ┌──────────────────────────────────────────┐
   you  ────────► │  keel  — specs, gates, store, evidence   │
                  └───┬──────────────┬──────────────┬─────────┘
                      │ drives       │ drives       │ drives
                  ┌───▼───┐      ┌───▼───┐      ┌───▼────┐
                  │Claude │      │ Kiro  │      │Copilot │   (headless / CLI)
                  │ Code  │      │       │      │        │
                  └───────┘      └───────┘      └────────┘
                      └──────── all read the same store ──────┘
```

What keel owns:

| Owns | Delegates |
| --- | --- |
| Spec → Plan → Tasks artefacts | Code generation |
| Gate definitions and gate verdicts | Tool calling |
| The single knowledge store + per-agent projections | Model selection |
| Repository structure map and symbol retrieval | Editing mechanics |
| Append-only trajectory / evidence bundles | Sandboxing (borrow the agent's) |
| Error classification and lesson promotion | UI |

This split is the reason it can be *sufficient on day one*. You are not waiting for keel to grow a feature the underlying agent already has. Everything keel adds is something no coding agent gives you: **auditable stopping conditions and durable memory across tools.**

**Anti-goal:** keel must never be the reason a task takes longer. If a gate is slower than the work it guards, the gate is wrong.

---

## 2. Seven principles

Each principle is stated, then grounded in what we can actually observe in the wild — including the Gao & Chen study you attached, which is unusually useful here because it measures behaviour instead of asserting best practice.

---

### P1 — Gates, not virtue

> Quality is produced by mechanical, falsifiable exit criteria placed between stages. It is never produced by asking the model to be careful.

A gate is a predicate over artefacts that returns `pass | fail | blocked`, with the evidence attached. If a gate cannot fail, it is documentation, not a gate. If a human has to read prose to know whether it passed, it is not a gate.

**Grounding.** Zhang et al. (arXiv 2604.11088) compared rules, skills and persistent configuration at scale and found guardrails beat guidance — and, uncomfortably, that randomly chosen rules helped about as much as carefully curated ones. That is a direct empirical attack on the "write better instructions" school. Enforcement is the variable that moves; exhortation is not. Ehsani et al. (arXiv 2601.15195) found unmerged agentic PRs are characterised by larger diffs, more files touched, and failed CI — all of which are *gateable properties*, not attitudes.

**Design consequence.** Gate definitions are data (`.keel/gates/*.toml`), gate results are data (`.keel/runs/<id>/gates/*.json`), and both are committed. The prompt is not where quality lives.

---

### P2 — One store, many agents

> There is exactly one canonical knowledge substrate. Every tool-specific context file is a generated, read-only projection of it.

`CLAUDE.md`, `AGENTS.md`, `.kiro/steering/*.md`, `.github/copilot-instructions.md` are outputs, not inputs. They carry a provenance header and a content hash; editing one directly is detected as drift and either reconciled back or rejected at the gate.

**Grounding.** This is the central finding of the attached paper and it is larger than expected: agent-facing artefacts — instruction files (35.4%) and agent working notes (25.1%) — account for **60.5%** of all observed documentation interaction, while API references account for **1.3%** and troubleshooting for **0.4%**. Agents also *write* documentation at 0.87× the rate they read it, and among the most-changed files in agentic PRs are `AGENTS.md`, `CLAUDE.md` and `copilot-instructions.md`. The store is not a side artefact of the workflow — for the agent, it *is* the workflow surface.

Two hazards come with this, both documented:

- **Unbounded growth.** Chakrabarti (arXiv 2608.11095) documents `CLAUDE.md` growing without bound — "catastrophic remembering". Every store entry therefore needs an owner, a scope and a decay rule.
- **Staleness / context rot.** Treude & Baltes (arXiv 2606.09090) apply documentation-consistency thinking to AI configuration artefacts. Store entries carry a `verified_at` and a cheap re-check.

**Design consequence.** The store is markdown (readable, diffable, greppable, portable) with YAML front matter for machine fields. Projections are generated by adapter plugins. There is a hard size budget per projection, enforced at `G0`.

---

### P3 — Prose is not an oracle

> Every acceptance criterion must name a machine-checkable oracle, or the spec does not pass its gate.

An EARS-style criterion (`WHEN <condition> THE SYSTEM SHALL <behaviour>`) is accompanied by an `oracle:` field: a test identifier, a shell command with an expected exit code, a schema to validate against, or a doctest. "Reviewer judgement" is a legal oracle value — but it is *named*, so it shows up as human cost.

**Grounding.** This is the sharpest negative result in the attached paper: across 557 sessions and 94,813 events, the authors observed **zero** instances of documentation being used as an oracle to check code. Not rare — absent. Worse, reading documentation was associated with *less* immediate testing (lift 0.23, adjusted OR 0.39) and less building (lift 0.15). The paper's own conclusion is that verifiability "corresponds to no behaviour recorded by our instrument and must therefore be designed for rather than assumed."

So: do not write a beautiful spec and expect the agent to honour it. Compile the spec down into something that runs.

**Design consequence.** `G0` fails on any criterion without an oracle. `G2` fails if a criterion's oracle was never executed in this run. Coverage here is *criterion coverage*, not line coverage.

---

### P4 — Retrieve, don't read — and budget it

> The agent navigates by structure and pulls symbols. It does not open files to find out what is in them, and it never exceeds its context budget silently.

On `keel init`, build a structural index (tree-sitter → symbol table → import/call graph → importance ranking) and emit a budget-fitted markdown map plus per-directory maps. During work, expose progressive-disclosure retrieval: outline before source, signature before body, metadata before implementation.

**Grounding.** The Codebase-Memory study (arXiv 2603.27277) measured a tree-sitter knowledge graph over MCP against a file-exploration agent across 31 repositories: roughly **10× fewer tokens and 2.1× fewer tool calls**, at 83% answer quality versus 92% — you buy an order of magnitude of context for nine points of recall on the hardest queries, and you can always fall back to a full read. The repository-map pattern (tree-sitter + PageRank + binary-search fit to a token budget) is the same idea in Aider's lineage. Meanwhile the attached paper shows documentation reads run in *streaks* — `P(read doc | read doc) = 0.270`, the strongest transition observed, and `Follow-reference` was entirely unattested — which says agents re-read rather than navigate links. Self-contained, locally retrievable units beat richly cross-linked prose.

Note the honest caveat: Claude Code deliberately avoids pre-indexing in favour of agentic search, and grep still wins on zero setup and exact patterns. So the index is an *accelerator with a fallback*, never a hard dependency.

**Design consequence.** Budgets are declared per stage and enforced, not advised. A full-file read above `N` lines requires a recorded justification, which the review gate can see. Your existing TECR work (graph-AST repo map + context budget governor) is the natural implementation — extract it as a library and let keel be its first consumer.

---

### P5 — Replay or it didn't happen

> Everything the model was shown, and every verdict a gate reached, is reconstructable from an append-only log.

One event stream per run: prompts, injections, tool calls, tool results, gate inputs, gate verdicts, human decisions. Resume, fork, diff and audit all operate on that stream. Gate verdicts embed the exact evidence (command, exit code, stdout hash) that produced them.

**Grounding.** DeepSeek Harness makes the session log the source from which model-visible history is *derived*, not merely an audit file — anything reaching a model request should be reconstructable from the event stream, which makes trajectories usable for regression analysis. That is the right invariant, and it is also the only way "auditable gates" means anything in a bank: a gate verdict you cannot reproduce is an opinion with a timestamp.

**Design consequence.** `.keel/runs/<id>/trajectory.jsonl`, append-only, plus an `evidence/` directory. A run is exportable as a single bundle for review or for your governance audience.

---

### P6 — Learn at the tail, and only through a gate

> Failures are classified, distilled into short lessons, and promoted into the store only by an explicit gate. Raw traces are never memory.

Every failure episode in a run is classified against a fixed taxonomy (§4.6). At end of run, a distillation step proposes zero or more *lesson cards*. Promotion requires: a testable rule, a second occurrence (or a human override), and no contradiction with an existing lesson. Unreferenced lessons decay.

**Grounding.** Three results shape this:

- **Learn from failures, not just successes, and store strategies rather than traces.** ReasoningBank (arXiv 2509.25140) distils reusable reasoning units from both successful and failed runs, reporting up to ~20% relative effectiveness gain and ~16% fewer interaction steps versus baselines that store raw trajectories or success-only routines.
- **Update incrementally or you lose the detail.** ACE (arXiv 2510.04618) names the two failure modes precisely: *brevity bias* (summarising away the domain insight) and *context collapse* (iterative rewriting eroding detail). Structured incremental edits, not rewrites.
- **Do not let unattributable failures pollute the store.** Peralta et al. (arXiv 2605.22534) inspected 353 rejected agentic PRs: only **35.7%** were clear agentic failures; **31.2%** were workflow-driven (duplicates, superseded, inactivity) and **33.1%** had no observable rationale. If you learn from all three buckets you are training on noise. The taxonomy must have an explicit `UNATTRIBUTABLE` class that is counted but never promoted.

One further caution from the attached paper: documentation was the first recovery move in only **5.4%** of 2,034 failure episodes, and only 7.5% of documentation interaction was failure-driven. Agents do not reliably go looking for lessons when stuck. So **lessons are injected by the harness at the relevant stage**, not left on a shelf for the agent to find.

---

### P7 — Everything is a plugin, except the spine

> The extension surface is wide and language-agnostic. The artefact schemas and the gate contract are versioned, small, and stable.

Three extension points in v1, no more:

1. **Gate checks** — a command returning a `GateResult` JSON on stdout.
2. **Store projections** — a command rendering the store into a tool-specific file.
3. **Agent drivers** — a command that runs a task headlessly against a given agent CLI and reports back.

Each registers a disposer so removal is clean.

**Grounding.** This is the good half of the Cordis idea: DeepSeek Harness makes models, tools, skills, sessions, sandboxes, storage, loops, scheduling and UI all replaceable plugins, wired by declared service dependencies rather than boot order, with effects that unwind on unload. The theoretical framing (spatiotemporal composability) gives you safe removal. The honest critique — raised by practitioners reviewing Cordis — is that the contracts are hard to follow precisely once an LLM is in the loop, and DSH itself ships with an explicit expect-breaking-changes warning.

So: take the *pattern* (contract + reversible registration + config-driven composition), reject the *ambition* (a fully pluggable agent loop). Your loop is someone else's product. Your spine is the thing that must not move.

---

## 3. Principle → mechanism map

| # | Principle | Primary mechanism | Fails loudly when |
| --- | --- | --- | --- |
| P1 | Gates, not virtue | `gates/*.toml`, `GateResult` | A stage advances without a verdict |
| P2 | One store, many agents | `store/` + adapters + hash drift check | A projection is hand-edited |
| P3 | Prose is not an oracle | `oracle:` field on every criterion | A criterion has no runnable check |
| P4 | Retrieve, don't read | structure map + symbol tools + budgets | Budget exceeded, or unjustified full reads |
| P5 | Replay or it didn't happen | `trajectory.jsonl` + evidence bundle | A verdict cannot be reproduced |
| P6 | Learn at the tail | taxonomy + lesson cards + promotion gate | A lesson is promoted without a rule |
| P7 | Plugin edges, stable spine | 3 plugin kinds, versioned schemas | A plugin needs a spine change |

---

## 4. Architecture

### 4.1 Repository layout

```
.keel/
  keel.toml                  # config: budgets, gates enabled, adapters, drivers
  store/                      # THE canonical knowledge substrate
    steering/
      product.md              # what this is for, who uses it
      tech.md                 # stack, versions, constraints
      structure.md            # generated: repo structure map (budget-fitted)
      conventions.md          # house rules, curated
    map/
      index.sqlite            # symbol table, import/call graph  (generated)
      <dir>/CODEMAP.md        # per-directory maps               (generated)
    lessons/
      L-0001-<slug>.md        # promoted lesson cards
    decisions/
      ADR-0001-<slug>.md
  specs/
    <slug>/
      spec.md                 # requirements, EARS + oracles
      plan.md                 # design, blast radius, budgets
      tasks.md                # ordered, each ↦ criteria
      evidence/
  runs/
    <run-id>/
      trajectory.jsonl        # append-only, everything shown to the model
      gates/G0.json … G4.json
      evidence/
      failures.jsonl          # classified failure episodes
  adapters/                   # generated projections (read-only, hashed)

# projections written to their conventional homes:
CLAUDE.md · AGENTS.md · .kiro/steering/*.md · .github/copilot-instructions.md
```

Everything under `store/`, `specs/` and the projections is committed. `runs/` is committed selectively (or archived) — that is your audit trail.

### 4.2 The store

Two tiers, borrowed from Kiro's split and validated by the attached paper's distinction between instruction files and working notes:

- **Steering** — durable, always injected, small. Product, tech, structure, conventions. Hard budget (suggest ~300 lines total across all steering; the projection budget is what actually binds).
- **Working notes** — per-spec, per-run, injected only in scope. Plans, verification logs, thoughts.

Every store file has front matter:

```yaml
---
id: CONV-0007
scope: repo | dir:src/api | lang:rust
owner: human | agent
verified_at: 2026-08-21
decay: 90d          # unreferenced after this → demotion review
sources: [runs/2026-08-14-a3f/failures.jsonl#L12]
---
```

`sources` is what makes a rule auditable: you can ask *why does this rule exist* and get a run ID.

### 4.3 Projections and drift

```
store/  ──render──►  CLAUDE.md   (header: generated by keel, hash=…)
        ──render──►  AGENTS.md
        ──render──►  .kiro/steering/*.md
        ──render──►  .github/copilot-instructions.md
```

`keel store check` recomputes hashes. Drift → either `keel store reconcile` (pull the human edit back into the canonical file) or fail `G0`. This is the only way a single store survives contact with four tools that each want to own their own file.

### 4.4 Gate pipeline

Keeps the plumb naming so your muscle memory transfers, with one addition at the tail.

```
IDEA ─► SPEC ─G0─► PLAN ─G1─► IMPLEMENT ─G2─► REVIEW ─G2.5─► HUMAN ─G3─► MERGE ─G4─► STORE
```

| Gate | Guards | Representative checks |
| --- | --- | --- |
| **G0** | Spec is buildable | Every criterion in EARS form; every criterion has an `oracle`; ambiguity count = 0; store drift = none; scope budget declared |
| **G1** | Plan is bounded | Every task ↦ ≥1 criterion; blast radius computed from the map and declared; per-task line budget set; rollback stated; no task without an exit condition |
| **G2** | Implementation is verified | Build + test + lint green; every criterion's oracle executed; diff ⊆ declared blast radius; line budget respected; baseline ratchet not regressed; no new unjustified full-file reads |
| **G2.5** | Adversarial review | Second pass against `conventions.md` + relevant lessons; looks specifically for test-invalidation (mocks that hide the bug) and scope creep |
| **G3** | Human decision | Evidence bundle attached; diff size within reviewable limit; human verdict recorded |
| **G4** | Learning | Every failure episode classified; lesson candidates proposed; promotions accepted/rejected by human |

**Gate contract** (stable, versioned):

```json
{
  "gate": "G2",
  "run": "2026-08-21-7c1",
  "verdict": "fail",
  "checks": [
    { "id": "blast-radius",
      "verdict": "fail",
      "expected": "src/api/**, tests/api/**",
      "actual":   "src/api/**, src/core/auth.rs",
      "evidence": "evidence/diff-stat.txt" },
    { "id": "oracle-coverage",
      "verdict": "pass",
      "detail": "7/7 criteria executed",
      "evidence": "evidence/oracles.json" }
  ],
  "schema": "keel.gate/1"
}
```

Note `blocked` as a third verdict, distinct from `fail`: the check could not run (missing tool, no network). Blocked never silently passes and never counts as an agentic failure (see P6).

### 4.5 Trajectory and evidence

`trajectory.jsonl` — one JSON object per event, append-only:

```json
{"t":"2026-08-21T09:14:02Z","seq":412,"kind":"inject","source":"store/lessons/L-0004.md","tokens":86}
{"t":"2026-08-21T09:14:03Z","seq":413,"kind":"tool_call","name":"symbol","args":{"q":"AuthGuard"},"tokens_out":210}
{"t":"2026-08-21T09:16:41Z","seq":611,"kind":"gate","gate":"G2","verdict":"fail","ref":"gates/G2.json"}
```

Invariant, borrowed directly from DSH: **anything that reached the model must be reconstructable from this stream.** That includes lesson injections — otherwise you can never answer "did the lesson actually help?"

### 4.6 Failure taxonomy

Fixed, small, and explicitly separating agent failure from everything else. This is the part most home-grown harnesses get wrong by conflating "the PR didn't land" with "the agent was wrong".

**Attribution (first, always):**

| Code | Meaning |
| --- | --- |
| `AGENTIC` | Observable technical failure caused by the agent's output |
| `PROCESS` | Workflow/environment: duplicate, superseded, flaky infra, missing credential |
| `HUMAN` | Requirement changed, human redirected, scope renegotiated |
| `UNATTRIBUTABLE` | No observable rationale — counted, never promoted to a lesson |

**Class (only meaningful when `AGENTIC`):**

| Code | Locus | Typical detector |
| --- | --- | --- |
| `SPEC-AMBIG` | spec | G0 ambiguity check, or a late "what did you mean" |
| `SPEC-MISSING` | spec | new criterion added mid-implement |
| `LOC-WRONG` | retrieval | edited symbol outside blast radius / wrong file |
| `CTX-STALE` | retrieval | acted on a store or map entry older than the code |
| `CTX-DRIFT` | context | contradicts an earlier established fact in the same run |
| `EDIT-COMPILE` | edit | build failure |
| `EDIT-RUNTIME` | edit | test failure, assertion, exception |
| `TEST-INVALID` | verification | test passes but mocks away the behaviour under test |
| `SCOPE-CREEP` | plan | diff exceeded declared blast radius or budget |
| `CONV-VIOLATION` | conventions | lint/house-rule breach, naming, layering |

Two calibration points from the literature worth encoding as expected priors, so your dashboards are not surprised: runtime errors dominate compile-time (~63/37) in agentic test failures, with assertion failures the single largest bucket (~29%); and the largest category of human intervention in agentic PRs is *guidance-level* — restricting the agent and enforcing project conventions (~58%) — which maps to `CONV-VIOLATION` and `SCOPE-CREEP`, not to `EDIT-*`. If your taxonomy is mostly firing on `EDIT-RUNTIME`, you are measuring the model. If it fires on `SCOPE-CREEP` and `CONV-VIOLATION`, you are measuring the harness — and those are the ones gates can actually fix.

### 4.7 Lesson cards

```markdown
---
id: L-0012
class: CONV-VIOLATION
scope: dir:src/api
occurrences: 3
rule_kind: gate-check | prompt-injection | both
verified_at: 2026-08-21
decay: 90d
sources: [runs/2026-08-14-a3f, runs/2026-08-19-b02, runs/2026-08-21-7c1]
---

**Trigger** New handler added under `src/api/`.

**Observation** Agent registers routes inline in `mod.rs` three times; house rule is
registration via `routes/registry.rs`.

**Rule** Route registration MUST occur in `routes/registry.rs`.

**Oracle** `rg -n 'router\.route\(' src/api --glob '!routes/registry.rs'` → expect 0 matches.
```

Promotion rules:

1. `attribution == AGENTIC` (never `PROCESS`/`UNATTRIBUTABLE`).
2. `occurrences >= 2`, or explicit human override.
3. Has a `rule` and — preferably — an `oracle`. A lesson with an oracle becomes a **gate check** and stops being a prompt. Prefer this every time: a lesson that is enforced does not need to be read.
4. No contradiction with an existing lesson in overlapping scope (fail loudly, force a merge).
5. ≤ 12 lines. Long lessons are specs in disguise.

**Decay.** A lesson with no injection and no gate-fire in `decay` days goes to a demotion review. This is the direct counter to unbounded `CLAUDE.md` growth.

**Injection.** Lessons are selected by scope + stage and injected by keel — never left for the agent to discover (5.4%, remember).

### 4.8 Retrieval layer

```
init:   walk → tree-sitter parse → symbols + imports + calls → SQLite
        → importance rank → budget-fit → structure.md + per-dir CODEMAP.md

query:  outline(path)            file skeleton, no bodies
        symbol(name)             signature + doc + location
        source(symbol_id)        body, on demand only
        refs(symbol)             callers
        importers(path)
        blast_radius(sym, d=2)   depth-weighted impact set
        slice(task_id)           the bundle for one task, budget-fitted
```

Exposed twice: as a CLI (for scripting and for gates) and as an MCP server (so Claude Code, Kiro and Copilot all get the same view). Progressive disclosure is the default — metadata first, source when asked.

Fallback path is mandatory: if the index is stale, absent, or the language has no grammar, fall through to ripgrep + read. The index is an accelerator, never a dependency. This matters because Claude Code's own designers concluded agentic search beat indexed RAG for their case; you want both, with the cheap one first.

### 4.9 Plugin contract

```toml
# .keel/keel.toml
[[gate.G2.check]]
id      = "route-registry"
cmd     = "keel-check-routes --json"
from    = "lesson:L-0012"

[[adapter]]
id      = "claude"
cmd     = "keel-render-claude"
out     = "CLAUDE.md"
budget  = 180          # lines

[[driver]]
id      = "claude-code"
cmd     = "keel-drive-claude-code"
default = true
```

Every plugin is a subprocess speaking JSON on stdout. Language-agnostic, testable in isolation, trivially mockable in CI. Registration returns a disposer so `keel plugin remove` unwinds cleanly.

---

## 5. Phased plan

The governing constraint you set: **every phase must be a complete daily driver.** So each phase below has a *sufficiency contract* — the sentence that must be true for you to use it every day without wishing for the next phase — and an explicit list of what is deliberately not built yet. A phase is done when the sufficiency contract holds, not when the code is elegant.

Suggested implementation language: **Rust**. You already have the tree-sitter + SQLite navigation engine and the TECR context governor in Forgiven; keel's Phase 4 is largely "extract and reuse". Single static binary also matters for a bank laptop.

---

### Phase 0 — Store and map (target: ~1 week)

**Sufficiency contract:** *Every AI session in any tool starts from the same, current, budget-bounded picture of this repo — and I can tell when that picture has drifted.*

Build:

- `keel init` — scaffold `.keel/`, interview for `product.md` / `tech.md`, generate `structure.md` from a first-pass structural walk.
- `keel map` — tree-sitter parse → SQLite symbol table → importance rank → budget-fitted `structure.md` + per-directory `CODEMAP.md`.
- `keel store render` — projections to `CLAUDE.md`, `AGENTS.md`, `.kiro/steering/`, `.github/copilot-instructions.md`, each with a hash header and a line budget.
- `keel store check` — drift detection. Wire to a pre-commit hook.

Not yet: gates, specs, orchestration, retrieval tools, learning.

**Exit criteria:** map regenerates in < 5s on your largest repo; all four projections under budget; drift check catches a hand-edit; you have run a week of normal work with it.

**Why this first:** it is the highest ratio of value to code in the whole plan. It is also the piece the attached paper says the agents actually consume — 60.5% of interaction — so it moves the needle before a single gate exists.

---

### Phase 1 — Spec → Plan → Tasks with G0/G1 (target: ~2 weeks)

**Sufficiency contract:** *I can turn an idea into a spec whose every criterion is falsifiable, and a plan whose blast radius is computed rather than guessed — and I never hand a vague spec to an agent by accident.*

Build:

- `keel spec new <slug>` — drives the configured agent to produce `spec.md` (EARS criteria + `oracle:` per criterion), then runs G0.
- G0 checks: EARS conformance, oracle presence, ambiguity scan, store drift, scope budget declared.
- `keel plan` — produces `plan.md` and `tasks.md`; blast radius computed from the Phase 0 map; per-task budgets.
- G1 checks: task↦criterion traceability, blast radius declared, budgets set, rollback stated.
- Human approval checkpoints between stages (explicit, recorded).

Not yet: automated implementation, trajectory log, learning.

**Exit criteria:** three real features specced through G0/G1; at least one spec *failed* G0 for a missing oracle and you agreed the gate was right.

**Risk to watch:** spec bloat. Kiro users report agents producing far more testing and documentation ceremony than a human wants. Put a hard criterion count and line budget in `keel.toml` and enforce it at G0.

---

### Phase 2 — Execution, evidence and G2/G2.5/G3 (target: ~3 weeks)

**Sufficiency contract:** *I can run a task to completion through a real agent CLI and get back a pass/fail verdict with attached evidence I could hand to an auditor — and nothing merges without it.*

Build:

- Agent driver plugin interface + first driver (`claude-code` headless).
- `keel run <task>` — execute, capture, gate.
- `trajectory.jsonl` append-only stream; evidence bundle per run.
- G2: build/test/lint, oracle-coverage (every criterion executed), diff ⊆ blast radius, budget respected, **baseline ratchet**.
- G2.5: adversarial review pass against conventions + injected lessons (lesson store is empty until Phase 3 — that's fine).
- G3: human verdict recorded; `keel export <run>` produces a reviewable bundle.

Not yet: learning, MCP retrieval server, parallel tasks.

**Exit criteria:** a full spec→merge cycle with no manual gate bookkeeping; a run reproducible from its trajectory; the ratchet actually blocks a regression once.

**This is the phase where keel earns its keep.** Everything before it is scaffolding; everything after it is amplification.

---

### Phase 3 — Failure classification and lesson promotion, G4 (target: ~2 weeks)

**Sufficiency contract:** *Recurring mistakes stop recurring, because the second occurrence turns into a gate check rather than a paragraph.*

Build:

- Failure episode extraction from `trajectory.jsonl` (a failure signal + the first recovery action).
- Classifier: attribution first (`AGENTIC` / `PROCESS` / `HUMAN` / `UNATTRIBUTABLE`), then class.
- `keel learn <run>` — proposes lesson cards; G4 requires human accept/reject.
- Lesson → gate-check compilation: a lesson with an oracle registers itself as a G2 check.
- Scope-based injection at the right stage.
- Decay job + demotion review.

Not yet: cross-repo lesson sharing, automatic rule synthesis without human sign-off.

**Exit criteria:** ≥ 5 promoted lessons, ≥ 2 of which became gate checks; `UNATTRIBUTABLE` rate visible on a dashboard and not silently learned from; a lesson has decayed and been demoted.

**Design warning:** resist promoting on a single occurrence. The single strongest failure mode of learning harnesses is a store full of confident rules derived from one flaky run.

---

### Phase 4 — Retrieval service (target: ~2 weeks)

**Sufficiency contract:** *Agents work from symbols, not files, in every tool — and I can see the token cost fall.*

Build:

- Promote the Phase 0 SQLite index into a queryable service.
- CLI + MCP server exposing `outline / symbol / source / refs / importers / blast_radius / slice`.
- Incremental reindex on file change (tree-sitter's incremental parse makes this cheap).
- Budget governor: per-stage token budgets, progressive disclosure, justification required for large full-file reads.
- Fallback to ripgrep on stale/absent index or unsupported grammar.

**Exit criteria:** measured token-per-task drop on a fixed set of five representative tasks, before/after, on the same model. Target the published order of magnitude; accept anything ≥ 3×. If you cannot measure it, do not ship it — the literature here is full of vendor numbers and at least one publicly retracted benchmark.

---

### Phase 5 — Breadth (ongoing)

**Sufficiency contract:** *New tools, new checks and new repos plug in without touching the spine.*

- Additional drivers: Kiro, Copilot, Codex, `pi`.
- Task dependency waves — independent tasks in parallel, dependent ones sequential (Kiro's wave model is a good reference).
- Cross-repo store: shared conventions/lessons as a git submodule or package, with local override precedence.
- Metrics surface: gate pass rates, failure class distribution, lesson hit rate, tokens/task, human-intervention minutes/task.
- `keel doctor` — health of index, drift, decayed lessons, orphaned specs.

**Spine freeze:** from Phase 3 onward, `keel.gate/1`, `keel.spec/1`, `keel.lesson/1` and the trajectory event schema are versioned and additive-only. If a plugin needs a spine change, that is a design review, not a patch.

---

## 6. Failure modes to design against

| Risk | Symptom | Mitigation |
| --- | --- | --- |
| Gate theatre | Gates always pass | Track pass rate per gate; a gate that never fails in 20 runs is deleted or tightened |
| Store obesity | Projections hit budget every render | Hard budget + decay + prefer gate-check over prompt-rule |
| Spec ceremony | 40 criteria for a 3-file change | Criterion count budget at G0; "quick path" for changes under N lines that skips to G2 |
| Learning on noise | Lessons derived from flaky CI | Attribution before class; `PROCESS` and `UNATTRIBUTABLE` never promote |
| Index rot | Stale symbols, confident wrong answers | `verified_at` + incremental reindex + mandatory ripgrep fallback |
| Harness lag | Underlying CLI ships a feature you half-built | Drivers stay thin; never reimplement what the agent already does |
| Solo-tool lock-in | Only works with Claude Code | Driver interface tested against ≥ 2 tools before Phase 5 |

---

## 7. Open decisions for you

1. **Name.** `keel` is a placeholder. Given `plumb`, something in the same trade vocabulary may read better as a pair.
2. **Relationship to plumb.** Is keel the successor (plumb's gate engine becomes keel's spine) or a sibling that *calls* plumb for the gate pipeline? The former is cleaner; the latter preserves your published artefact.
3. **Language.** Rust for the TECR reuse and single-binary distribution; TypeScript if you want to sit closer to the DSH/Cordis plugin ecosystem and consume their plugins directly.
4. **Quick path.** Should trivial changes bypass G0/G1 entirely (Kiro's "quick spec" analogue)? Recommend yes, with a line-count threshold — otherwise the harness gets abandoned for small work, which is most work.
5. **Store scope for CIB.** Whether `conventions.md` and lessons are per-repo or pulled from a shared platform-level store. This is the piece that scales your 50+ application portfolio, and it is also the piece that turns keel into a governance instrument rather than a personal tool.

---

## 8. References

**Attached paper**

- Gao & Chen (2026). *From Agent Behaviour to Agent-Friendly Documentation.* arXiv:2608.20195. — 60.5% agent-facing; 1.3% API reference; zero validation events; `P(edit code | read doc)` = 0.002; 70.2% self-initiated; 5.4% doc-first recovery; code precedes docs 4.7×.

**Harness architecture**

- DeepSeek Harness / Cordis (2026). Everything-is-a-plugin; append-only trajectory as the source of model-visible history; sandbox and approval as separate controls. MIT, developer preview.
- Kiro. Specs (requirements/design/tasks, EARS), steering files, hooks, task dependency waves.

**Gates and configuration**

- Zhang et al. (2026). *Guardrails Beat Guidance.* arXiv:2604.11088.
- Chakrabarti (2026). *Why Does CLAUDE.md Keep Growing?* arXiv:2608.11095.
- Treude & Baltes (2026). *Context Rot in AI-Assisted Software Development.* arXiv:2606.09090.
- Gloaguen et al. (2026). *Evaluating AGENTS.md.* arXiv:2602.11988.

**Failure and learning**

- Ehsani et al. (2026). *Where Do AI Coding Agents Fail?* arXiv:2601.15195.
- Peralta et al. (2026). *Why Are Agentic PRs Merged or Rejected?* arXiv:2605.22534 — 35.7% / 31.2% / 33.1% attribution split.
- ReasoningBank (2026). arXiv:2509.25140 — learn from failures, store strategies not traces.
- ACE (2026). *Agentic Context Engineering.* arXiv:2510.04618 — brevity bias, context collapse.
- MSR 2026 Mining Challenge — test-failure taxonomy; runtime ~63% vs compile ~37%, assertions ~29%.
- MSR 2026 — developer interventions in agentic PRs; guidance-level ~58%.

**Retrieval**

- Codebase-Memory (2026). arXiv:2603.27277 — ~10× tokens, 2.1× tool calls, 83% vs 92% quality over 31 repos.
- Repository-map pattern (Aider lineage) — tree-sitter + PageRank + budget fit.
- SWE-Adept (2026). arXiv:2603.01327 — definition-level indexing with local adjacency lists.
