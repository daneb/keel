# Security

## Reporting a vulnerability

Open a [private security advisory](https://github.com/daneb/keel/security/advisories/new)
on this repository. Please do not open a public issue for anything exploitable.

Expect a first response within a week. keel is maintained by one person; there
is no on-call rotation and no SLA beyond best effort.

## Supported versions

The most recent release only. keel is pre-1.0 and there are no backports.

---

## Threat model

keel is a build harness. Its job is to run your commands, your agents, and your
checks, and to record what happened. That makes several things true which are
worth stating rather than leaving implied.

### `.keel/keel.toml` is executable code

Every one of these config fields is a shell command keel will run:

| Field | Runs when |
| --- | --- |
| `[verify] build` / `test` / `lint` | G2 |
| `[oracle] test_cmd` / `doctest_cmd` | any spec oracle of that kind |
| `[[ratchet]] cmd` | G2's baseline ratchet |
| `[[gate.<G>.check]] cmd` | that gate |
| `[[driver]] cmd` | `keel run` |
| `[review] cmd` | G2.5 |

`verify`, `oracle` and `ratchet` commands are passed to `sh -c`, so they are
full shell, not an argv list.

**Cloning an untrusted repository and running any keel command that reaches one
of those fields executes that repository's code.** This is the same trust model
as `Makefile`, `package.json` scripts, `.cargo/config.toml` or a git hook — but
unlike those it has not been written down anywhere until now.

Treat `.keel/keel.toml` in a pull request exactly as you would treat a change to
CI configuration: as code, reviewed by a person.

### Driver output is untrusted input

A driver is a subprocess keel does not control. Its stdout is parsed as
`keel.driverresult/1` JSON and its stderr is captured verbatim. keel defends
the process boundary — the driver runs in its own process group, is killed as a
group on timeout, and malformed output produces `blocked` rather than a panic —
but it does not sanitise the content. A hostile driver can put arbitrary bytes
into your evidence files.

### Evidence files capture command output, and are committed

`keel run` writes `build.txt`, `test.txt`, `lint.txt` and `driver-stderr.txt`
into `.keel/runs/<id>/evidence/`, and `.keel/runs/**` is intended to be
committed — that is the point of an audit trail.

**If a build, test, lint or driver command prints a secret, keel commits it.**
This is the most likely way keel leaks something, and it is a property of
faithfully recording what your commands printed. Keep credentials out of
command output, and scan your repository for secrets (see `.keel/checks/secrets`
and the `secrets` job in CI).

### Shared stores are trusted by reference

A `[[shared]]` entry points at another repository's `.keel/store`, and its
conventions and lessons are rendered into this repository's projections — which
is to say, into the context every agent reads. A shared store you do not control
can therefore influence how an agent behaves in your repository.

Point `[[shared]]` only at stores you would grant commit access to. Vendoring
one as a git submodule pins it to a reviewed commit, which is stronger than a
path that silently follows someone else's `main`.

### Prompt injection is not solved here

keel assembles context and hands it to an agent. Content in your store, your
specs, and files an agent retrieves all end up in a model's context window, and
keel does not attempt to detect or neutralise instructions hidden in them. The
mitigations keel does offer are structural rather than semantic: the persona and
lesson caps bound how much a single bad input can shift, approvals bind to a
SHA-256 so a change cannot be inherited silently, and G2 checks the diff against
the declared scope regardless of what the agent believed it was doing.

---

## Reviewing the code keel helps produce

The studies are consistent that AI-generated code carries security defects at a
meaningful rate, and keel exists to put agents to work. So G2.5 reviews the
**diff** for security defects — the added and modified lines, not the repository
around them. Pre-existing weaknesses are not this change's findings, and
reporting them buries the ones that are.

The reviewer grades what it finds, and the grade decides what happens:

| Grade | Meaning | Effect |
| --- | --- | --- |
| `critical` | Exploitable as written | **G2.5 fails** |
| `high` | Exploitable given a reachable caller | **G2.5 fails** |
| `medium` | Needs a condition not shown in the diff | Recorded, does not block |
| `low` | Hardening | Recorded, does not block |

Categories the reviewer is asked for: `injection`, `authz`, `crypto`,
`secret-exposure`, `unsafe-input`, `resource-exhaustion`.

Grading and blocking are deliberately separate axes. `severity` in
`keel.reviewresult/1` says whether a finding blocks; `grade` says how dangerous
it is. A hardcoded credential and a missing hardening comment are not the same
thing, and one fail/concern flag cannot say so.

### When a HIGH is found

Fix it. That is the expected path and there is no shortcut worth taking instead.

When it is genuinely a false positive, or genuinely not reachable, accept it
deliberately and on the record:

```sh
keel approve --stage security <slug> --note "why this is not exploitable here"
```

The acceptance binds to the SHA-256 of `security-findings.json`, which records
finding **identity** — category, grade, file and line — and not the reviewer's
prose. Two consequences, both intended:

- Rewording is not a new finding. The same defect described differently next
  week does not spuriously re-open a decision you already made.
- **A different finding is not covered by an old acceptance.** A new `high` at
  another line changes the file, supersedes the approval, and fails the gate
  again. You cannot accept one HIGH and inherit that acceptance over the next
  one.

Fixing the findings empties the file, which also supersedes the acceptance —
so a stale approval never sits there looking current.

### What this does not do

The reviewer is a language model reading a diff. It finds what a careful
reviewer reading the same diff would find, and misses what they would miss. It
is not a substitute for a SAST tool over the whole repository, for dependency
scanning (`cargo-audit` covers that at G2), or for a penetration test. A
deterministic scanner over changed files is the intended next layer, and is not
built yet.

Set `advisory = true` under `[review]` to grade without blocking while you are
calibrating the reviewer on an existing codebase.

## What keel does defend

These are deliberate, tested properties rather than incidental ones:

- **Approvals bind to content.** An approval records the SHA-256 of what was
  approved; editing the artefact supersedes the sign-off rather than inheriting
  it ([ADR-0003](.keel/store/decisions/ADR-0003-approvals-bind-to-a-hash.md)).
- **`blocked` is never `pass`.** A check that could not run says so, with its
  own exit code (3). A gate with no checks is `blocked`
  ([ADR-0002](.keel/store/decisions/ADR-0002-blocked-is-a-verdict.md)).
- **Generated files cannot be edited silently.** Two hashes per projection
  separate "stale" from "a human edited this", and the latter refuses rather
  than overwriting ([ADR-0001](.keel/store/decisions/ADR-0001-two-hashes-per-projection.md)).
- **Drivers are contained.** Own process group, killed as a group on timeout,
  bounded drain so a surviving grandchild cannot hold a run open, and
  conformance probes run in a scratch repository rather than your tree.
- **Dependencies are audited at the gate.** A RustSec advisory fails G2
  (`.keel/checks/cargo-audit`), and CI re-checks weekly because advisories land
  against dependencies that have not changed.

## What is not covered yet

Named so the absence is a known state rather than an assumption:

- No SBOM, and no signing or build provenance on released artefacts.
- No license or dependency-source policy (`cargo-deny` is not wired in).
- No fuzzing of the parsers that take untrusted input, most importantly the
  driver result parser.
- No deterministic SAST over changed files. The G2.5 security review is a model
  reading the diff; a pattern-based scanner alongside it is the next layer.
- The `cfg(windows)` branches compile but are unexercised; keel is developed and
  tested on macOS, and CI runs Linux.
