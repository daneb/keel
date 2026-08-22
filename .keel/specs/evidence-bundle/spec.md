---
id: SPEC-0003
slug: evidence-bundle
schema: keel.spec/1
status: implemented
scope:
  - "src/evidence/**"
  - "src/cmd/export.rs"
budget:
  criteria: 6
  lines: 150
verified_at: 2026-08-22
---

# Exportable evidence bundle

## Context

PLAN.md G3 requires a human verdict against an attached evidence bundle, and
§5 requires that a run be exportable as one artefact for review or for a
governance audience. The bundle is the thing handed to somebody who was not
present when the work happened.

## Acceptance criteria

### AC-1 A bundle is one file

WHEN `keel export <run-id>` is invoked THE SYSTEM SHALL write a single
`.tar.gz` archive and print its path on stdout.

oracle: cmd `keel export $(keel runs --latest) | xargs test -f` exit 0

### AC-2 The bundle contains the whole record

THE SYSTEM SHALL include the trajectory, every gate result, every evidence
file, and the spec, plan and tasks in the archive.

oracle: test tests/export.rs::bundle_contains_every_required_member

### AC-3 A bundle names the store it was built from

THE SYSTEM SHALL write a `manifest.json` carrying the run id, the store hash,
the keel version, and the SHA-256 of every archive member.

oracle: test tests/export.rs::manifest_validates_against_the_published_schema

### AC-4 A bundle verifies against itself

WHEN `keel export --verify <bundle>` is invoked against an archive whose member
hashes match its manifest THE SYSTEM SHALL exit 0.

oracle: test tests/export.rs::verify_accepts_an_intact_bundle

### AC-5 Tampering is detected

IF an archive member does not match its manifest hash THEN THE SYSTEM SHALL
exit non-zero and name the member.

oracle: test tests/export.rs::verify_names_the_tampered_member

### AC-6 The bundle is legible to a non-participant

THE SYSTEM SHALL include a `README.md` in the archive stating what the run
changed, which gates ran, and what each verdict was.

oracle: human a reviewer who did not run the work states what changed and which gate failed, reading only the bundle

## Out of scope

Signing, encryption, and uploading bundles anywhere. A bundle is a file on
disk; distributing it is somebody else's problem.
