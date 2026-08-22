---
id: ADR-0012
schema: keel.adr/1
status: accepted
scope: repo
owner: human
verified_at: 2026-08-22
phase: 4
---

# `keel bench` measures token cost, and says that is all it measures

## Context

The Phase 4 exit criterion demands a measured token drop and warns that the
literature is full of vendor numbers and at least one publicly retracted
benchmark.

The temptation is to report a ratio as though it were a quality result.

## Decision

`keel bench` runs five fixed questions about this repository and compares
retrieval tokens against the tokens of the file reads that would otherwise answer
them — both sides counted with the same estimator, locally, now. The tasks are
fixed in source: a benchmark whose tasks move is one you can tune.

The output states in full that it measures cost and not answer quality, and cites
the published 83%-versus-92% quality figure alongside the ~10× token figure.

## Consequences

The number is honest and unimpressive-sounding next to vendor claims. It is also
reproducible on the reader's own machine in under a second, and `keel bench`
exits non-zero below 3× so the claim cannot quietly rot.

What is still not measured is answer quality. Doing so needs a task set with
known-correct answers, which is Phase 5 work if it is worth doing at all.
