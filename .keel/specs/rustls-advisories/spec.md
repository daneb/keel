---
id: SPEC-0018
slug: rustls-advisories
schema: keel.spec/1
status: draft
scope:
  - "Cargo.lock"
budget:
  criteria: 8
  lines: 120
verified_at: 2026-11-02
---

# Clear RustSec advisories in the TLS dependency chain

## Context

`cargo audit` reports six RustSec advisories in keel's TLS dependency chain,
pulled in transitively through `reqwest` → `rustls`:

- `rustls 0.23.45` — RUSTSEC-2026-0346 (`Debug` impls expose TLS secrets),
  -0347 (data sent to outer server after ECH rejection), -0348 (1 GiB
  pre-auth allocation via brotli cert decompression), -0349 (unauthenticated
  ServerHello downgrades key exchange). Patched in `>=0.23.46`.
- `rustls-webpki 0.103.15` — RUSTSEC-2026-0382 (verification may accept
  distrusted intermediates). Patched in `>=0.103.16`.
- `webpki-root-certs 1.0.9` — RUSTSEC-2026-0383 (bundled roots trust a
  distrusted emSign intermediate). Patched in `>=1.0.10`.

keel's own G2 `cargo-audit` check fails on these, correctly — they are real
vulnerabilities, two of them (secret exposure, intermediate trust) directly
security-relevant. They are not caused by any feature change; they are drift in
the pinned dependency graph. All three fixes are patch-level bumps reachable by
updating `Cargo.lock` alone, without changing any `Cargo.toml` version
requirement or any source.

## Acceptance criteria

### AC-1 cargo audit reports no vulnerabilities

WHEN `cargo audit` runs against `Cargo.lock` THE SYSTEM SHALL report zero
vulnerabilities.

oracle: cmd `cargo audit --quiet` exit 0

### AC-2 rustls is at or above the patched version

THE SYSTEM SHALL pin `rustls` to a version at or above `0.23.46` in
`Cargo.lock`.

oracle: cmd `cargo tree -p rustls --depth 0 | grep -qE 'v0\.(2[3-9]|[3-9][0-9])\.(4[6-9]|[5-9][0-9])'` exit 0

### AC-3 rustls-webpki is at or above the patched version

THE SYSTEM SHALL pin `rustls-webpki` to a version at or above `0.103.16` in
`Cargo.lock`.

oracle: cmd `cargo audit --quiet` exit 0

### AC-4 webpki-root-certs is at or above the patched version

THE SYSTEM SHALL pin `webpki-root-certs` to a version at or above `1.0.10` in
`Cargo.lock`.

oracle: cmd `cargo audit --quiet` exit 0

### AC-5 The crate still builds

WHEN the workspace is built after the upgrade THE SYSTEM SHALL compile with no
errors.

oracle: cmd `cargo build --quiet` exit 0

### AC-6 The test suite still passes

WHEN the test suite runs after the upgrade THE SYSTEM SHALL pass every test.

oracle: cmd `cargo test --quiet` exit 0

### AC-7 No source change accompanies the upgrade

THE SYSTEM SHALL change only `Cargo.lock`, leaving every file under `src/` and
`tests/` untouched, so the upgrade is auditable as a dependency bump alone.

oracle: cmd `git diff --name-only HEAD -- src tests | wc -l | grep -qx 0` exit 0

### AC-8 Lint stays clean

WHEN clippy runs after the upgrade THE SYSTEM SHALL emit no warnings.

oracle: cmd `cargo clippy --all-targets --quiet` exit 0

## Out of scope

- Upgrading `reqwest`, `tokio`, or any dependency not named above. The `tokio`
  unsoundness warning and the `unsafe-libyaml` unmaintained warning are
  non-failing `cargo audit` warnings and are left to a separate change.
- Changing any `Cargo.toml` version requirement. If a transitive bump turns
  out to need a manifest change, that is a larger decision to be made
  deliberately, not folded in here.
- Any source or behaviour change. This is a dependency bump only.
