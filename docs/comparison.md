# keel compared with other spec-driven tools

keel is not the only tool that puts a spec between you and an agent. This page
says where it differs, and where something else is the better choice.

"Not documented" means the tool's own page, checked on the date under
[Sources](#sources), does not describe the property. It is not a claim that the
tool cannot do it. Every keel cell links to the ADR or test that backs it.

| | keel | spec-kit | Kiro | BMAD | Tessl | Elephant-Goldfish | Claude Code hooks |
|---|---|---|---|---|---|---|---|
| falsifiable criteria | Yes: each criterion carries an oracle ([test](../tests/oracle_sensitivity.rs)) | Stated: its spec-driven.md checklists require requirements to be testable and unambiguous[^speckit] | Partial: specs hold user stories with acceptance criteria, falsifiability not stated[^kiro] | Not documented[^bmad] | Not documented[^tessl] | Not documented[^egm] | Not documented[^hooks] |
| blocked as a verdict | Yes: pass, fail or blocked ([ADR-0002](../.keel/store/decisions/ADR-0002-blocked-is-a-verdict.md)) | Not documented[^speckit] | Not documented[^kiro] | Not documented[^bmad] | Not documented[^tessl] | Not documented[^egm] | Yes: exit code 2 blocks the action, and a deny decision is a separate block path[^hooks] |
| approvals bound to content | Yes: an approval binds to a hash ([ADR-0003](../.keel/store/decisions/ADR-0003-approvals-bind-to-a-hash.md)) | Not documented[^speckit] | Not documented[^kiro] | Not documented[^bmad] | Not documented[^tessl] | Not documented[^egm] | Not documented[^hooks] |
| tamper-evident evidence | Yes: a hash-chained bundle verified offline ([test](../tests/bundle.rs)) | Not documented[^speckit] | Not documented[^kiro] | Not documented[^bmad] | Not documented[^tessl] | Not documented[^egm] | Not documented[^hooks] |
| computed blast radius | Yes: recomputed, never trusted ([ADR-0009](../.keel/store/decisions/ADR-0009-blast-radius-is-recomputed.md)) | Not documented[^speckit] | Not documented[^kiro] | Not documented[^bmad] | Not documented[^tessl] | Not documented[^egm] | Not documented[^hooks] |
| test-tampering review | Yes: added mocks and weakened assertions are flagged ([test](../tests/phase2.rs)) | Not documented[^speckit] | Not documented[^kiro] | Not documented[^bmad] | Not documented[^tessl] | Not documented[^egm] | Not documented[^hooks] |
| lessons enforced as checks | Yes: enforced lessons become checks ([ADR-0007](../.keel/store/decisions/ADR-0007-enforced-lessons-are-not-injected.md)) | Not documented[^speckit] | Not documented[^kiro] | Not documented[^bmad] | Not documented[^tessl] | Not documented[^egm] | Not documented[^hooks] |
| agent drivers | Yes: a driver contract with a conformance suite ([test](../tests/driver.rs)) | Partial: GitHub Copilot is named, other integrations are not[^speckit] | Not documented[^kiro] | Yes: Claude Code and Codex plugins are named, and any tool that supports skills works[^bmad] | Agent-agnostic, no agents named[^tessl] | Antigravity is named, other environments are announced[^egm] | Run wherever Claude Code runs: CLI, Agent SDK and plugins[^hooks] |

## Where keel is weaker

- **spec-kit**: choose it when you want lightweight spec scaffolding with
  integrations for other agents.
- **Kiro**: choose it when you want an IDE with in-product spec review and
  commenting.
- **BMAD**: choose it when you want role-based agile planning for whole
  projects.
- **Tessl**: choose it when you want a registry of library usage specs.
- **Elephant-Goldfish**: choose it when you want a fresh-context review of a
  design document before implementation, which keel does not do.
- **Claude Code hooks**: choose them when you want immediate per-tool-call
  blocking across 33 events, with no spec or plan stage.

This comparison covers only what each tool's primary pages state. Every "Not
documented" may be a gap in the page, not in the tool.

## Sources

[^speckit]: https://github.com/github/spec-kit and https://github.com/github/spec-kit/blob/main/spec-driven.md (checked: 2026-11-04)
[^kiro]: https://kiro.dev/docs/specs/ (checked: 2026-11-04)
[^bmad]: https://github.com/bmad-code-org/BMAD-METHOD (checked: 2026-11-04)
[^tessl]: https://docs.tessl.io/ (checked: 2026-11-04)
[^egm]: https://opensource.googleblog.com/2026/10/introducing-elephant-goldfish-model-plugin.html (checked: 2026-11-04)
[^hooks]: https://code.claude.com/docs/en/hooks (checked: 2026-11-04)
