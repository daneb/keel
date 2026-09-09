---
id: CONV-0001
scope: repo
owner: human
verified_at: 2026-08-21
---

# Conventions

House rules that apply to every change in this repository. Keep this list short
and specific: a rule nobody can violate mechanically is a rule that will be
violated. Where a rule can be checked by a command, say so — in Phase 3 those
become gate checks and stop costing context.

## Working agreement

- Match the surrounding code. Naming, comment density and idiom are local
  conventions, not global ones.
- Change the smallest surface that solves the problem. If a fix needs a wider
  blast radius, say so before making it, not after.
- A test that mocks away the behaviour under test is worse than no test.

## Authoring a spec

Write `.keel/specs/<slug>/spec.md` by looping against G0, not by guessing what
it wants up front:

1. Draft or edit one acceptance criterion: an EARS sentence (`THE SYSTEM
   SHALL …`, or `WHEN/WHILE/IF…THEN/WHERE … THE SYSTEM SHALL …`, upper case,
   never "should") plus an `oracle:` line (`cmd`, `test`, `schema`, `doctest`,
   or `human` — `human` is legal but shows up as cost on the report).
2. Run `keel gate g0 <slug>`.
3. Fix only what the verdict names — a bad EARS shape, a missing oracle, an
   unresolved placeholder, or a flagged weasel word ("handle", "appropriate",
   "robust", and similar — the full list is in `src/spec/ears.rs`) — then
   rerun. Don't pre-empt failures the gate hasn't reported yet.
4. Repeat until G0 passes, then `keel approve <slug> --stage spec`.

This applies the same way regardless of which agent is driving the
conversation — Claude Code, Kiro, Copilot, or a human alone.

## Rules

_Add rules as you find yourself repeating them. One line each, imperative mood._

- Prefer `?` over `unwrap()`.
