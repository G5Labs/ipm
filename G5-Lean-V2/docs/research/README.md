# Backend survey notes (2026-09-12)

Read-only research notes produced while writing
[../lean-backend-design-v1.md](../lean-backend-design-v1.md). Each note separates
observed facts (with `file:line` citations into this checkout) from
inferences, and ends with coupling, simplification candidates, and open
questions. They are evidence for the design, not decisions.

| Note | Area |
|---|---|
| [research-exec-isolation.md](research-exec-isolation.md) | Task execution, worker isolation, host and VM lifecycle, preview VMs, terminal |
| [research-task-lifecycle.md](research-task-lifecycle.md) | Job and task state machines, assignment, leases, recovery, maintenance loops |
| [research-crdt-docs.md](research-crdt-docs.md) | CRDT layer, drafts, "authority", document and ontology editing model |
| [research-git-workspace.md](research-git-workspace.md) | Workspace/branch/repo model, internal git store, head operations, remote tracking |
| [research-agent-tooling.md](research-agent-tooling.md) | g5d CLI, agent tools, MCP server, profiles, provider launch, credentials |
| [research-governance.md](research-governance.md) | Roles, approvals, merge workflow, pull requests, enforcement points |
| [research-chat.md](research-chat.md) | Chat sessions, sticky runtimes, forking, persistence between turns, recovery |
| [research-frontend-preview-infra.md](research-frontend-preview-infra.md) | Frontend feature inventory, preview end to end, infra and deployment model |

Line numbers refer to the checkout as of 2026-09-12 and will drift as the
re-architecture proceeds.
