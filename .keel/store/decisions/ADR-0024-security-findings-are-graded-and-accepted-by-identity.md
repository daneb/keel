---
id: ADR-0024
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-25
phase: field
---

# Security findings carry a grade beside their severity, and an acceptance binds to finding identity rather than to prose

## Context

G2.5 reviewed a diff for four things — test-invalidation, scope creep,
convention breaches, missing coverage — and nothing security-related. keel
exists to put coding agents to work, and the literature is consistent that
AI-generated code carries security defects at a meaningful rate, so the gate
that reviews agent output was silent on the failure mode most likely to matter.

Two obstacles. `keel.reviewresult/1` models severity as `fail | concern`, a
binary with no room for "how bad", and the spine is frozen additive-only, so
that enum cannot be redefined. And a finding produced by a language model is
worded differently every run, which defeats the hash-binding that makes every
other approval in keel trustworthy.

## Decision

**A grade sits beside severity rather than replacing it.** `severity` answers
"does this block" — policy. `grade` (`critical`/`high`/`medium`/`low`) answers
"how dangerous is it" — assessment. They are different axes, and conflating
them is why security checks tend to end up either all-blocking or all-advisory:
one flag cannot distinguish a hardcoded credential from a missing hardening
comment. `grade` is an optional field, which is additive and therefore
permitted under the freeze; a reviewer that has never heard of it still parses,
and there is a test that says so.

`critical` and `high` fail G2.5. `medium` and `low` are recorded and do not
block.

**An acceptance binds to finding identity, not to the reviewer's prose.**
`security-findings.json` records the sorted set of `(id, grade, file, line)`
tuples and nothing else; `keel approve --stage security` hashes that. The
detail text stays in the run's `review.json`, which is evidence rather than a
hash target.

The alternative — hashing the findings as the reviewer wrote them — was
rejected in both directions it fails. A model rewording the same defect would
supersede a decision already made, training people to re-approve reflexively
until they stop reading. And two genuinely different defects at the same
location could produce similar prose, letting an old acceptance cover a new
finding, which is the exact failure the hash binding exists to prevent.

## Consequences

Identity is `(id, grade, file, line)`, so **a different defect of the same
category at the same line is indistinguishable from the accepted one**. A
`crypto` finding at `foo.rs:42` accepted today would cover a different `crypto`
finding at `foo.rs:42` tomorrow. Line numbers move when code around them
changes, which makes this narrow in practice — an edit near the finding
supersedes the acceptance anyway — but it is a real gap and prose was the worse
trade.

Grades come from a model and are therefore not calibrated. An over-graded
finding costs a person's attention; spending it twice on nothing is how a check
gets switched off. The prompt says so directly, `advisory = true` exists for
calibrating on an existing codebase, and the honest mitigation is that a human
still decides.

This layer is a model reading a diff. It finds what a careful reviewer would
find and misses what they would miss. A deterministic scanner over changed
files is a separate, complementary layer and is not built yet.
