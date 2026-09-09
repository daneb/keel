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

### `keel serve` opens a listening socket

Every other keel command reads and writes files and exits. `keel serve` binds a
TCP port and serves the contents of `.keel/` to a browser, which is a different
class of exposure from anything else in this tool. What follows is the whole of
it.

**It is read-only, mechanically.** The request parser rejects any method other
than `GET` and `HEAD` before routing, and rejects any request carrying a body.
There is no route that writes. A gate cannot be run, a spec cannot be approved
and a lesson cannot be promoted from the browser — those stay deliberate CLI
acts bound to an artefact hash, which is the property
[ADR-0003](.keel/store/decisions/ADR-0003-approvals-bind-to-a-hash.md) exists to
protect.

**It binds loopback only.** `127.0.0.1`, never `0.0.0.0`, and there is no
`--host` flag to widen it. A flag that widens the bind is a flag that eventually
gets set in an alias.

**`Host` is validated, because of DNS rebinding.** Loopback binding alone does
not make a local server private. Any page the operator visits can re-resolve its
own hostname to `127.0.0.1` after its DNS TTL expires and then issue same-origin
requests to the port — reading specs, plans, diff stats and evidence logs.
The defence is to reject any request whose `Host` is not exactly
`127.0.0.1:<port>` or `[::1]:<port>`, which is why the URL keel prints uses the
literal address and not `localhost`. `Origin`, when present and foreign, is
rejected too, and no CORS header is ever emitted.

**Evidence content is untrusted, and the browser is the thing at risk.** The
threat model already grants above that a hostile driver can put arbitrary bytes
into evidence files. Those bytes are served to a browser by an origin that can
read the whole `.keel/` tree, which makes stored XSS the live concern rather
than a theoretical one. Evidence is served as `text/plain` only and never
sniffed, every response carries `nosniff` and a `Content-Security-Policy` of
`default-src 'none'` with `'self'` for script and style, and the page never
assigns disk content to `innerHTML`.

That CSP is also what makes the "no telemetry, no CDN, no web fonts" promise
mechanical rather than a matter of good intentions: the page cannot reach the
network even if a future edit tries to.

**The only route that touches disk by name is evidence.** The page, its CSS and
its JavaScript are compiled into the binary with `include_str!`, so there is no
general static-file route to traverse. Evidence file names are matched by exact
string equality against what `read_dir` returned for that run, and the resolved
path is canonicalised and asserted to remain inside the run's evidence
directory. The allowlist is what makes traversal impossible by construction —
important on a case-insensitive, Unicode-normalising filesystem where a
blocklist of `..` is defeatable — and the canonicalise step is what catches a
symlink planted in `evidence/` by a hostile driver.

**It is not a daemon.** It runs in the foreground, holds no state, writes
nothing, and dies with Ctrl-C. There is no launchd plist, no auto-start and no
graceful-shutdown path, because that is where daemons begin.

**On a shared machine, the port is the exposure.** There is deliberately no
token in the URL. A token would not fix rebinding — `Host` validation does that
— and it would leak through `Referer`, shell history, terminal scrollback and
screenshots while making the tool annoying to use. What it would buy is
protection from another local user on the same box, and that user can already
read `.keel/runs/**` directly, so the server grants them no capability they
lacked. The honest statement is therefore: **on a multi-user host such as a
shared build box or a CI runner, any local user can read what `keel serve`
serves.** A Unix domain socket would be the right primitive there, and browsers
cannot speak one. Do not run `keel serve` on a host whose other users should not
read your repository.

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

### As many reviewers as you configure

`[[review]]` is an array. Every entry is a subprocess answering the same
contract — `keel.reviewrequest/1` in, `keel.reviewresult/1` out — and keel does
not care whether a model, a pattern scanner, or your own script produced the
findings. They are graded, recorded and accepted through one path.

```toml
[[review]]
id = "model"                    # logic and context: an authorisation check
cmd = ".keel/reviewers/claude"  # applied after the effect, a bound nobody enforces

[[review]]
id = "semgrep"                  # known patterns: injection, weak crypto,
cmd = ".keel/sast/semgrep"      # unsafe deserialisation
timeout_secs = 300
```

The `id` names the pass on the gate report, prefixes its findings
(`semgrep:subprocess-shell-true`) and names its evidence file. Adding a third
reviewer — `gosec`, `bandit`, a house-style script — is a config entry, never a
change to keel.

**keel ships no scanner.** The semgrep adapter is a 4.8KB script; semgrep itself
is yours to install, and if it is not on `PATH` the check reports `blocked`. It
scans **only the files the diff touches** — a repository-wide scan reports
everything already there, which buries the findings this change is responsible
for.

Two things to know about semgrep specifically. Its registry rulesets (`p/...`)
are fetched over the network on first use; set `SEMGREP_RULES` to a vendored
local ruleset to run fully offline. And the adapter never passes
`--config auto`, which would send project metadata to semgrep.dev;
`--metrics=off` is always set.

If a reviewer cannot run, its check is `blocked`, never `pass`. "We did not
look" and "we looked and it is clean" are not the same verdict.

### What this does not do

The reviewer is a language model reading a diff, and the scanner matches known
patterns. Between them they find what a careful reviewer and a good linter
would find, and miss what both would miss. Neither is a substitute for a
repository-wide audit, for dependency scanning (`cargo-audit` covers that at
G2), or for a penetration test.

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
  driver result parser — and now also the HTTP request parser in `keel serve`.
- No token-based access control on `keel serve`. Access is governed by the
  loopback bind and `Host` validation alone, so every local user on the machine
  can read what it serves. Reasoning is in the threat model above.
- The `cfg(windows)` branches compile but are unexercised; keel is developed and
  tested on macOS, and CI runs Linux.
