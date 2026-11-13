# System Invariants

<a id="g5.invariant"></a>
[I · g5.invariant]

System invariants are rules that must hold across the whole platform, whatever feature or path is involved.

<a id="g5.invariant.migrations"></a>
[I · g5.invariant.migrations]

Do not edit existing migration SQL or checksums, except for migrations added on the current feature branch. Use a new numbered migration to change stored schema or data. A behavior change alone does not require new storage.

<a id="g5.architecture"></a>
[D · g5.architecture]

Architecture invariants fix how the parts of G5 fit together, so that every path reaches data and effects the same sanctioned way.

<a id="g5.architecture.commands"></a>
[D · g5.architecture.commands]

Every entry path — routes, hosts, callbacks, CLI and MCP — reaches data only through the operations of the component that owns it, or its documented transaction steps.

- Callers must not recreate its rules, locks or raw writes; access and dependency boundaries prevent this bypass.
- An existing function or module can own these operations; a new service is not required.
- Preserve the documented dispatch steps and the flush callback-lock exception.

<a id="g5.security.tenant"></a>
[D · g5.security.tenant]

Keep records, references, events and resource access within their organization. Knowing another organization’s identifier never grants access, even through privileged operations.

## Security tiers

<a id="g5.security.tiers"></a>
[D · g5.security.tiers]

G5 ranks its security invariants in two tiers by the boundary a failure would cross. This gives one set of invariants whatever the actor, and an order for triage. The same outcome reached by code in a VM, a leaked credential or an outsider is judged at the boundary it crosses.

### Tier 0: the organization boundary

<a id="g5.security.tier0"></a>
[I · g5.security.tier0]

Tier 0 is the platform and organization boundary; a failure there could end the business.

- Here "nobody" means people with no membership in the organization, people removed or deactivated from it, and machines of another organization or of none: workspace VMs, API keys and worker hosts.
- Removal takes effect as [current authority](security.g5.md#g5.security.current-authority) commits to it: new admission is refused from that commit, open event streams close within 30 seconds, and delivered bytes are not recalled.

<a id="g5.security.tier0.identity"></a>
[I · g5.security.tier0.identity]

Nobody uses G5 without signing in through GitHub or Microsoft Entra ID ([login providers](../features/users.g5.md#g5.user.login-providers)).

- Sign-in admits an identity only with a pending invitation or an existing membership; there is no self-service entry ([admission](../features/users.g5.md#g5.user.admission)).
- A session whose last active membership ends becomes account-only ([session authority](../features/users.g5.md#g5.user.session-authority)).
- Service identities, API keys and VM credentials hold no person session and cannot sign in ([credential classes](security.g5.md#g5.security.credential-classes)).

<a id="g5.security.tier0.outsiders"></a>
[I · g5.security.tier0.outsiders]

Nobody sees or changes an organization’s data, or uses its compute, unless they are a current active member of it. This specializes [tenancy](#g5.security.tenant) for the actors tier 0 names.

- Data is guarded twice: every query is tenant-scoped, and forced [row-level security](security.g5.md#g5.security.tenant.rls) blocks any cross-organization row, holding even the privileged system role to explicit checks so no single mechanism is trusted alone.
- Stored and transported data is [encrypted](security.g5.md#g5.security.encryption).
- Compute stays separate: a [host](../software_architecture/hosts.g5.md#g5.host.tenancy) serves one organization, so no two organizations’ VMs share a hypervisor, and API keys and VM credentials are pinned to their organization.
- Access ends with [active membership](security.g5.md#g5.security.active-membership); open [streams](security.g5.md#g5.security.membership-streams) close within the bound in [the tiers](#g5.security.tiers).
- Platform operators see only organization-level facts ([operator boundary](../features/organizations.g5.md#g5.organization.platform-operator.tenant-boundary)).

<a id="g5.security.tier0.host-reach"></a>
[I · g5.security.tier0.host-reach]

A compromised worker host exposes only secrets and credentials tied to its own organization.

- A host holds only that organization’s material: its host credential and the secrets, credentials and incarnation tokens sent to the VMs placed on it.
- Another organization’s hosts and VMs take no traffic from it: each organization has its own [VPC](../software_architecture/hosts.g5.md#g5.host.vpc) and a host’s firewall admits only that network. Hosts within one organization reach each other, and a host may reach the internet.
- The host’s cloud identity is shared by every organization’s hosts in a deployment. It grants only log and metric writes, read of the platform’s image and worker-image registries that every host needs to boot, and read of the whole guest compile-cache prefix, every organization’s sub-prefix included ([cache fleet](../software_architecture/hosts.g5.md#g5.host.cache-fleet)). Guests cannot reach the cloud metadata service ([guest egress](security.g5.md#g5.security.guest-egress)).
- The shared identity is accepted by design: leaking it gives telemetry writes and reads of platform-built images and of the deployment’s own compiled cache objects, not another organization’s secrets, credentials or repositories.

<a id="g5.security.tier0.log-content"></a>
[I · g5.security.tier0.log-content]

Customer content never leaves the tenancy boundary through platform logs, telemetry or error messages. This is the side-channel counterpart of [redaction](security.g5.md#g5.security.redaction).

- Logs, traces and error reports carry identifiers, shapes and reasons — never repository content, chat or document text, or secret values.
- An error returned to a caller never includes another organization’s data.
- The rule holds for operator-visible cloud logs as for responses.

### Tier 1: inside an organization

<a id="g5.security.tier1"></a>
[I · g5.security.tier1]

Tier 1 lies inside one organization, between its authorized people; a failure there is a bad experience, not a breach of tenancy. Authorized members are trusted colleagues, so a tier 1 finding that needs a member to act on purpose may be deferred.

<a id="g5.security.tier1.attribution"></a>
[I · g5.security.tier1.attribution]

Nobody creates a ledger record that looks as if another person made it. The ledger is every durable record naming a person as actor: Git history, chat, comments, suggestions, votes, approval decisions, issue writes, and G5’s writes at a provider such as GitHub.

- Every record takes its actor from the authenticated credential, never from the request, so identity cannot be forged through the G5 API or a G5-vended credential.
- A person’s provider writes, such as opening a pull request, use that person’s own connected account ([pull request authorship](../software_architecture/git.g5.md#g5.git.pr-authorship), [authorization loss](../software_architecture/git.g5.md#g5.git.authorization-loss)).
- G5’s own writes as the organization’s GitHub App, such as [automatic publication](../software_architecture/git.g5.md#g5.git.auto-publication) and ordinary merges, name no person.
- A credential acts for one person: an [API key](../features/users.g5.md#g5.user.api-key-authority) for whoever it was issued to, a VM credential for the workspace’s current owner ([agent scope](security.g5.md#g5.security.agent-workspace-scope), [agent audit](security.g5.md#g5.security.agent-audit)).
- Commit text is the exception: author and committer are advisory, and the recorded push is the attribution ([decision](../software_architecture/git.g5.md#g5.git.commit-identity-question)).

<a id="g5.security.tier1.ownership"></a>
[I · g5.security.tier1.ownership]

Nobody writes into an object that belongs to someone else.

- A workspace’s [branches](../features/workspaces.g5.md#g5.workspace.branch), VM, files, execution, terminal and preview ([owner authority](../software_architecture/workspace_vm.g5.md#g5.security.owner)), its live chat ([chat authority](../features/chat.g5.md#g5.chat.authority)) and its configuration ([configuration authority](../features/workspaces.g5.md#g5.workspace.configuration-authority)) are written only by its current owner or that owner’s workspace agent.
- A VM push updates only its own workspace’s branches, or its owner’s personal branch through the [personal mount](../features/personal_workspace.g5.md#g5.personal.mount-push) ([VM authority](../software_architecture/git.g5.md#g5.git.vm-authority)).
- A child’s work enters its parent’s branch only through a G5 [merge](../features/workspaces.g5.md#g5.workspace.merge) by someone with merge authority: the parent’s current owner, an administrator for [Base](../features/workspaces.g5.md#g5.governance.pr.base-admin), or the child’s owner under [source-owner merge](../features/workspaces.g5.md#g5.governance.pr.source-owner-merge). Nobody pushes into the parent.
- Administration grants no bypass beyond what other blocks sanction, such as [stopping a VM](../software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.by-user) and [approval administration](../features/workspaces.g5.md#g5.workspace.approval-administration). An administrator changes ownership only by explicit [transfer](../features/workspaces.g5.md#g5.workspace.transfer) and waives G5’s merge requirements only by a recorded [override](../features/workspaces.g5.md#g5.workspace.merge.admin-override), never by writing as the owner.
- Personal records — notifications, settings, connected accounts — belong to their person alone.

<a id="g5.security.tier1.administration"></a>
[I · g5.security.tier1.administration]

Only a current organization Admin changes organization-scoped state: memberships and roles, invitations, organization settings, secrets, provider accounts and API keys, repositories and hosts ([administrators](../features/organizations.g5.md#g5.organization.admins), [Admin settings](../features/organizations.g5.md#g5.security.org-admin-settings)). Membership changes preserve the last active Admin under [administrator continuity](../features/organizations.g5.md#g5.organization.ownership). Members and viewers administer nothing organization-wide.

<a id="g5.security.tier1.visibility"></a>
[I · g5.security.tier1.visibility]

Nobody sees what they are not authorized to see, and read authority is the same at every layer. Every active member reads every ordinary workspace’s records and branches, and repository read covers every branch and flush snapshot ([repository permissions](../features/repositories.g5.md#g5.repo.permissions)).

- The exceptions under [audience](../features/workspaces.g5.md#g5.workspace.audience): a formal review workspace is [human read-only](../features/agent_approvals.g5.md#g5.agent-approval.human-read-only), and a personal workspace is read by its owner and administrators only ([personal access](../features/personal_workspace.g5.md#g5.personal.access)).
- [Notifications](../features/users.g5.md#g5.user.notifications) and personal settings are visible to their person alone.
- A stored secret value is never shown back in full and is [redacted](security.g5.md#g5.security.redaction) from output.
- One exception by design: a workspace’s configured secrets reach its owner’s VM as plain environment variables for the owner’s agents, terminal and previews ([dispatch secrets](../features/chat.g5.md#g5.chat.dispatch-secrets), [terminal admission](../features/terminal.g5.md#g5.terminal.secret-admission)). This is an accepted, unmitigated threat: an agent can read or leak a secret, and G5 declares that rather than hiding a secret from the person whose VM runs with it.

<a id="g5.security.tier1.platform-integrity"></a>
[I · g5.security.tier1.platform-integrity]

Nobody runs their own code on a worker host or inside G5 itself, whether by escaping a workspace VM or by tampering with what G5 builds and ships.

- Customer code runs inside the [microVM boundary](../software_architecture/hosts.g5.md#g5.host.isolation), which must contain [guest root](security.g5.md#g5.security.guest-authority) and which G5 keeps patched.
- Tenancy does not rest on it: the microVM was adopted for dependency isolation first, a breakout is treated as possible over time (critical severity, medium-to-high likelihood), and the [host rule](#g5.security.tier0.host-reach) bounds what a breakout exposes to one organization.
- What G5 deploys is built from pinned, committed sources into immutable artifacts ([immutable images](../software_architecture/hosts.g5.md#g5.host.immutable-image), [artifact authority](../features/operations.g5.md#g5.quality.delivery.artifact-authority)). Every deployed image is scanned at delivery and re-checked as new vulnerabilities appear ([image CVE reporting](../features/operations.g5.md#g5.quality.delivery.image-cve-reporting)).

<a id="g5.security.permission-model"></a>
[D · g5.security.permission-model]

The permission model, read as one table.

- A viewer reads every ordinary workspace and repository, may discuss and suggest, and is granted write permission on nothing ([viewers](../features/organizations.g5.md#g5.organization.viewers)).
- A member does everything a viewer does, owns workspaces, and writes only to the workspaces they own ([members](../features/organizations.g5.md#g5.organization.members)).
- An administrator does everything a member does, administers the organization, reads every personal workspace, may [transfer](../features/workspaces.g5.md#g5.workspace.transfer) any workspace to any active owner-eligible member, and may merge with an override where they may already merge.
- Organization roles are Admin, Member and Viewer; role changes preserve [administrator continuity](../features/organizations.g5.md#g5.organization.ownership).
- There is no per-workspace read restriction; private workspaces are a future feature ([audience](../features/workspaces.g5.md#g5.workspace.audience)).
- Administration never includes using another member’s VM or writing as them.
- A delegated credential holds at most the authority of the person it acts for, within its scopes.

## Metadata

### g5.invariant

- agent
- Defines the `g5.invariant` level.
- Children: [Migrations](#g5.invariant.migrations).

### g5.invariant.migrations

- human-provided
- Expands: [System invariants](#g5.invariant).
- Source: [Ben, line 336](../../../docs/ben_new_ontology.md#L336).

### g5.architecture

- agent
- Defines the `g5.architecture` level for this and other pages.
- Children: [Commands](#g5.architecture.commands).

### g5.architecture.commands

- human-provided
- Expands: [Architecture invariants](#g5.architecture).
- Source: [Ben, line 338](../../../docs/ben_new_ontology.md#L338).

### g5.security.tenant

- human-provided
- Source: [Ben, line 337](../../../docs/ben_new_ontology.md#L337).

### g5.security.tiers

- human-provided
- security
- Children: [Tier 0](#g5.security.tier0), [Tier 1](#g5.security.tier1).
- Source for every block in this section: the owner's threat model of 2026-10-30, written after the 2026-10-29 meeting with James Moore, with the owner's decisions and the meeting's points ([request](../../chat_intent/security-invariants/request.md)); [design](../../pr_ontology_diff_design/security-invariants/ontology-plan.md). Code facts cited in this section were checked at `c351b446c` on 2026-10-30 and re-checked on 2026-11-06 at main `89eeea5e4` after the branch was rebased ([what was checked](../../pr_ontology_diff_design/security-invariants/ontology-plan.md#facts-re-checked-and-where-the-request-was-narrowed)). The owner's [audit of 2026-11-02](https://github.com/G5Labs/G5-Lean/issues/1047) checked every block of this section against main `01a9dfa70`; its wording corrections are applied, and the contradictions it left to the owner are recorded under the blocks they concern.

### g5.security.tier0

- human-provided
- security
- Expands: [Security tiers](#g5.security.tiers).
- Children: [Identity](#g5.security.tier0.identity), [Outsiders](#g5.security.tier0.outsiders), [Host reach](#g5.security.tier0.host-reach), [Log content](#g5.security.tier0.log-content).
- The owner's tier 0 definition. The removal bound restates the [issue #610](https://github.com/G5Labs/G5-Lean/issues/610) fix as [current authority](security.g5.md#g5.security.current-authority) records it.

### g5.security.tier0.identity

- human-provided
- security
- Expands: [Tier 0](#g5.security.tier0).
- The owner's tier 0 bullets on identity providers and membership; the linked blocks carry the evidence.

### g5.security.tier0.outsiders

- human-provided
- security
- Expands: [Tier 0](#g5.security.tier0).
- The owner's tier 0 bullet on current members only, with the meeting's defense-in-depth point. Specializes [g5.security.tenant](#g5.security.tenant). Migration 087 added row-level security to `notifications`, `usage_records` and `provider_quota_snapshots`, which an earlier trace had found unprotected; the 2026-11-02 audit found the forced policies in place for the tenant role on every reachable table.

### g5.security.tier0.host-reach

- human-verified
- security
- Expands: [Tier 0](#g5.security.tier0).
- The owner's two worker-host bullets, qualified by the infrastructure: same-organization hosts reach each other and the internet (meeting of 2026-10-29), and the shared identity is accepted by decision 4. Each organization gets a VPC `g5-org-<id>` with an allow-internal firewall rule (`backend/g5-api-core/src/cloud/gcp.rs`). Hosts carry the identity named by `G5_WORKER_SERVICE_ACCOUNT`, required for the cloud provider to start; its grants are `roles/logging.logWriter` and `roles/monitoring.metricWriter` (`infra/terraform/modules/iam/main.tf`), `roles/artifactregistry.reader` on the `g5`, `g5-tiers` and `g5-worker-images` registries, object read on the `g5-worker-v*` objects of the worker-image bucket, and object read on the `guest-sccache/` prefix (`infra/qa/platform/main.tf`, re-checked 2026-11-06 at main `89eeea5e4`). The `cloud-platform` OAuth scope the instance requests grants nothing beyond those roles. Decision 4 named only the cache read; the registry and worker-image reads are what a host needs to boot (audit 2026-11-02, C5).
- The cache read is not organization-scoped: the grant covers all of `guest-sccache/`, whose per-organization sub-prefixes hold objects the deployment's producer compiles from the platform's own sources (`warm-guest-sccache` in `infra/ci/cloudbuild-deploy.yaml`); the host's cache door serves only its own prefix, which does not bind a leaked cloud credential. `docs/code_gaps.md` records the breadth as RUNTIME-006, and [cache fleet](../software_architecture/hosts.g5.md#g5.host.cache-fleet) still describes a per-organization read credential. Decision 4 accepts the shared grant for the host identity; whether that clause of the cache block should be amended is left to the owner.
- [Issue #611](https://github.com/G5Labs/G5-Lean/issues/611) should be closed with this decision. Not verified: that the live hosts already run as the dedicated identity rather than the project default. The issue's separate points on host JWT delivery through instance metadata are not settled here.

### g5.security.tier0.log-content

- human-verified
- security
- Expands: [Tier 0](#g5.security.tier0).
- The meeting's point as the owner recorded it; the scope wording (identifiers, shapes and reasons; operator-visible cloud logs) is the agent's. Whether today's logging meets this has not been audited ([follow-up](../../pr_ontology_diff_design/security-invariants/ontology-plan.md#follow-up-work)).

### g5.security.tier1

- human-provided
- security
- Expands: [Security tiers](#g5.security.tiers).
- Children: [Attribution](#g5.security.tier1.attribution), [Ownership](#g5.security.tier1.ownership), [Administration](#g5.security.tier1.administration), [Visibility](#g5.security.tier1.visibility), [Platform integrity](#g5.security.tier1.platform-integrity).
- The owner's tier 1 definition and the trusted-colleagues rule; issue #610 restates it ("under the rule that authorized users are trusted colleagues this was treated as deferred, with offboarding noted as the case where that trust ends").

### g5.security.tier1.attribution

- human-provided
- security
- Expands: [Tier 1](#g5.security.tier1).
- The owner's first tier 1 bullet with the meeting's ledger definition and impersonation wording; decision 2 settles commit text ([commit identity](../software_architecture/git.g5.md#g5.git.commit-identity-question)). The App-installation clause answers the 2026-11-02 audit's C3: App writes impersonate nobody, because the bot is not a person. Record routes take the actor from the authenticated principal, for example `auth.require_user_id()` in `backend/g5-api-routes/src/routes/comments.rs`.

### g5.security.tier1.ownership

- human-provided
- security
- Expands: [Tier 1](#g5.security.tier1).
- The owner's second tier 1 bullet. The parent-merge rule is the owner's decision of 2026-11-06 ([request](../../chat_intent/security-invariants/request.md)), which replaces the meeting's 2026-10-30 reading that only the parent's owner lands the merge with every merge authority, the #1104 source-owner path included. Checked 2026-11-06 at main `c825367d5`: `merge_workspace` in `backend/g5-api-core/src/services/workspace/workspace_service.rs` refuses a user caller who does not own the destination, and the merge routes admit a source owner through `workspace_mutation_policy.rs::source_owner_may_merge`. The personal-mount and sanctioned-administration clauses answer the 2026-11-02 audit's C7 and C4; the audit's H2 (a member advancing Base through the provider with the App token and auto-pull, with no G5 merge) is a recorded violation in [issue #1047](https://github.com/G5Labs/G5-Lean/issues/1047), not accepted behavior.

### g5.security.tier1.administration

- human-provided
- security
- Expands: [Tier 1](#g5.security.tier1).
- The owner's bullet "Only organization Admin can change org scoped settings"; the list of organization-scoped state restates the linked blocks. The Owner-role clauses are superseded by [issue #1106](https://github.com/G5Labs/G5-Lean/issues/1106): Admin, Member and Viewer remain, with last-active-Admin continuity. That retirement preserves existing additive-grant and repository-import behavior; the broader C1/C2 policy gap below remains separate.
- Open for the owner (audit 2026-11-02, [issue #1047](https://github.com/G5Labs/G5-Lean/issues/1047), C1 and C2): an administrator can grant `org:admin` to a member through [grants](../features/organizations.g5.md#g5.organization.grant-lifecycle-question), and `update_member_role` then lets that member promote themselves; eligible ordinary members may [import repositories](../software_architecture/git.g5.md#g5.security.tenant.scm-import) without administrator status. Either the grants are capped and import requires an administrator, or this statement is amended; neither is decided here.

### g5.security.tier1.visibility

- human-provided
- security
- Expands: [Tier 1](#g5.security.tier1).
- The owner's bullets on authorized access and on secrets as environment variables, read with decision 5 and the meeting's acceptance of that exposure as unmitigated. The configuration routes return at most a bounded hint of a stored secret value (`secret_key_hints` in `backend/g5-api-routes/src/routes/config.rs`).
- Gap (audit 2026-11-02): row-level security does not encode the personal-workspace boundary; the API, UI, streams and Git enforce it. Configurable audiences are gone with migration 149, so no invite-only boundary remains to encode.

### g5.security.tier1.platform-integrity

- human-provided
- security
- Expands: [Tier 1](#g5.security.tier1).
- The owner's bullets on foreign code and on the CVE scan, kept on tier 1 by decision 1 and widened to every deployed image by decision 3; severity, likelihood and the dependency-isolation origin are the meeting's. Gap: as of 2026-11-06 the scan covers only part of the deployed images, at deploy time only; which images are scanned today is recorded under [image CVE reporting](../features/operations.g5.md#g5.quality.delivery.image-cve-reporting).

### g5.security.permission-model

- human-provided
- security
- The owner's bullet "Write down the permission model: Admin can transfer ownership of any workspace to anybody. Anyone can read branch as the basic permission for each org member is a viewer of the workspace", read with decision 5. "Anybody" is bounded by the [transfer](../features/workspaces.g5.md#g5.workspace.transfer) rule's active, owner-eligible recipient, which the code enforces (so viewers, who cannot own workspaces, are excluded).
- Owner hand-over and step-down are superseded by [issue #1106](https://github.com/G5Labs/G5-Lean/issues/1106), which retires organization Owner rather than introducing handover. Existing Owners become ordinary Admins and all supported membership changes preserve the final active Admin; the [retirement design](../../pr_ontology_diff_design/issue-1106/ontology-plan.md) retain separate workspace ownership and historical attribution.
