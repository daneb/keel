---
id: SPEC-0012
slug: keel-cover
schema: keel.spec/1
status: approved
scope:
- src/cover/**
- src/cmd/cover.rs
- src/cmd/mod.rs
- src/main.rs
- src/gate/g2.rs
- src/gate/diff.rs
- action.yml
- schemas/**
- tests/cover.rs
budget:
  criteria: 8
  lines: 650
verified_at: 2026-09-25
---

# A PR check: every change covered by a verified bundle

## Context

A bundle proves what one run did (SPEC-0011), but nothing yet stops a change
reaching `master` without one. Roadmap R10 asks for a PR check: fail when a
PR's change has no valid bundle. That's the piece an outside team adopts first,
and it needs nothing but keel: no runtime, no Moor.

The design decisions:
- **Every commit needs cover, not only agent-marked ones.** A trailer can be
  deleted, and a check that can be dodged that way checks nothing.
- **Bundles are committed in the PR** under `.keel/bundles/`, so the check
  runs offline, with no artifact storage.

Commit SHAs can't link a run to a PR: a driver's work isn't committed when it's
gated. The link is the **content of the gated tree**. G2 records a hash of the
tree it judged. The PR's head is covered when a committed bundle verifies, its
run passed, and its hash equals the head's. The hash leaves out `.keel/**`,
which committing the bundle itself changes, and keel's rendered projections,
which `store-drift` already checks. It keeps lockfiles in, because a dependency
change is exactly what has to be covered.

## Acceptance criteria

### AC-1 G2 records the tree it judged

WHEN G2 runs THE SYSTEM SHALL write the content hash of the working tree it
judged to the run's evidence as `tree.txt`.

oracle: test tests/cover.rs::g2_records_the_tree_it_judged

### AC-2 The content hash ignores where the bytes live

THE SYSTEM SHALL compute the same content hash for the same files whether they
are committed, staged or untracked. The hash leaves out `.keel/**`, gitignored
files and keel's rendered projections, and covers every other path, lockfiles
included.

oracle: test tests/cover.rs::content_hash_is_the_same_committed_or_not

### AC-3 A covered head passes

WHEN `keel cover` runs and a bundle under `.keel/bundles/` passes `keel bundle
verify`, records a `pass` run verdict and a tree hash equal to the head's THE
SYSTEM SHALL exit 0 and name that bundle.

oracle: test tests/cover.rs::a_verified_bundle_of_this_tree_covers_it

### AC-4 An uncovered head fails, saying why

IF no bundle under `.keel/bundles/` covers the head THEN THE SYSTEM SHALL exit
1, print the head's content hash, and give each bundle's reason: failed
verification, a verdict other than pass, or a different tree.

oracle: test tests/cover.rs::an_uncovered_change_names_each_reason

### AC-5 A change after the run is not covered

IF the tree changes after a bundle's run, including a lockfile THEN THE
SYSTEM SHALL report that bundle as covering a different tree.

oracle: test tests/cover.rs::an_edit_after_the_run_is_not_covered

### AC-6 An exemption is a stated human decision

WHERE `keel cover --exempt <reason>` is given THE SYSTEM SHALL exit 0 and print
that the head is exempted, not covered, with the reason.

oracle: test tests/cover.rs::an_exemption_passes_as_exempted_not_covered

### AC-7 The report is machine-readable

WHEN `keel cover --json` runs THE SYSTEM SHALL print one `keel.cover/1` object
with the head hash, the verdict (`covered`, `uncovered` or `exempted`), and
each bundle considered with its reason.

oracle: test tests/cover.rs::json_report_validates_against_the_published_schema

### AC-8 A GitHub Action runs it on pull requests

THE SYSTEM SHALL ship `action.yml`, a composite action that checks out the
PR's head, installs a pinned keel version, runs `keel cover`, and passes
`--exempt` with the PR's `keel:exempt` label as the reason when that label is
set.

oracle: test tests/cover.rs::the_action_runs_keel_cover_on_the_pr_head

## Out of scope

- Changing `keel init`'s `.gitignore`, which still ignores `.keel/bundles/`.
  Committing a bundle takes `git add -f` for now.
- Covering each commit separately. The head's tree is what merges, and every
  commit in the PR contributes to it.
- A bundle built by CI itself (the GitHub Actions runtime).
- Branch protection settings, which are the repository owner's to set.
