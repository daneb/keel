---
id: SPEC-0010
slug: copilot-credential-preflight
schema: keel.spec/1
status: approved
scope:
- .keel/drivers/copilot
- .keel/drivers/_common.sh
budget:
  criteria: 6
  lines: 120
---

# Preflight the Copilot driver's credential before it runs the agent

## Context

The GitHub Copilot CLI accepts an OAuth device-flow token (`gho_`, from
`copilot /login`) but rejects classic personal access tokens (`ghp_`)
outright, and fine-grained PATs usually fail. The reliable credential is
therefore the device-flow token, which the host supplies to the sandbox as
`COPILOT_GITHUB_TOKEN` (how the host obtains and injects it is the runtime
host's concern — e.g. moor reads it from the host's `copilot-cli` Keychain
entry; verified working locally).

Today the `copilot` driver only learns a credential is unusable *after*
invoking `copilot -p`: it runs the agent, catches the auth failure in a
`case` on the output, and emits a generic `blocked`. For a wave run it has
already set up a git worktree by then. The driver should instead check the
credential it was given *before* spending the agent call or creating a
worktree, and name the exact fix — `copilot /login` on the host — rather
than letting the agent fail on, e.g., a classic PAT.

Single responsibility: this spec owns the **in-sandbox driver's** credential
check and messaging. Discovering and injecting the token is the host
runtime's job (sibling, moor-side).

## Acceptance criteria

### AC-1 A classic PAT is rejected before the agent runs

WHEN the copilot driver is invoked and `COPILOT_GITHUB_TOKEN` begins with
`ghp_` THE SYSTEM SHALL emit `blocked` and SHALL NOT invoke `copilot -p`.

oracle: cmd `COPILOT_GITHUB_TOKEN=ghp_x .keel/drivers/copilot < /dev/null 2>&1 | grep -qi blocked` exit 0

### AC-2 A rejected or missing credential names copilot /login as the fix

WHEN the credential preflight fails for any reason THE SYSTEM SHALL emit a
message containing the string `copilot /login`.

oracle: cmd `.keel/drivers/copilot < /dev/null 2>&1 | grep -q "copilot /login"` exit 0

### AC-3 A missing credential blocks before invoking the agent

IF no `COPILOT_GITHUB_TOKEN` (nor other accepted env credential) is present
THEN THE SYSTEM SHALL emit `blocked` with exit code 0 and SHALL NOT invoke
`copilot -p`.

oracle: cmd `env -u COPILOT_GITHUB_TOKEN -u GH_TOKEN -u GITHUB_TOKEN .keel/drivers/copilot < /dev/null 2>&1 | grep -qi blocked` exit 0

### AC-4 A device-flow (gho_) token passes preflight and runs the agent

WHEN `COPILOT_GITHUB_TOKEN` begins with `gho_` THE SYSTEM SHALL pass the
preflight and proceed to invoke `copilot -p`, whose own result then decides
the run.

oracle: human with a real gho_ device-flow token set, the driver reaches and runs copilot -p rather than blocking in preflight

### AC-5 Enterprise data-residency host is honoured

WHERE `COPILOT_GH_HOST` (or `GH_HOST`) is set THE SYSTEM SHALL make it
available to the Copilot CLI so authentication targets the enterprise host
rather than github.com.

oracle: human with COPILOT_GH_HOST set, the driver exports/passes it so copilot targets that host — reviewed against the driver source

### AC-6 Preflight runs before any worktree is created

WHEN a wave build's copilot preflight fails THE SYSTEM SHALL exit before
creating any directory under `.keel/worktrees/`.

oracle: human starting a --waves copilot build with no credential leaves .keel/worktrees/ with no new entry

## Out of scope

- Obtaining, storing, or refreshing the token — the host supplies
  `COPILOT_GITHUB_TOKEN`; this driver only validates and reports.
- Running the device flow inside the sandbox (no interactive session, by
  design). The fix the message points at (`copilot /login`) is a host act.
- The other drivers (claude/codex/kiro): each has its own credential shape
  and is a separate preflight if wanted.
