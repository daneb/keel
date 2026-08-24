---
id: ADR-0023
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-24
phase: 5
---

# Driver scripts ship embedded in the binary; `keel.toml` is edited by appending text, never by round-tripping `Config`

## Context

Every reference driver script — `claude-code`, `codex`, `copilot`, `kiro`,
`noop` — lived only in this repository's own `.keel/drivers/`, hand-authored
for keel's self-hosted use and excluded from the published crate by
`Cargo.toml`'s `exclude = [".keel/"]`. `keel init` never wrote them anywhere
else. `cargo install keel-harness` followed by `keel init` produced a config
pointing the *default* driver at `.keel/drivers/claude-code` — a file that
existed nowhere outside this repository. Found by a user on a fresh machine
who wanted `kiro`, on their first real attempt to use one.

## Decision

`assets/drivers/*` — outside `.keel/`, so it ships in the crate — is now the
single source of truth, embedded at compile time via `include_str!`
(`src/driver/builtin.rs`). `keel init` writes every script into
`.keel/drivers/` and registers all five as `[[driver]]` entries. For a
repository that already ran `init` before this existed, `keel init` correctly
refuses to touch `.keel/` again; `keel driver scaffold` is the narrower
recovery path, writing only what is missing.

The first implementation of that recovery path loaded the whole `Config`,
mutated `cfg.drivers`, and called `cfg.save()`. That round-trips through
`toml::to_string_pretty`, which has no memory of comments or section order —
`Config` does not store either. The first time someone ran a command whose
only stated job was "add a missing driver," it would have silently deleted
every comment in their `keel.toml` and rewritten every section in struct
field order. Caught before release by testing the exact scenario a real user
would hit — a hand-written comment, an already-initialised repository — and
diffing before against after rather than trusting a green build.

`scaffold` now appends raw `[[driver]]` TOML blocks as text and never touches
a byte it did not add. That change surfaced a second, narrower bug: an empty
`drivers: Vec<Driver>` was serializing as `driver = []`, which collides with
an appended `[[driver]]` block under the same key and fails to parse. Fixed
by skipping serialization of an empty list, consistent with how every other
optional TOML section in this file already behaves.

## Consequences

`Config` is no longer a safe way to *edit* `keel.toml` incrementally from
code — only to read it, and to write it once, whole, when nothing human-authored
exists yet to lose (a brand-new file). Any future command that adds to an
existing config, rather than replacing it outright, needs the same
append-as-text discipline `scaffold` now uses, not a load-mutate-save round
trip. That is a sharper constraint than most Rust config-handling code
assumes, and it is easy to reach for `cfg.save()` again out of habit; there is
no compiler check that stops it.
