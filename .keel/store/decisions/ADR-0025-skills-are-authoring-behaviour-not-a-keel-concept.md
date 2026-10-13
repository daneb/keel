---
id: ADR-0025
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-10-13
phase: field
---

# Agent "skills" are authoring behaviour, not a keel concept

## Context

A question arose: should keel adopt a library of reusable agent "skills" in the
style of Matthew Pocock's `mattpocock/skills` (`grill-me`, `tdd`,
`diagnosing-bugs`, `handoff`, …), either as a first-class keel concept or to
improve the roles keel already defines?

The motivation is sound and worth stating plainly: better software, pursued two
ways —

- **(a)** pushing the user to clarify requirements before anything is built;
- **(b)** importing industry-borne engineering practices that tend toward better
  outcomes.

These two motivations are **assumptions, not established truth**. That a
grilling step raises requirement quality, and that packaged practices transfer
to better outcomes in *this* harness, are plausible beliefs this project has not
measured. This ADR is a decision taken under that uncertainty, not a claim the
uncertainty is resolved.

The decision matters because the two things are different *kinds* of artefact.
Pocock's skills are, by their own README, deliberately *not* a process owner:
they are small, composable, injected **prose** that shapes how an agent behaves
in the moment, with no enforcement. keel is the opposite by design — a
conductor, not an agent loop, whose entire value is **gates, not virtue** (P1):
a rule nobody can mechanically violate. A skill is virtue by construction; it
lives only as an injected prompt and binds nothing.

keel already owns the *enforced* form of most of what these skills nudge:
`tdd` ↔ G2.5 (tests untouched, no mock of the behaviour under test) plus `test:`
oracles; `diagnosing-bugs` ↔ G4 failure taxonomy and lesson promotion;
`handoff` ↔ the evidence bundle and chain. Where a behaviour can become an
oracle, ADR-0007 already says it compiles into a G2 check and *stops* being a
prompt. A "skills" concept is the inverse of that principle.

The one skill with no keel equivalent is `grill-me`: the gap between what the
user *wants* and what the agent *builds*. That gap sits **upstream of G0**. G0
checks that a spec is falsifiable (EARS + an oracle per criterion); it does not
check that the spec captures what the human meant. A human still authors the
criteria.

Before committing to build anything for that gap, we checked whether the gap is
the one actually hurting this repository. It is not — at least not visibly. Of
166 classified failure episodes across 53 runs, the largest class is
`scope-creep` (46), but 45 of those 46 fired on `line-budget` (23) or
`blast-radius` (22) — the agent overran a budget or edited files outside the
declared scope, not misunderstood an ambiguous requirement. 61% of all failures
are AGENTIC, only 23% PROCESS. And well-specified features still took many runs
to pass: `copilot-credential-preflight` has six EARS criteria each with an
oracle, explicit scope and out-of-scope lists, and *still* took eight runs — its
fails were mechanical (`ghp_` vs `gho_`, worktree ordering, budget), none of
which a sharper requirements conversation would have prevented. So the evidence
says a grilling step would address a problem this repository does not
demonstrably have. (This is inference from the checks that fired, not proof no
run was ever hurt by a vague spec; attribution is itself heuristic, per
ADR-0005.)

## Decision

1. **Do not add "skills" as a keel concept, and do not rework the four
   subprocess roles** (drivers, gate checks, reviewers, shared stores — the
   roles confirmed as the subject of the question). They are the right
   abstractions, they are enforced, and a bag-of-prompts concept would blur P1
   and the stable-spine contract (P7). keel's differentiator is precisely that
   it is *not* a library of hopeful prompts.

2. **Set aside building any `grill-me`-style requirement-grilling step, as
   premature.** Were it built, it would belong in the authoring loop
   (`keel spec new`, the "Authoring a spec" house rule) as agent-side behaviour
   feeding G0 — not as a keel component, and never replacing a gate. But the run
   evidence above does not show the want-vs-build gap as a live failure mode in
   this repository, so building the mechanism now would be optimising a problem
   we cannot yet see. The idea is recorded, not pursued. If the evidence changes
   — a measurable share of failures traced to specs that were falsifiable but
   wrong — this is the first thing to reach for, and the shape above is where it
   goes.

3. **Treat the motivations (a) and (b) as unproven assumptions, and let
   evidence — not appetite — decide when to act on them.** Any future adoption
   of a practice should earn its place the way a lesson does: evidence first,
   two runs to promote (ADR-0006), and an oracle if one exists (ADR-0007) — not
   by importing a prompt library wholesale, and not by building a mechanism
   ahead of the failure data that would justify it.

## Consequences

- The spine stays small and enforcement-first; no new concept competes with
  gates for authority.
- Nothing is built for the want-vs-build gap. That is the point: the gap is
  real in general but not visible in this repository's failure data, so a
  grilling mechanism would be premature optimisation. The decision records the
  idea and the reason for not pursuing it, so it need not be re-litigated from
  scratch.
- The trigger to revisit is named and measurable: a grilling step becomes worth
  building when a non-trivial share of failures traces to specs that passed G0
  (falsifiable) but were *wrong* — distinct from the budget/blast-radius and
  edit-runtime failures that dominate today. `keel metrics` and the failure
  taxonomy are where that signal would show up.
- Rejecting a popular, well-regarded external library has a cost: contributors
  may expect keel to consume `mattpocock/skills` directly. The answer is that
  anyone may run those skills in their *agent* alongside keel; keel just does
  not absorb them into its spine or present them as its own roles.
