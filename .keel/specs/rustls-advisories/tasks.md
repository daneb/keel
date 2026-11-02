---
id: TASKS-0018
slug: rustls-advisories
schema: keel.tasks/1
---

# Tasks

Each task must name the criteria it satisfies, the files it touches, a line
budget and an exit condition. G1 checks all four, and checks that every
criterion in the spec is covered by at least one task.

Add `- depends_on: T-1` where order matters. Tasks with no dependency on
each other form a wave; `keel tasks` shows them.

### T-1 Bump the three advisory crates in Cargo.lock
- criteria: AC-1, AC-2, AC-3, AC-4, AC-5, AC-6, AC-7, AC-8
- files: Cargo.lock
- budget: 20
- exit: `cargo audit --quiet` exits 0

The upgrade is one atomic lockfile edit — `cargo update -p rustls -p
rustls-webpki -p webpki-root-certs` — so it is a single task. Every
acceptance criterion is a different lens on the same edit (audit clean,
each crate at its patched floor, build/test/lint still green, no source
touched), verified by the oracles in the spec rather than by splitting
the one change into eight.
