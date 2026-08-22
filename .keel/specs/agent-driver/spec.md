---
id: SPEC-0002
slug: agent-driver
schema: keel.spec/1
status: implemented
scope:
  - "src/driver/**"
  - "src/cmd/run.rs"
budget:
  criteria: 8
  lines: 160
verified_at: 2026-08-22
---

# Agent driver plugin interface

## Context

keel delegates code generation (PLAN.md §1). To do that it needs a thin,
language-agnostic contract for running a task against somebody else's coding
agent. Drivers stay thin on purpose: the moment a driver reimplements what the
underlying CLI already does, keel is competing with a product instead of
conducting it.

## Acceptance criteria

### AC-1 A driver is a subprocess speaking JSON

WHEN keel invokes a driver THE SYSTEM SHALL pass the task as a single JSON
object on stdin and read a single JSON object from stdout.

oracle: test tests/driver.rs::task_in_result_out

### AC-2 A driver that cannot start blocks rather than fails

IF a driver executable cannot be started THEN THE SYSTEM SHALL record the
verdict `blocked` and SHALL NOT record an agentic failure.

oracle: test tests/driver.rs::unstartable_driver_is_blocked

### AC-3 Driver output is validated before use

IF a driver prints output that does not match `keel.driverresult/1` THEN THE
SYSTEM SHALL exit non-zero and name the offending field.

oracle: test tests/driver.rs::invalid_result_names_the_field

### AC-4 The bundled claude-code driver satisfies the contract

WHERE the driver id `claude-code` is configured THE SYSTEM SHALL invoke the
command named by `driver.cmd`, pass it the task JSON on stdin, and parse its
stdout as a `keel.driverresult/1` object.

oracle: test tests/driver.rs::claude_code_driver_round_trips

### AC-5 Every driver invocation enters the trajectory

WHEN a driver is invoked THE SYSTEM SHALL append `driver_call` and
`driver_result` events to the run trajectory.

oracle: test tests/driver.rs::invocation_is_recorded

### AC-6 A driver run respects its declared timeout

WHEN a driver exceeds the timeout declared in `.keel/keel.toml` THE SYSTEM
SHALL terminate the subprocess and record the verdict `blocked`.

oracle: test tests/driver.rs::timeout_terminates_and_blocks

## Out of scope

Implementing drivers for Kiro, Copilot and Codex. Those are Phase 5 and must
not require a change to this contract.
