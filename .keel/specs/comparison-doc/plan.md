---
id: PLAN-0021
slug: comparison-doc
schema: keel.plan/1
blast:
  depth: 2
  declared:
  - docs/comparison.md
  - README.md
  - tests/comparison_doc.rs
  computed: []
  computed_lines: 0
rollback: git revert
verified_at: 2026-11-04
---

# Design — comparison-doc

## Approach

This is a documentation change with one test; no keel source is touched. The
seam is the page's own structure, checked by `tests/comparison_doc.rs`, which
reads `docs/comparison.md` and asserts the fixed columns and rows, that each
cell about another tool carries a footnote resolving to a `## Sources` entry
(an `https` link and a `checked: YYYY-MM-DD` date), that each keel cell links
to a file that exists under `.keel/store/decisions/` or `tests/`, and that
`## Where keel is weaker` names every compared tool. A second assertion reads
`README.md` and checks the link to the comparison comes before the first
diagram.

`docs/comparison.md` is written from the `## Source notes` section below, in
the build step, which has no web access. The writing rule there governs every
cell about another tool: say only what the source states, otherwise write
"Not stated in the source checked" with a footnote.

Tasks are chained because they all edit `docs/comparison.md` and
`tests/comparison_doc.rs`; `README.md` is touched only by the README task.
T-6 is a human oracle and has no code of its own.

## Source notes

Researched on the host on 2026-11-04 from primary pages. Verbatim:

```text
WRITING RULE for the build: a cell about another tool may only say what the source states. Where the source does not state a property, write 'Not stated in the source checked' and footnote it; NEVER write 'No' or 'None' or imply the tool lacks the property. Do not use any fact that is not in these notes.

SOURCES (all checked: 2026-11-04):
[1] spec-kit https://github.com/github/spec-kit and https://github.com/github/spec-kit/blob/main/spec-driven.md
[2] Kiro https://kiro.dev/docs/specs/
[3] BMAD https://github.com/bmad-code-org/BMAD-METHOD
[4] Tessl https://docs.tessl.io/
[5] Elephant-Goldfish https://opensource.googleblog.com/2026/10/introducing-elephant-goldfish-model-plugin.html (published 2026-10-22; repo https://github.com/google/elephant-goldfish-model)
[6] Claude Code hooks https://code.claude.com/docs/en/hooks

FINDINGS PER ROW (S=stated by source, NS=not stated):
Falsifiable criteria: spec-kit S (spec-driven.md checklists require requirements to be testable and unambiguous); Kiro S partly (specs hold user stories with acceptance criteria; falsifiability not stated); BMAD NS; Tessl NS (evals of a skill's impact on tasks are mentioned, not spec acceptance criteria); EGM NS (Goldfish does a go/no-go check of paths and signatures in a design doc); hooks NS (a hook can return a reason such as a test suite must pass, but hooks do not define criteria).
Blocked as a verdict: hooks S (exit code 2 blocks the action; permissionDecision deny is a separate block path); spec-kit NS (verdicts shown are verified/partial/failed, go/needs-clarification/kill, converged); Kiro NS (task status in-progress or completed); BMAD NS; Tessl NS; EGM NS (an ambiguous document requires further drafting).
Approvals bound to content: NS for all six tools in the sources checked. Kiro mentions a Review button and comments, no hashing.
Tamper-evident evidence: NS for all six. Tessl mentions a full audit trail without saying tamper-evident. spec-kit saves reports to project folders.
Computed blast radius: NS for all six. spec-kit mentions consistency analysis; EGM's design doc enumerates files to change; spec-driven.md says affected plans are flagged when requirements change; Kiro says bugfix specs validate that nothing else breaks. None says it is computed.
Test-tampering review: NS for all six. spec-kit has a bug-test step and spec-driven.md says tests must be approved before implementation.
Lessons enforced as checks: NS for all six. Tessl says it can mine agent logs for recurring mistakes; spec-kit presets can enforce organisational standards; neither is stated to turn lessons into checks.
Agent drivers: spec-kit names GitHub Copilot, others generic 'integrations'; Kiro NS; BMAD names a Claude Code plugin and a Codex plugin and says any AI tool that supports skills works; Tessl says agent-agnostic, no names; EGM names Antigravity and says other environments are coming; hooks run wherever Claude Code runs (CLI, Agent SDK, plugins).

WHERE KEEL IS WEAKER (ground each only in the above): hooks = immediate per-tool-call blocking across 33 events with no spec or plan stage; spec-kit = lightweight spec scaffolding with integrations for many agents; Kiro = an IDE with in-product spec review and commenting; BMAD = role-based agile planning for whole projects; Tessl = registry of library usage specs; Elephant-Goldfish = fresh-context review of a design doc before implementation, which keel does not do; also note this comparison covers only what primary pages state and that every 'not stated' may be a gap in the page, not in the tool.
For keel cells link only to files that exist: .keel/store/decisions/ADR-*.md (verify with ls) and tests/*.rs including tests/oracle_sensitivity.rs, tests/driver_usage.rs, tests/g3_judgement.rs. Do not link an ADR that does not exist.
```

ADR files present when this plan was written: ADR-0000 to ADR-0025 under
`.keel/store/decisions/`, named `ADR-NNNN-<slug>.md`. Link by full file name.

## Blast radius

<!-- generated by `keel plan`; edits here are overwritten -->

Computed from the import graph at depth 2: **0 files, 0 lines**.

_No indexed file matches the declared scope. If these are new files, that is
expected; if not, the scope globs are wrong._


## Rollback

`git revert` — replace this if the change needs a different rollback.
