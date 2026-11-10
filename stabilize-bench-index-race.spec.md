<!-- moor:rules
Write this spec from what you want the change to do. This file is all an
AI assistant needs; it doesn't need to read the project's code. Then run
`moor spec push <this file>`. This block is removed when you push.

To pass its checks, the spec needs:
1. The front matter below, with `scope` listing the paths the change may
   touch (globs).
2. At most 12 acceptance criteria, each a `### AC-n <short title>` heading.
3. Each criterion states ONE requirement in exactly one of these forms:
     THE SYSTEM SHALL <response>
     WHEN <trigger> THE SYSTEM SHALL <response>
     WHILE <state> THE SYSTEM SHALL <response>
     IF <condition> THEN THE SYSTEM SHALL <response>
     WHERE <feature> THE SYSTEM SHALL <response>
   `THE SYSTEM SHALL` is upper case. Never write "should".
4. Each criterion is followed by at least one `oracle:` line:
     oracle: cmd `<shell command>` exit <code>
     oracle: test <test identifier>
     oracle: human <what a reviewer must judge>
   Prefer one that runs; use `human` only when nothing can.
5. No vague words in criteria (appropriate, efficient, robust, handle,
   support, reasonable, fast, should, etc). Name observable behaviour and
   concrete numbers instead.
6. The whole file stays under 250 lines.
-->
---
slug: stabilize-bench-index-race
status: draft
scope:
  - "tests/phase4.rs"
  - "Cargo.toml"
  - "Cargo.lock"
  - ".keel/keel.toml"
budget:
  criteria: 6
  lines: 120
---

# Stabilize the bench index race

## Context

`the_benchmark_reports_a_ratio_and_holds_the_target` in `tests/phase4.rs`
shells out to `keel bench --json` with its working directory set to keel's own
repository (`CARGO_MANIFEST_DIR`). `keel bench` (src/cmd/bench.rs) opens the
live, on-disk `.keel` index through `Retriever::open` and bails when that index
is degraded or when any query falls back to ripgrep — both of which happen
while the index is being rewritten.

Cargo runs integration tests multi-threaded by default. Other tests in the
suite exercise `keel map` and index reads against that same real repository, so
a concurrent rebuild overwrites the index mid-bench. The result is a test that
passes when run alone but fails under the full `cargo test` suite. That single
failure is what blocks the G2 `test` check for every spec gated in this
sandbox, including already-merged work such as `g1-usability` and
`spec-abandonment`.

The fix is to stop the bench test from running concurrently with any other test
that touches the shared repository index, by serializing it with the
`serial_test` crate. No product code changes: the benchmark and the index
behaviour are correct; only the test's concurrency with its siblings is wrong.

This change also switches the project's default driver in `.keel/keel.toml`
from `claude-code` to `copilot`, so that future agent-driven builds in this
sandbox use the credential the operator has wired up. The driver default is
configuration, not product code, and does not affect the test fix.

Finally, it clears the one failing `cargo audit` vulnerability — RUSTSEC-2026-0459,
`libsqlite3-sys 0.38.2` (fixed in `>= 0.39.0`) — which is pulled in only through
`rusqlite`. `rusqlite 0.40.2` pins `libsqlite3-sys 0.38.x`, so the fix bumps
both `rusqlite` entries in `Cargo.toml` to `0.41.1`, which admits
`libsqlite3-sys 0.39.1`. The upgrade compiles with no source change. This is
folded in here so the whole combined change lands as one PR with a green
`cargo audit`.

## Acceptance criteria

### AC-1 serial_test is a pinned dev-dependency

THE SYSTEM SHALL declare `serial_test` as a dev-dependency in `Cargo.toml`
pinned to an exact version.

oracle: cmd `grep -A20 '\[dev-dependencies\]' Cargo.toml | grep -qE 'serial_test = "[0-9]'` exit 0

### AC-2 The full test suite passes

WHEN the whole test suite runs THE SYSTEM SHALL exit zero with no failing test.

oracle: cmd `cargo test --quiet` exit 0

### AC-3 The suite stays green across repeated runs

WHEN `cargo test` runs three times in succession THE SYSTEM SHALL exit zero on
every run, so the race cannot resurface intermittently.

oracle: cmd `for i in 1 2 3; do cargo test --quiet || exit 1; done` exit 0

### AC-4 The benchmark test is annotated to run outside concurrent index writes

WHERE a test in `tests/phase4.rs` runs `keel bench` against `CARGO_MANIFEST_DIR`
THE SYSTEM SHALL annotate it with `#[serial]`, and SHALL leave tests that use
isolated temporary fixtures unannotated.

oracle: human The test invoking `keel bench` against CARGO_MANIFEST_DIR carries #[serial]; tests using isolated temp fixtures do not.

### AC-5 Copilot is the single default driver

THE SYSTEM SHALL mark exactly one driver in `.keel/keel.toml` as default, and
that driver SHALL be `copilot`.

oracle: cmd `test "$(keel driver list | grep -c 'default')" = "1" && keel driver list | grep copilot | grep -q default` exit 0

### AC-6 libsqlite3-sys is at or above the patched version

THE SYSTEM SHALL pin `libsqlite3-sys` to a version at or above `0.39.0`, by
declaring every `rusqlite` dependency in `Cargo.toml` at a version requirement
at or above `0.41.0`.

oracle: cmd `cargo tree -i libsqlite3-sys --depth 0 | grep -qE 'v0\.(39|[4-9][0-9])\.'` exit 0

## Out of scope

- Changing `keel bench` or `Retriever` so the benchmark builds its own private
  index. That is a larger behaviour change; this spec only removes the test
  concurrency that triggers the race.
- Making the whole integration suite single-threaded (`--test-threads=1`),
  which would slow every run to fix one test.
- Altering the 3× ratio target or the fixed benchmark task set.
- Touching `tests/phase1.rs` or any spec's product code.
- The `unsafe-libyaml` (RUSTSEC-2024-0449) and `tokio` (RUSTSEC-2026-0386)
  `cargo audit` findings. Both are allowed, non-failing warnings; clearing them
  is a separate decision.
- Asserting `cargo audit` exits zero as a spec oracle. That command fetches the
  advisory database over the network, which the sandbox denies by default, so a
  raw `cargo audit` oracle flakes on the fetch rather than on any real
  advisory. The deterministic invariant — `libsqlite3-sys >= 0.39.0` — is
  asserted by AC-6, and keel's own G2 `cargo-audit` check independently
  confirms no failing advisory.
