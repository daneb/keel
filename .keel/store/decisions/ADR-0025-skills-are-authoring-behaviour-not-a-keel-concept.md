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

## Decision

1. **Do not add "skills" as a keel concept, and do not rework the four
   subprocess roles** (drivers, gate checks, reviewers, shared stores — the
   roles confirmed as the subject of the question). They are the right
   abstractions, they are enforced, and a bag-of-prompts concept would blur P1
   and the stable-spine contract (P7). keel's differentiator is precisely that
   it is *not* a library of hopeful prompts.

2. **Borrow `grill-me`'s intent — requirement clarification — as agent-side
   authoring behaviour, not as a keel component.** It belongs in the authoring
   loop (`keel spec new`, the "Authoring a spec" house rule), in the
   driver/steering layer, feeding better input into G0. Steal the behaviour,
   not the mechanism. The gate keeps the final word.

3. **Record the motivations (a) and (b) as assumptions to be tested, not as
   accepted truth.** Any future adoption of a practice beyond requirement
   grilling should earn its place the way a lesson does: evidence first, two
   runs to promote (ADR-0006), and an oracle if one exists (ADR-0007) — not by
   importing a prompt library wholesale.

## Consequences

- The spine stays small and enforcement-first; no new concept competes with
  gates for authority.
- The real gap — pre-G0 alignment between human intent and the written spec —
  is named and assigned to the authoring layer, where it can be improved
  without touching the spine.
- The decision is explicitly provisional on its assumptions. If a grilling step
  is added and does *not* measurably improve spec quality (e.g. no drop in G1
  "spec changed after approval" or in late-stage rework), that is grounds to
  revisit, because this ADR was taken under acknowledged uncertainty.
- Rejecting a popular, well-regarded external library has a cost: contributors
  may expect keel to consume `mattpocock/skills` directly. The answer is that
  anyone may run those skills in their *agent* alongside keel; keel just does
  not absorb them into its spine or present them as its own roles.
