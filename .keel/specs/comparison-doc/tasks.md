---
id: TASKS-0021
slug: comparison-doc
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 One table compares keel with six named tools on fixed rows
- criteria: AC-1
- files: docs/comparison.md, tests/comparison_doc.rs
- budget: 60
- exit: `cargo test --test comparison_doc the_table_has_the_fixed_columns_and_rows` exits 0

### T-2 Every claim about another tool cites a dated source
- criteria: AC-2
- files: docs/comparison.md, tests/comparison_doc.rs
- depends_on: T-1
- budget: 45
- exit: `cargo test --test comparison_doc every_other_tool_cell_footnotes_a_dated_https_source` exits 0

### T-3 Every claim about keel links to its evidence
- criteria: AC-3
- files: docs/comparison.md, tests/comparison_doc.rs
- depends_on: T-2
- budget: 45
- exit: `cargo test --test comparison_doc every_keel_claim_links_to_an_existing_adr_or_test` exits 0

### T-4 The page names where keel is weaker
- criteria: AC-4
- files: docs/comparison.md, tests/comparison_doc.rs
- depends_on: T-3
- budget: 45
- exit: `cargo test --test comparison_doc the_weaker_section_names_every_compared_tool` exits 0

### T-5 The README links to the comparison
- criteria: AC-5
- files: README.md, tests/comparison_doc.rs
- depends_on: T-4
- budget: 30
- exit: `cargo test --test comparison_doc readme_links_the_comparison_before_the_first_diagram` exits 0

### T-6 A newcomer can name what is specific to keel
- criteria: AC-6
- files: docs/comparison.md
- depends_on: T-5
- budget: 30
- exit: a person confirms AC-6 by reading only docs/comparison.md; recorded as a judgement once keel judge is available
