# ADR-0001: One evidence chain — keel owns the format, the runtime holds the pen

**Status:** Proposed
**Date:** 2026-09-23

## Context

Evidence for an agent-authored change is split across two logs today. keel
holds approvals (`approvals.jsonl`, `keel.approval/1`), gate verdicts and a
per-run `trajectory.jsonl`. Moor holds a SHA-256 hash chain
(`~/.moor/projects/<name>/audit/chain.jsonl`) with exec records, folded egress
verdicts and agent turns. An auditor has to trust both logs and the join
between them.

The obvious merge — "keel owns the chain and appends to it" — is unsafe.
keel runs *inside* the sandbox. Moor's chain is trustworthy because of where
it is written, not because it is hashed: the audit directory lives on the host,
is never mounted into a container, and only the host-side `moor` process
writes it (Moor THREAT-MODEL §8). An unkeyed hash chain does not stop anyone
who can write the file: they rewrite the tail, recompute every `prev_hash`,
and `verify` still passes. A chain written from inside the sandbox is one the
agent can forge.

Two further constraints:

- Moor deliberately keeps conversation text out of its chain, because
  `moor audit --export` ships the chain wherever the bundle goes. keel's
  trajectory records what reached the model (`inject`, `driver_call`).
  Copying it into the chain reverses that disclosure decision.
- Moor cannot see pushes from `moor shell`, and its egress proxy does not
  intercept TLS, so it cannot tell a push from a fetch. The chain can hold
  "a connection to github.com happened", not "commit X was pushed", unless
  the push goes through the runtime.

## Decision

1. **One chain, one format, one verifier — owned by keel.** keel defines
   `keel.chain/1` (entry shape, hashing rule, entry kinds) and ships
   `keel chain verify`. Moor's `audit::append_chained` and `verify_chain`
   become an implementation of that format, not a separate one.

2. **The runtime's host process is the only writer when a runtime is
   present.** keel inside the sandbox never appends to the chain. It emits
   entry payloads to a sink named by `KEEL_CHAIN_SINK` (a pipe or socket the
   runtime provides); the runtime stamps sequence, timestamp and
   `prev_hash` on the host side and appends. The agent can lie about what
   keel emitted but cannot rewrite what was already recorded.

3. **Without a runtime, keel appends in-process and says so.** Every such
   entry carries `writer: "in-process"`, and `keel chain verify` reports the
   chain as *self-attested*. This keeps local, un-sandboxed use working
   without letting it pass for contained evidence.

4. **The head is anchored outside the writer.** `keel chain head` prints the
   last entry's hash; `keel chain verify --head <hash>` fails if the chain
   ends anywhere else. The anchor travels with the change (PR check, git
   note), so a rewritten tail is detected even before signing lands.
   Signatures (cosign) remain phase 3 and strengthen the anchor; they do not
   replace it.

5. **The trajectory stays out of the chain.** A run's chain entry carries the
   SHA-256 of `trajectory.jsonl`, not its content. The bundle may include the
   trajectory as a separate, optional member; the chain proves it was not
   altered.

6. **Secrets are redacted before hashing.** Redacting after hashing breaks
   verification; redacting before means the hash commits to what the auditor
   actually sees. Stated limit, inherited from Moor: only values of
   *declared* secret names are caught.

## Consequences

- The evidence-chain spec (`.keel/specs/evidence-chain/`) implements the keel
  side: format, append, sink mode, verify, head anchoring, redaction.
- Moor needs a follow-up: read `KEEL_CHAIN_SINK` payloads from the container
  and append them, and emit its own exec/egress/turn entries in
  `keel.chain/1`. Existing Moor chains stay verifiable by `moor audit
  --verify`; they are not migrated.
- Push evidence (R7) is only as good as the push path. A push recorded by
  the runtime carries ref and SHA; one made from an interactive shell shows
  up only as an egress entry. The PR check closes that gap from the remote
  side, not from the chain.
- Approver identity in `keel.approval/1` is still a self-declared string.
  The chain proves *that* an approval was recorded and when, not who made
  it. Authenticating the approver is a separate decision.
