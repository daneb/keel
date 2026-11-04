---
id: SPEC-0020
slug: oracle-sensitivity
schema: keel.spec/1
status: approved
scope:
- src/gate/g2.rs
- src/gate/oracle_exec.rs
- src/spec/oracle.rs
- src/worktree.rs
- src/config.rs
- schemas/mutationresult.json
- assets/mutation/cargo-mutants
- src/driver/builtin.rs
- tests/fixtures/mutation/**
- tests/oracle_sensitivity.rs
- tests/support.rs
- tests/cover.rs
budget:
  criteria: 8
  lines: 540
verified_at: 2026-11-03
---

# An oracle must fail without the change it claims to verify

## Context

G0 proves every criterion has an oracle and that the oracle parses. G2 proves
the oracle passes. Neither proves the oracle tests anything: `assert!(true)`,
a test that checks the wrong function, or a `cmd` oracle whose command
succeeds on any tree all pass both gates. An agent that writes both the code
and its tests has every incentive to write oracles like that, and G2.5's
mock and test-movement heuristics do not look for them.

Two mechanisms, in order of cost:

1. **Sensitivity**, native to keel and language-agnostic. Re-run each
   executable oracle against the base commit with only the run's test files
   carried over. An oracle that still passes there was passing before the
   implementation existed, so it does not verify the implementation. This is
   TDD's red step, checked after the fact instead of trusted.
2. **Mutation**, through a plugin. Mutate the changed lines and report which
   mutants no oracle kills. Advisory by default, following ADR-0017: an
   uncalibrated check that blocks gets routed around.

Some criteria are guards: "the tabs remain where they are" holds before the
change and must still hold after. Those are declared, not failed.

## Acceptance criteria

### AC-1 Executable oracles are re-run at the base commit

WHEN every executable oracle passes in G2 THE SYSTEM SHALL create a worktree
at the run's base commit, copy in the files from the run's diff that match
`[oracle] sensitivity_paths` (default `tests/**`), run each `cmd`, `test` and
`doctest` oracle there, and write the outcomes to `oracle-sensitivity.json`
in the run's evidence.

oracle: cmd `cargo test --test oracle_sensitivity executable_oracles_are_rerun_in_a_base_worktree_with_test_files_carried_over` exit 0

### AC-2 An oracle that passes at the base fails the check

IF an oracle not marked as a guard passes at the base commit THEN THE SYSTEM
SHALL fail `G2/oracle-sensitivity` and name each such criterion and its
oracle.

oracle: cmd `cargo test --test oracle_sensitivity an_oracle_passing_without_the_change_fails_sensitivity` exit 0

### AC-3 Guard criteria must hold before and after

WHERE a criterion's oracle is written `oracle: guard <oracle>` THE SYSTEM
SHALL require that oracle to pass both at the base commit and on the change,
and SHALL fail `G2/oracle-sensitivity` when it fails at the base.

oracle: cmd `cargo test --test oracle_sensitivity a_guard_oracle_must_pass_at_the_base_and_on_the_change` exit 0

### AC-4 A sensitivity run that cannot complete is blocked

IF the base worktree cannot be created, the build fails there, or an oracle
cannot execute there THEN THE SYSTEM SHALL report `G2/oracle-sensitivity` as
`blocked` with the reason, and never as `pass`.

oracle: cmd `cargo test --test oracle_sensitivity an_unbuildable_base_blocks_sensitivity_rather_than_passing_it` exit 0

### AC-5 Oracles keel cannot isolate are counted, not passed

WHEN an oracle is `human` or `schema`, or its test lives in a file outside
`sensitivity_paths` that the diff also changes, THE SYSTEM SHALL list it as
`not assessed` with the reason in the check's detail and in
`oracle-sensitivity.json`, and SHALL not count it as sensitive.

oracle: cmd `cargo test --test oracle_sensitivity oracles_keel_cannot_isolate_are_listed_as_not_assessed` exit 0

### AC-6 A configured mutation tool reports surviving mutants

WHERE `[mutation] cmd` is configured THE SYSTEM SHALL run it after
sensitivity passes, read its stdout as `keel.mutationresult/1` (killed and
survived mutants, each with file and line), and add one finding per survived
mutant inside the spec's scope to `G2/mutation`.

oracle: cmd `cargo test --test oracle_sensitivity survived_mutants_in_scope_become_mutation_findings` exit 0
oracle: schema schemas/mutationresult.json validates tests/fixtures/mutation/cargo-mutants-sample.json

### AC-7 Mutation findings are advisory until configured to block

WHILE `[mutation] advisory` is unset or `true` THE SYSTEM SHALL report
`G2/mutation` findings without failing G2, and WHERE it is `false` a
survived mutant SHALL fail G2.

oracle: cmd `cargo test --test oracle_sensitivity mutation_is_advisory_by_default_and_blocks_only_when_configured` exit 0

### AC-8 A cargo-mutants adapter ships with keel

WHEN `keel init` runs in a Cargo repository THE SYSTEM SHALL write the
embedded `.keel/mutation/cargo-mutants` adapter, which runs `cargo mutants --in-diff` on the
run's diff and prints `keel.mutationresult/1`, and SHALL report `blocked` if
`cargo-mutants` is absent.

oracle: cmd `cargo test --test oracle_sensitivity the_cargo_mutants_adapter_emits_mutationresult_and_blocks_when_absent` exit 0

## Existing fixtures

The demo spec that `tests/support.rs` installs, and the tests that run it
with the no-op driver (`tests/cover.rs` and the published-schema test), use
oracles that hold before and after any change. They are tagged
`oracle: guard` as part of this change; no other test file is edited.

## Out of scope

- Adapters for mutmut, Stryker or PIT. The `keel.mutationresult/1` contract
  is the extension point; each adapter is written when a repository needs it.
- Making mutation blocking by default. That is a decision for after
  `keel metrics` shows its false-positive rate across real runs.
- Inline `#[cfg(test)]` modules in changed `src/` files. AC-5 reports them as
  not assessed; isolating them needs a language-aware splitter.

## Decision to record

An ADR, "An oracle must fail without its change", recording why sensitivity is
native and blocking while mutation is a plugin and advisory.
