# Changelog

Notable changes to keel. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and keel uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

Pre-1.0 means the command surface may still move. The wire schemas are frozen
and additive-only — see *Spine freeze* in [ROADMAP.md](ROADMAP.md).

## [0.4.0] - 2026-08-25

### Added

- **Security review at G2.5.** The adversarial reviewer now also grades
  findings for security defects — injection, authz, crypto misuse, secret
  exposure, unsafe input handling, resource exhaustion — judging only the
  added and modified lines. `critical`/`high` fail the gate; `medium`/`low`
  are recorded without blocking. Grade and severity are separate fields:
  severity decides whether a finding blocks, grade decides how dangerous it
  is, so the check can distinguish a hardcoded credential from a missing
  hardening comment rather than collapsing both into one flag.
- **`[[review]]` is an array.** Configure as many reviewers as you want — a
  model, a static scanner, a house-style script — each answering the same
  `keel.reviewrequest/1` → `keel.reviewresult/1` contract. A reviewer's `id`
  names its check, prefixes its findings, and names its evidence file, so a
  third reviewer is a config entry, never a change to keel.
- **A semgrep adapter** ships embedded (`keel init` writes it to
  `.keel/sast/`), scanning only the files a diff touches. keel ships no
  scanner — semgrep is yours to install; its absence reports `blocked`,
  never a silent pass.
- **`keel approve --stage security <slug>`** accepts graded findings
  deliberately, bound to the SHA-256 of their identity (category, grade,
  file, line) rather than the reviewer's prose — a reworded finding keeps
  the acceptance, a genuinely new one supersedes it.
- **A secrets gate check** (`.keel/checks/secrets`, gitleaks) scans git
  history and the uncommitted diff, because `keel run` commits build/test/
  lint/driver output as evidence, and a secret any of those commands prints
  becomes a committed artefact.
- **A `cargo-audit` gate check** — a known RustSec advisory fails G2.
- **CI** — build/test/clippy, a weekly advisory re-check, and a secrets scan,
  all invoking the same plugins the local gates run.
- **`keel driver scaffold`** writes missing reference driver scripts (and now
  the SAST adapter) into an already-initialised repository, where `keel init`
  correctly refuses to run again.
- **SECURITY.md** — the threat model this project had but had never written
  down, most importantly that `.keel/keel.toml` is executable code.

### Fixed

- **`keel plan` no longer runs against a spec G0 has rejected.** It had no
  check at all beyond "scope is non-empty" — `keel run` already refused
  without a passing G1 for the equivalent reason.
- **A driver timeout now holds on Linux.** `kill_group` shelled out to
  `kill -KILL -<pgid>`, which BSD's `kill` accepts and procps-ng does not;
  the group kill silently did nothing on every CI runner. Now a direct
  `libc::kill`. The timeout also no longer depends on that kill succeeding:
  reader threads return over a channel with a bounded drain instead of an
  unbounded `join`.
- **A reviewer's exit code was ignored.** A reviewer reporting failure
  (e.g. "the scanner is not installed") alongside an empty findings list
  read as a clean pass. A non-zero exit is now `blocked`.
- The seeded config now says where build/test/lint go, and a config parse
  error naming a Rust struct carries a plain-language hint.

### Changed

- **Breaking:** `[review]` as a table no longer parses; it is `[[review]]`,
  an array. Pre-1.0; the error names the fix.

## [0.3.2] - 2026-08-24

### Fixed

- **Every driver but a hand-authored one was unusable on a fresh install.**
  The reference scripts for `claude-code`, `codex`, `copilot`, `kiro` and
  `noop` lived only in this repository's own `.keel/drivers/`, excluded from
  the published crate. `cargo install keel-harness` followed by `keel init`
  produced a config pointing the *default* driver at a script that existed
  nowhere. The scripts now ship embedded in the binary (`assets/drivers/*`)
  and `keel init` writes them into `.keel/drivers/`, registering all five.
  For a repository already initialised before this fix, `keel driver
  scaffold` is the recovery path — it writes what is missing without
  touching a script you have already edited.
- `keel driver scaffold` edits `keel.toml` by appending text, never by
  reloading and re-saving the whole config — an earlier version of this fix
  round-tripped through `Config`, which does not remember comments or
  section order, and would have silently deleted every comment in the file
  the first time someone ran a command whose only job was adding a driver.
- An empty `[[driver]]` list no longer serializes as `driver = []`, which
  collided with the very first driver block `scaffold` tried to append and
  failed to parse.

### Added

- `keel driver scaffold [--force]` — write missing reference driver scripts
  and register missing `[[driver]]` entries on an already-initialised
  repository, where `keel init` correctly refuses to run again.
- GETTING-STARTED.md documents drivers other than Claude Code and shared
  stores (`[[shared]]`) for the first time, both re-verified end to end
  before being written down rather than assumed correct from the design.

## [0.3.1] - 2026-08-24

Found by putting keel to real work on a second project's real branch, not
just gating uncommitted diffs against itself.

### Fixed

- **`--no-driver` no longer gates an empty diff.** Gating an already-committed
  branch diffed HEAD against HEAD, so blast-radius and line-budget silently
  passed on nothing. It now diffs from the merge-base with the repository's
  trunk; `--base <ref>` overrides. A driver run is unaffected.
- **`reviewable-size`'s file count matched its own line count.** The fail
  message was counting keel's own untracked run evidence as part of the
  diff under review.
- **`keel init` no longer seeds Rust commands into every stack.** `cargo test`
  was the default oracle regardless of what `init` detected. `Config::for_stack`
  seeds from the detected stack; an unrecognised one gets blank commands
  (which report `blocked`, honestly) rather than a wrong guess. Bun's oracle
  guards the same `bun test --test-name-pattern` vacuous-match hole the Rust
  default was written to avoid; Go and pytest get the same treatment.
- **`about` is only ambiguous when it quantifies.** G0 rejected "its
  assessment about coaching" as hedging; a preposition decides nothing.
- The seeded config now says where `build`/`test`/`lint` go, and a config
  parse error naming a Rust struct now carries a plain-language hint.

### Added

- **`keel gate g2`** — G2, G2.5 and G3 judge a change rather than a document,
  so they only ran inside `keel run`. `--no-driver` already did this; the
  command a person would actually type just failed with "unrecognized
  subcommand" and named no alternative. `g2.5` and `g3` are aliases.
- **`keel approve --stage review <slug>`** clears G2.5's test-invalidation
  block once a human has looked at the flagged lines, bound to their SHA-256
  — a newly added mock supersedes the acknowledgement rather than inheriting
  it. Previously a blocked check stayed blocked on every subsequent run with
  no way to record that someone had examined it.
- Bun is detected as its own stack (`bun.lock`/`bun.lockb`), not folded into
  npm.

## [0.3.0] - 2026-08-23

### Added

- **C# is indexed.** `.cs` files yield namespaces, classes, interfaces,
  structs, enums, records, delegates, methods, constructors and properties,
  and `using` directives become import edges so blast radius reaches through
  them. `keel init` recognises a `.sln` or `.csproj` and records `C# (.NET)`.

  A `using` names a namespace rather than a file, so it resolves to one
  representative file in that namespace's directory. Blast radius through a
  `using` is therefore a floor, not a ceiling.

## [0.2.2] - 2026-08-23

Documentation only. No change to keel's behaviour.

### Fixed

- The README diagrams are legible in both GitHub themes. The architecture
  diagram filled its subgraphs near-white and the sequence diagram pinned
  `theme:base` with near-black message text — each correct in exactly one
  theme, and the sequence messages were invisible on a dark page. Node text
  now sits on opaque fills whose contrast no page theme can affect; anything
  drawn on the page background inherits GitHub's theme instead of fixing a
  colour. Both were rendered at `-t default` and `-t dark` and read before
  landing.
- The install instructions say `cargo install keel-harness`. Version 0.2.1
  was published before that change, so the crates.io page for it still
  recommends installing from git.

## [0.2.1] - 2026-08-23

Packaging only. No functional change to keel itself.

### Added

- Dual MIT / Apache-2.0 licensing, with `LICENSE-MIT` and `LICENSE-APACHE`.
  The repository previously carried no licence at all, which left it
  all-rights-reserved by default.
- Package metadata crates.io requires: description, licence, repository,
  homepage, documentation, keywords and categories.

### Changed

- The published package is named **`keel-harness`**. Both `keel` and
  `keel-cli` were already taken on crates.io by unrelated projects. The
  installed binary is still `keel` — only the package name differs:

  ```sh
  cargo install keel-harness   # installs a binary called `keel`
  ```

### Fixed

- `.keel/`, `docs/` and `target/` are excluded from the package. keel runs
  itself, so `.keel/` held 242 run and evidence files that were being swept
  into the crate — 345 files packaged before, 103 after.

## [0.2.0] - 2026-08-23

First public release. All five phases of [PLAN.md](PLAN.md) are built.

### Added

- **Knowledge.** A durable `.keel/store/` and rendered projections for each
  agent (`CLAUDE.md`, `AGENTS.md`). Two hashes per projection: a `store=` hash
  that goes stale when the source moves, and a `body=` hash that refuses on
  drift rather than silently overwriting a human edit.
- **Structure.** A tree-sitter symbol index over 8 languages with retrieval by
  outline and symbol. 5,705 files index in 1.25 s; retrieval measures 14.6×
  less context at 100% recall on keel's own benchmark.
- **Artefacts.** Spec, plan and tasks as reviewable files. Requirements use
  EARS; every acceptance criterion carries a falsifiable `oracle:` — one of
  `cmd`, `test`, `schema`, `doctest` or `human`.
- **Gates.** G0 through G4, each returning one of three verdicts: `pass`
  (exit 0), `fail` (exit 1), or `blocked` (exit 3). `blocked` never silently
  passes and is never counted as an agentic failure.
- **Evidence.** A trajectory event stream per run and an exportable bundle;
  `keel export --verify` re-checks a bundle against its manifest.
- **Learning.** Failure episodes are classified and recurring ones are promoted
  to lesson cards. Promotion needs two occurrences in *distinct runs* and a
  human decision at G4. Lessons that carry an oracle become gate checks rather
  than injected context.
- **Approvals** bound to the SHA-256 of the artefact they approve, so editing a
  signed-off spec supersedes the sign-off instead of quietly inheriting it.
- **Blast radius** computed from the import graph and recomputed at G1, so the
  declared scope of a change is checked rather than trusted.
- **Waves.** Independent tasks run in parallel, one git worktree each, with
  patches applied sequentially.
- **Drivers** for `claude-code`, `codex`, `copilot` and `kiro`, over a thin
  contract (`keel.drivertask/1` on stdin, `keel.driverresult/1` on stdout).
  `keel driver check` validates a new one in six conformance cases.
- **Shared stores** so several repositories can enforce one set of platform
  rules, via a filesystem path — a sibling checkout, a submodule or a vendored
  copy.
- `release.sh`, which gates a release on formatting, clippy, a non-zero test
  count and a matching CHANGELOG section.

### Known limitations

Stated plainly, because a tool that audits other work should be legible about
its own. The full accounting is in [ROADMAP.md](ROADMAP.md).

- `G2/store-drift` has never failed in 26 runs across two repositories. It may
  be correctly always-true here, but it is the check to scrutinise first.
- G2.5 passes on heuristics alone unless an adversarial reviewer is configured.
- Three lessons are in force against the five the Phase 3 exit criterion wants.
  They accrue with use and are not manufactured.
- macOS only. The `cfg(windows)` branches compile and are unexercised.

[0.4.0]: https://github.com/daneb/keel/releases/tag/v0.4.0
[0.3.2]: https://github.com/daneb/keel/releases/tag/v0.3.2
[0.3.1]: https://github.com/daneb/keel/releases/tag/v0.3.1
[0.3.0]: https://github.com/daneb/keel/releases/tag/v0.3.0
[0.2.2]: https://github.com/daneb/keel/releases/tag/v0.2.2
[0.2.1]: https://github.com/daneb/keel/releases/tag/v0.2.1
[0.2.0]: https://github.com/daneb/keel/releases/tag/v0.2.0
