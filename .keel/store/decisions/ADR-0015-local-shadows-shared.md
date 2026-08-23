---
id: ADR-0015
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-23
phase: 5
---

# A local lesson shadows a shared one, and a missing required store is loud

## Context

A shared store lets platform conventions and lessons be written once and apply
across a portfolio. Two questions follow immediately: what happens when a
repository disagrees, and what happens when the store cannot be found.

## Decision

**Local wins by shadowing.** A local lesson with the same id as a shared one
replaces it. A repository must be able to say "not here" about a platform rule,
and doing so by shadowing an id is visible in review; silently ignoring it is
not. A shared card is not a consumer's to demote — the error says to shadow it
or change it where it is published.

**A missing required store fails.** `required = true` is the default. It fails
`keel doctor`, fails G0 and G2, and is stated in the projection body itself so
an agent reading only `CLAUDE.md` learns that rules it is supposedly held to did
not load. Shared content is hashed into the store hash, so a platform change
marks every consumer's projections stale.

## Consequences

A repository whose shared store has moved stops passing gates until it is fixed,
which is friction exactly proportional to how much the missing rules mattered.

The alternative — warn and continue — is worse in the specific way that matters
for governance: the rule stops applying while everyone downstream still believes
it is in force, and the gate that was supposed to notice is the one that
shrugged.
