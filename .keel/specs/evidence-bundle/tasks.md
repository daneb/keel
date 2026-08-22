---
id: TASKS-0003
slug: evidence-bundle
schema: keel.tasks/1
---

# Tasks

### T-1 Archive writer and manifest
- criteria: AC-1, AC-2, AC-3
- files: src/evidence/mod.rs, src/evidence/manifest.rs
- budget: 70
- exit: `keel export` writes one .tar.gz containing every required member

### T-2 Verification and tamper detection
- criteria: AC-4, AC-5
- files: src/evidence/mod.rs, src/cmd/export.rs
- budget: 45
- exit: verify exits 0 on an intact bundle and names the member on a tampered one

### T-3 Human-legible bundle README
- criteria: AC-6
- files: src/evidence/readme.rs
- budget: 30
- exit: a reviewer reading only the bundle states what changed and which gate failed
