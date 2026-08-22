---
id: SPEC-0001
slug: trajectory-log
schema: keel.spec/1
status: draft
scope:
  - "src/trajectory/**"
  - "src/cmd/run.rs"
budget:
  criteria: 8
  lines: 160
verified_at: 2026-08-21
---

# Append-only trajectory log

## Context

PLAN.md P5 requires that everything the model was shown, and every verdict a
gate reached, is reconstructable from an append-only log. Today keel records
gate verdicts as standalone JSON files with no ordering between them and no
record of what was injected. A verdict you cannot reproduce is an opinion with
a timestamp.

The invariant to build toward, borrowed from DeepSeek Harness: anything that
reached a model must be reconstructable from this stream.

## Acceptance criteria

### AC-1 One event per line

WHEN an event is appended THE SYSTEM SHALL write exactly one JSON object,
terminated by a newline, to `.keel/runs/<run-id>/trajectory.jsonl`.

oracle: test tests/trajectory.rs::one_json_object_per_line

### AC-2 Sequence numbers are gapless and increasing

THE SYSTEM SHALL assign each event within a run a `seq` field that starts at 1
and increases by exactly 1 per event.

oracle: test tests/trajectory.rs::seq_is_gapless_and_increasing

### AC-3 Existing trajectories are never truncated

IF a trajectory file already exists for a run THEN THE SYSTEM SHALL append to
it and preserve every line already present.

oracle: test tests/trajectory.rs::append_preserves_existing_lines

### AC-4 Gate verdicts enter the stream

WHEN a gate reaches a verdict THE SYSTEM SHALL append an event of kind `gate`
carrying the gate id, the verdict, and the path of the gate result file.

oracle: test tests/trajectory.rs::gate_verdict_is_recorded

### AC-5 Injections are recorded with their token cost

WHEN keel injects a store document into an agent prompt THE SYSTEM SHALL append
an event of kind `inject` carrying the source path and the token count.

oracle: test tests/trajectory.rs::injection_records_source_and_tokens

### AC-6 A run is replayable from its own stream

WHEN `keel replay <run-id>` is invoked THE SYSTEM SHALL print every event of
that run in `seq` order and exit 0.

oracle: cmd `keel replay $(keel runs --latest) | head -1` exit 0

### AC-7 A corrupt line fails loudly

IF a line in a trajectory file is not valid JSON THEN THE SYSTEM SHALL exit
non-zero and name the file and line number.

oracle: test tests/trajectory.rs::corrupt_line_names_file_and_line

## Out of scope

Trajectory compaction, remote shipping of trajectories, and any analysis over
them. This spec covers writing and reading the stream, nothing built on top.
