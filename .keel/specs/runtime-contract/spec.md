---
id: SPEC-0010
slug: runtime-contract
schema: keel.spec/1
status: approved
scope:
- src/runtime/**
- src/cmd/run.rs
- src/main.rs
- src/config.rs
- schemas/**
- tests/runtime.rs
budget:
  criteria: 8
  lines: 550
verified_at: 2026-09-24
---

# Runtime posture attestation

## Context

A gated change is only as trustworthy as the sandbox it ran in, and today
keel cannot tell a contained run from one on a bare laptop. The roadmap's
`keel.runtime/1` names five operations (`provision`, `attest`, `exec`,
`events`, `teardown`), but under Moor the runtime launches keel inside the
container, not the reverse: keel cannot provision the box it runs in. So v0 of
the contract is the one direction that holds today. The runtime writes a
`keel.posture/1` attestation before starting keel and names it in
`KEEL_RUNTIME_ATTESTATION`. keel records it in the evidence chain (ADR-0001)
and checks it against the properties `[runtime] require` lists, before the
agent runs. `provision`, `exec` and `teardown` wait until orchestration moves
host-side with `moor recipe`.

An attestation is a claim, not proof. keel checks that the runtime *claimed*
each required property and records exactly what it claimed. Whether the claim
is honest is the runtime's job, done from outside the sandbox. A property the
runtime cannot vouch for is `blocked`, never `pass`: the same rule every
keel gate follows.

## Acceptance criteria

### AC-1 The attestation enters the chain before the agent runs

WHEN `keel run` starts and `KEEL_RUNTIME_ATTESTATION` names a readable file
THE SYSTEM SHALL append an `attest` chain entry carrying the file's SHA-256
and its properties before the driver is invoked.

oracle: test tests/runtime.rs::attest_entry_precedes_the_driver

### AC-2 Proven posture lets the run proceed

WHEN every property named in `[runtime] require` is marked `proven` in the
attestation THE SYSTEM SHALL record the `posture` gate as pass and invoke the
driver.

oracle: test tests/runtime.rs::proven_posture_passes_and_runs_the_driver

### AC-3 Unprovable posture blocks

IF a property named in `[runtime] require` is absent from the attestation or
marked `unproven` THEN THE SYSTEM SHALL record the `posture` gate as blocked,
name the property, exit 3, and not invoke the driver.

oracle: test tests/runtime.rs::unproven_property_blocks_before_the_driver

### AC-4 Violated posture fails

IF a property named in `[runtime] require` is marked `violated` THEN THE
SYSTEM SHALL record the `posture` gate as fail, name the property, exit 1,
and not invoke the driver.

oracle: test tests/runtime.rs::violated_property_fails_before_the_driver

### AC-5 A required posture with no attestation blocks

IF `[runtime] require` names any property and `KEEL_RUNTIME_ATTESTATION` is
unset or names a file that cannot be read THEN THE SYSTEM SHALL record the
`posture` gate as blocked, exit 3, and not invoke the driver.

oracle: test tests/runtime.rs::missing_attestation_blocks

### AC-6 A malformed attestation blocks and names the field

IF the attestation does not match `schemas/posture.json` THEN THE SYSTEM
SHALL record the `posture` gate as blocked and name the first offending
field.

oracle: test tests/runtime.rs::malformed_attestation_names_the_field

### AC-7 No requirement, no change

WHILE `[runtime] require` is empty THE SYSTEM SHALL run without an
attestation and SHALL NOT write a `posture` gate result.

oracle: test tests/runtime.rs::no_requirement_leaves_runs_unchanged

### AC-8 Waves are checked once, up front

WHEN `keel run --waves` starts THE SYSTEM SHALL evaluate the `posture` gate
once before the first wave and not invoke any driver if it does not pass.

oracle: test tests/runtime.rs::waves_check_posture_before_the_first_wave

## Out of scope

- `provision`, `exec`, `events` and `teardown`, including keel starting or
  stopping a sandbox.
- Moor producing the attestation, or checking its claims from outside the
  container.
- A fixed list of posture property names. keel checks whatever
  `[runtime] require` names, and the runtime decides what it can vouch for.
- Egress and push evidence.
