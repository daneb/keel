---
id: SPEC-0009
slug: test-movement-docs-aware
schema: keel.spec/1
status: approved
scope:
- src/gate/g25.rs
- .gitignore
budget:
  criteria: 8
  lines: 200
verified_at: 2026-09-23
---

# G2.5 test-movement counts only testable code

## Context

The G2.5 `test-movement` heuristic in `src/gate/g25.rs` classifies every
changed file that is neither a test (`crate::map::rank::is_test`) nor
incidental (`crate::gate::g2::is_incidental_for`) as "code", then blocks when
any such file changed with no test file alongside it. Its stated purpose is to
confirm that a criterion's oracles actually exercise the code that changed.

A documentation-only change (`README.md`, `GETTING-STARTED.md`, a design doc)
is neither a test nor incidental, so it is counted as code. A change that
touches only prose therefore blocks G2.5 — and through it G3 — with the message
"code file(s) changed and no test file did", even though there is no code to
exercise and no test could meaningfully move. Unlike `test-invalidation`, this
verdict is not cleared by a `--stage review` approval, has no `keel.toml`
override, and no per-check flag on `keel approve`, so an all-green,
fully-approved docs change stays permanently `blocked`.

The premise of the heuristic — "code changed, was it tested?" — is meaningless
for a change that contains no code. The fix is to count only testable source
files as code, so a change whose substantive files are all documentation does
not demand a moved test.

The heuristic has a second blind spot with the same root: `is_test` recognises
only path conventions (`tests/`, `_test.go`, `.spec.ts`, …). Rust's dominant
convention — inline `#[cfg(test)]` modules inside the source file — never moves
a separate test file, so a source change that adds `#[test]` functions is
counted as untested code and blocks. keel is written this way throughout and is
developed inside itself, so nearly every legitimate source change trips this. A
changed source file whose added lines introduce a `#[test]` or `#[cfg(test)]`
attribute is tested by definition and must count as test movement.

## Acceptance criteria

Each criterion is one EARS sentence plus at least one runnable oracle.

### AC-1 A docs-only change does not block test-movement

WHEN the changed files in a diff are all documentation and none are source
code or test files THE SYSTEM SHALL return a `test-movement` verdict of `pass`.

oracle: cmd `cargo test --bin keel gate::g25::tests::docs_only_change_does_not_block_test_movement` exit 0

### AC-2 Code changed with no test still blocks

WHEN a diff changes at least one source-code file and no test file THE SYSTEM
SHALL return a `test-movement` verdict of `blocked`.

oracle: cmd `cargo test --bin keel gate::g25::tests::code_without_tests_is_flagged_for_a_look` exit 0

### AC-3 Docs alongside code do not mask the missing test

WHEN a diff changes at least one source-code file and a documentation file but
no test file THE SYSTEM SHALL return a `test-movement` verdict of `blocked`.

oracle: cmd `cargo test --bin keel gate::g25::tests::docs_do_not_mask_a_missing_test` exit 0

### AC-4 Code changed with a test present still passes

WHEN a diff changes a source-code file and a test file THE SYSTEM SHALL return
a `test-movement` verdict of `pass`.

oracle: cmd `cargo test --bin keel gate::g25::tests::code_with_tests_passes` exit 0

### AC-5 A source file that adds inline tests counts as test movement

WHEN a diff changes a source-code file whose added lines introduce a `#[test]`
or `#[cfg(test)]` attribute and no separate test file is changed THE SYSTEM
SHALL return a `test-movement` verdict of `pass`.

oracle: cmd `cargo test --bin keel gate::g25::tests::inline_rust_tests_count_as_test_movement` exit 0

## Out of scope

- Any change to the `test-invalidation` heuristic, its approval binding, or the
  mocking/weakened-assertion vocabularies.
- Adding a `keel approve` per-check override flag or a `keel.toml` dismissal for
  `test-movement`; this fix removes the false positive at its source instead of
  adding a way to wave it through.
- Reclassifying what counts as documentation anywhere other than inside
  `test_movement`; the ranking helpers `is_test` and `is_entry_point` are
  untouched.
- Config files (`*.toml`, `*.yaml`, `*.json`) that are neither docs nor source:
  their existing treatment as substantive is unchanged.

## Note on scope

`.gitignore` is in scope only to add one line ignoring `.serena/`, the Serena
MCP's local per-project workspace, so it stops appearing in the gated diff as
untracked noise. It carries no behaviour and has no oracle; it is declared here
so the blast-radius check stays honest rather than being waved through.
