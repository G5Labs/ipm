# Fixed workspace audiences

[Issue #1107](https://github.com/G5Labs/G5-Lean/issues/1107) authorizes removing configurable workspace sharing while preserving personal privacy and formal review isolation. Implementation and publication are authorized by the owner's build-to-ship request.

## Design

The workspace registry owns identity. The existing scoped authorization boundary decides reads from that identity and the reader's current active organization membership: ordinary workspaces and Base admit eligible members; personal workspaces admit their owner and organization owners/admins; formal review runtimes admit only their existing workflow authority. Retained formal evidence continues to follow source-history authority, with target content checked separately. No replacement setting, grant list, or lifecycle is needed.

A pure SQL audience predicate takes the existing identity and authenticated reader facts, including the administrator role. Direct reads, bulk queries, and privileged parent/runtime lookups use that same rule. It supplies neither organization admission nor credential scope: callers retain both. Live reads additionally require a live workspace; retained PR history deliberately applies the same audience to retained identity without requiring liveness. Formal review refusal takes precedence over administrator and owner admission.

Reader writes retain current membership and workspace identity through their transaction using the existing membership and workspace locks. Remove only invitation locks. Workspace ownership, transfer/deletion admission, content policy, approval decisions, ordinary review-copy operations, and repository membership remain separate authorities. Git still reads by repository, with personal repositories restricted and formal evidence refs hidden.

Remove the visibility field and workspace-member routes, requests, DTOs, fixtures and producers. Unknown metadata mutation fields are refused so an obsolete privacy request cannot appear successful. Description and phase behavior remain supported. Creation and recovery derive access from personal/formal identity instead of copying visibility.

## Governing commitments

- [Workspace audience](../../ontology/features/workspaces.g5.md#g5.workspace.audience) and [personal access](../../ontology/features/personal_workspace.g5.md#g5.personal.access) define the new fixed rule.
- [Current authority](../../ontology/concepts/security.g5.md#g5.security.current-authority) and unchanged [active membership](../../ontology/concepts/security.g5.md#g5.security.active-membership) govern admission and commit.
- Unchanged [configuration authority](../../ontology/features/workspaces.g5.md#g5.workspace.configuration-authority) and [content policy](../../ontology/features/workspaces.g5.md#g5.workspace.content-policy) keep broader reads from granting ownership.
- Unchanged [formal review inspection](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.human-read-only) and [agent read scope](../../ontology/concepts/security.g5.md#g5.security.agent-read-scope) govern special consumers.
- [Retained PR reads](../../ontology/software_architecture/git.g5.md#g5.git.pr-reads) follow retained identity and current reader authority.

## Migration and deployment boundary

A forward migration replaces dependent SQL functions and the access-generation trigger, then drops `workspace_members` and `workspaces.visibility`. Published migrations remain byte-identical. Organization activity, membership and ownership revocation remain effective.

Formerly invite-only ordinary workspaces, including their conversations, artifacts, comments and retained PR history, become organization-readable. Personal and formal identities never enter that expansion. Before deployment, an operator must inventory live/deleted ordinary, Base, personal and formal records, their old visibility/grants and retained PRs. No production inventory or deployment has been performed in this workspace. If that inventory reveals a need for selective ordinary access, it conflicts with complete removal and needs an owner decision rather than a hidden compatibility feature.

This is a coordinated application/schema cutover: drain and stop all old API/background processes before applying the migration, then start only the matching release. The normal pre-upgrade migration job alone does not make overlapping old instances compatible. A migration lock timeout rolls back the migration for retry. Before reopening traffic, verify ordinary admission and personal/formal refusal. Restore the pre-cutover database and old application together only while traffic remains stopped; after admission expands, rollback cannot revoke information already read. Prefer forward repair after reopening.

## Validation

Exercise fresh installation and a populated upgrade through the production migrator, repeat no-op, shared predicate parity, retired endpoint/field refusal, and preserved metadata edits. API and browser tests cover ordinary readers, personal owner/admin/member isolation, formal confinement, listings and retained PRs. Reader-write races cover membership withdrawal/deactivation, deletion and ownership fencing. Run affected backend/frontend suites and exact-candidate CI; review findings are validated against these commitments. Record actual results and deployment gaps without treating local tests as production observation.
