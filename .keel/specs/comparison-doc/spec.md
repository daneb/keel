---
id: SPEC-0021
slug: comparison-doc
schema: keel.spec/1
status: approved
scope:
- docs/comparison.md
- README.md
- tests/comparison_doc.rs
budget:
  criteria: 6
  lines: 260
verified_at: 2026-11-03
---

# Say plainly why keel, and where something else is the better choice

## Context

README.md and the docs site say what keel does. Neither says how it differs
from the tools a reader already knows: GitHub spec-kit, Kiro, BMAD, Tessl,
Google's Elephant-Goldfish model, and Claude Code hooks. A newcomer cannot
see the properties that are specific to keel, such as the three-valued
verdict, hash-bound approvals, falsifiable criteria and offline-verifiable
evidence, without reading PLAN.md and the ADRs.

A comparison page goes stale quickly and invites marketing. This spec
constrains both: every claim about another tool cites a source with a date,
every claim about keel links to the ADR or test that backs it, and the page
says where keel is weaker.

## Acceptance criteria

### AC-1 One table compares keel with six named tools on fixed rows

THE SYSTEM SHALL provide `docs/comparison.md` with a table whose columns are
keel, spec-kit, Kiro, BMAD, Tessl, Elephant-Goldfish and Claude Code hooks,
and whose rows are exactly: falsifiable criteria, blocked as a verdict,
approvals bound to content, tamper-evident evidence, computed blast radius,
test-tampering review, lessons enforced as checks, and agent drivers.

oracle: cmd `cargo test --test comparison_doc the_table_has_the_fixed_columns_and_rows` exit 0

### AC-2 Every claim about another tool cites a dated source

WHEN a cell describes a tool other than keel THE SYSTEM SHALL footnote it to
an entry in a `## Sources` section that holds an `https` link and a
`checked: YYYY-MM-DD` date.

oracle: cmd `cargo test --test comparison_doc every_other_tool_cell_footnotes_a_dated_https_source` exit 0

### AC-3 Every claim about keel links to its evidence

WHEN a keel cell claims a property THE SYSTEM SHALL link it to a file under
`.keel/store/decisions/` or `tests/` that exists in the repository.

oracle: cmd `cargo test --test comparison_doc every_keel_claim_links_to_an_existing_adr_or_test` exit 0

### AC-4 The page names where keel is weaker

THE SYSTEM SHALL include a `## Where keel is weaker` section with at least
one entry per compared tool, each naming the tool and a situation in which
a team would choose that tool over keel.

oracle: cmd `cargo test --test comparison_doc the_weaker_section_names_every_compared_tool` exit 0

### AC-5 The README links to the comparison

THE SYSTEM SHALL link `docs/comparison.md` from README.md's opening section,
before the first diagram.

oracle: cmd `cargo test --test comparison_doc readme_links_the_comparison_before_the_first_diagram` exit 0

### AC-6 A newcomer can name what is specific to keel

WHEN a reader who has not used keel reads only `docs/comparison.md` THE
SYSTEM SHALL have given them enough to name three properties that the other
six tools lack.

oracle: human a reader new to keel, reading only docs/comparison.md, names three properties specific to keel and each matches a row where keel is the only tool marked

## Out of scope

- Benchmarks against the other tools. No shared task set exists, and an
  unshared one would be tuned to flatter keel (ADR-0012).
- Automatic staleness checks on the `checked:` dates. A date older than six
  months is visible to the reader; a CI check that fails on the calendar
  is its own decision.
- Publishing the page to the docs site. That follows once the page exists.
