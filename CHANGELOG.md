# Changelog

Notable changes to keel. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and keel uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

Pre-1.0 means the command surface may still move. The wire schemas are frozen
and additive-only — see *Spine freeze* in [ROADMAP.md](ROADMAP.md).

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

[0.2.0]: https://github.com/daneb/keel/releases/tag/v0.2.0
