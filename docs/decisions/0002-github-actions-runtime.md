# ADR-0002: GitHub Actions as a runtime — the runner holds the pen

**Status:** Proposed
**Date:** 2026-09-25

## Context

ADR-0001 split the evidence chain: keel owns the format and the verifier, and
a runtime's host process, which the sandbox can't reach, is the only writer.
Moor implemented that on a Mac. Roadmap R9 asks for a second runtime producing
the identical bundle schema, and phase 3 names GitHub Actions. That's also
where `keel cover` (SPEC-0012) runs: a CI runtime lets the bundle a pull
request needs be built and attested by CI, instead of committed by its author.

On a runner, the job's VM is disposable, but everything in it shares one trust
domain unless we split it. So we split it the way Moor does.

## Decision

1. **Gate only, for now.** CI runs `keel run --no-driver` on the PR's head:
   the gates, the chain, the attestation and the bundle. No coding agent, and
   no model API key in CI. A driver is a later, additive flag.

2. **keel runs in a hardened container; the runner step is the host.**
   - The container gets a read-only root filesystem, all capabilities dropped,
     `no-new-privileges`, and a non-root user.
   - Its network is an input, `network: bridge | none`, defaulting to
     `bridge`. G2 runs the repository's own build and test, which fetch
     dependencies, and this runtime has no egress proxy yet. A repository
     that vendors its dependencies can choose `none`. Either way the
     attestation says which, so a repository can require it.
   - It gets the checkout bind-mounted at `/workspace`, a sink directory, and
     the attestation read-only. In CI, mounting the checkout is the point, so
     the attestation records the bind mounts it finds rather than claiming
     there are none.
   - Everything the host writes lives in `$RUNNER_TEMP`, which is never
     mounted into the container: the chain, the posture attestation, and the
     folded sink.

3. **gVisor when it's there, hardened runc when it isn't.** The runner is
   Linux, so `runsc` works. The runtime installs it and runs the container
   under it, and attests `kernel.isolated` as proven only when `docker
   inspect` reports that runtime. If `runsc` can't be installed, the job falls
   back to runc and attests `kernel.isolated` as `unproven`. A repository that
   requires it (`[runtime] require = ["kernel.isolated"]`) then blocks, rather
   than passing quietly.

4. **The host chain starts from the repository's.** Approvals are human
   decisions recorded where the human is, usually in the repository's
   committed `.keel/chain.jsonl`, written in-process. CI copies that chain into
   `$RUNNER_TEMP`, verifies it, and appends to it with `writer:
   "github-actions"`. The bundle's `approvals` check then has the approvals to
   check, and every entry still says who wrote it. keel's own verifier already
   treats in-process entries as self-attested.

5. **The attestation says what GitHub says.** It combines the same `docker
   inspect` properties Moor derives with `kernel.isolated` and the run's
   identity (repository, workflow, run id, head SHA) from the runner's
   environment. GitHub's OIDC-signed identity token is the stronger form and
   comes with signing; until then the identity is a claim from the runner,
   recorded as such.

6. **The bundle is committed back to the PR.** A bot commit adds it under
   `.keel/bundles/`, so `keel cover` counts it unchanged. That commit only
   touches `.keel/`, which the tree hash leaves out, so the bundle covers the
   head it was built for.

7. **The host side is keel's own code, run on the runner.** Two runtime-neutral
   subcommands do the host's work: `keel runtime fold` (sink into chain) and
   `keel runtime attest` (container into `keel.posture/1`). A composite action,
   `runtime/action.yml`, drives Docker. Moor keeps its own Rust
   implementation; both write the frozen formats.

## Consequences

- R9 becomes testable: the same spec run under Moor and under this runtime
  must give bundles with the same schema, which `keel bundle verify` checks
  alike.
- The action needs `contents: write` to push the bundle commit. A PR from a
  fork can't get it, so fork PRs fall back to an artifact plus a comment.
  That's stated, not solved, here.
- Until OIDC signing, the run identity in the attestation is the runner's
  claim. It's a strong claim, but it isn't signed.
- With the default `bridge` network, the pull request's own code runs its
  tests with internet access. Nothing secret is inside the container: no
  token, and the host chain is out of reach. So the exposure is exfiltration
  of the repository's own contents, which the PR author already has.
  `network: none` removes even that for repositories that can build offline.
- `keel.posture/1` gains an optional top-level `runtime_identity`, an
  additive change the Spine freeze allows.
