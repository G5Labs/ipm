# Organization

<a id="g5.organization"></a>
[I · g5.organization]

An organization is a tenant of G5: a group of people with roles who share workspaces, repositories, hosts and settings, isolated from every other organization.

<a id="g5.organization.membership"></a>
[I · g5.organization.membership]

Each organization retains its memberships and their roles: admin, member or viewer. Organization Owner is retired; its former holders are ordinary Admins with unchanged identities and workspace ownership. Only [active memberships](../concepts/security.g5.md#g5.security.active-membership) authorize use of its resources.

<a id="g5.organization.viewers"></a>
[I · g5.organization.viewers]

Organization viewers may observe resources they can access but cannot be granted write permission on anything. [Discussion and suggestions](#g5.organization.viewer-collaboration-question) are permitted participation; [notification read state](users.g5.md#g5.user.notifications) belongs to the recipient.

<a id="g5.organization.viewer-ceiling"></a>
[D · g5.organization.viewer-ceiling]

Additive authority (organization default scopes, individual grants and workspace overrides) cannot raise a viewer above read-only scopes. Current authority at admission and the scopes issued in tokens apply the same ceiling, so a configured `org:admin` or write grant on a viewer membership confers nothing. Members and administrators keep their configured additive grants under the [grant rule](#g5.organization.grant-lifecycle-question).

<a id="g5.organization.grant-lifecycle-question"></a>
[D · g5.organization.grant-lifecycle-question]

Organization default scopes, individual grants and workspace overrides remain supported. For administrators and members they add to the role's own scopes, including scopes the role lacks. Only viewers are capped, by the [viewer ceiling](#g5.organization.viewer-ceiling), because the [viewer rule](#g5.organization.viewers) forbids granting them write permission.

<a id="g5.organization.members"></a>
[I · g5.organization.members]

Organization members can own workspaces and have read and write permission on organization resources.  

<a id="g5.organization.admins"></a>
[I · g5.organization.admins]

Organization admins can do everything a member can, invite users, change other users' roles, remove other users, change organization settings, and grant or revoke permissions on organization-owned entities for themselves or others. Membership changes preserve [administrator continuity](#g5.organization.ownership), existing self-action restrictions, workspace-transfer requirements and unfinished cleanup.

<a id="g5.organization.ownership"></a>
[D · g5.organization.ownership]

Every supported tenant creation and membership-management operation preserves at least one active Admin, decided by application code atomically with the complete membership change. Only an Admin membership without a deactivation timestamp counts; invitations, inactive Admins and additive `org:admin` grants do not, and an execution hold does not remove activity.

- Concurrent removal, demotion, deactivation and retries cannot remove the final active Admin, and a refusal, error or cancellation leaves no partial membership, personal-workspace retirement, execution hold or access revocation.
- Supported writers must use this application boundary; privileged manual SQL and repair bypass it and own their intended final state.
- Whole-organization deletion remains valid, and the [operator organization](#g5.organization.platform-operator.bootstrap) keeps its bootstrap and recovery exception.

<a id="g5.organization.invites"></a>
[I · g5.organization.invites]

Organization invitations become memberships when the invited user logs in, or expire after a reasonable time. [Existing accounts join immediately](#g5.organization.existing-user-invitation-question).

<a id="g5.organization.host-limits"></a>
[I · g5.organization.host-limits]

Organization policies include scaling limits on the number of hosts.

<a id="g5.organization.owner-limits"></a>
[I · g5.organization.owner-limits]

Organization defaults cap how many VMs one owner may have running and how many workspaces one user may own, so runaway agents are stopped before they consume the organization's resources. A path that lets a user exceed a limit is acceptable when it is not readily exploitable; lowering a limit, for example, does not stop running VMs. Workspaces count only for their owner, active or deactivated.

<a id="g5.organization.tenancy"></a>
[I · g5.organization.tenancy]

An organization is a strong tenancy boundary: its resources — workspaces, hosts, workspace VMs, repositories — are inaccessible to other organizations. Every store of organization-specific information must protect that boundary, for example through database [row-level access control](../concepts/security.g5.md#g5.security.tenant.rls) or separate host pools. Platform operators see only the organization-level facts the [operator tenant boundary](#g5.organization.platform-operator.tenant-boundary) allows.

<a id="g5.team"></a>
[D · g5.team]

A team is the working structure created with an organization — its repositories and Base workspace — so members can start work as soon as it exists.

<a id="g5.team.onboarding"></a>
[D · g5.team.onboarding]

Organization creation commits its initial Admin membership, dedicated docs repository, internal code repository and Base workspace metadata together so setup cannot leave a partly created team.

<a id="g5.organization.agent-member-directory"></a>
[D · g5.organization.agent-member-directory]

A workspace agent can look up its organization's members as its owner reads them on the Members page: each member's user id, name, email, role and active-or-deactivated state, without pending invitations. Each lookup rechecks the caller incarnation and the owner's active membership, reads only that organization, and is unaudited, like [agent reads](../concepts/security.g5.md#g5.security.agent-read-scope).

- Each member also lists the accounts they connected to G5 at external providers, each a provider and login (today only GitHub), so the agent can name a person as an assignee in any issue domain.
- Words match a name, an email or a login.

## Member deactivation

<a id="g5.organization.deactivation"></a>
[I · g5.organization.deactivation]

An active organization administrator can deactivate another admin, member or viewer in that organization; the last active administrator is protected by [continuity](#g5.organization.ownership). The person then cannot enter or use that organization through existing or new sessions, CLI credentials or API keys. Access to other organizations is unaffected.

<a id="g5.organization.deactivation.retention"></a>
[I · g5.organization.deactivation.retention]

Deactivation retains membership, role, workspace ownership and history without deletion or reassignment. Activity is independent of role-based ownership eligibility, so existing foreign-key references remain valid. [Workspace transfers](workspaces.g5.md#g5.workspace.transfer) remain separate administrative actions.

<a id="g5.organization.reactivation"></a>
[I · g5.organization.reactivation]

Only an explicit administrator Reactivate action restores a deactivated membership's access under its retained role and permissions; login, invitation acceptance and role changes do not reactivate it. Workspaces remain stopped until deliberately started. Unfinished [deactivation shutdown](../software_architecture/workspace_vm.g5.md#g5.lifecycle.member-deactivation.recovery) still blocks compute after access is restored.


## Invitation authority

<a id="g5.organization.invitation-authority"></a>
[D · g5.organization.invitation-authority]

Login may grant membership only from a current, pending and unexpired invitation for the verified identity; [existing accounts may join when invited](#g5.organization.existing-user-invitation-question). Acceptance and revocation resolve against the same current invitation; a completed revocation cannot later create membership.


<a id="g5.organization.existing-user-invitation-question"></a>
[D · g5.organization.existing-user-invitation-question]

Inviting an email that already belongs to a G5 account adds that person to the organization at once, with the invited role, as an exception to the [login rule](#g5.organization.invites). Only an address with no account gets a pending invitation, which becomes a membership at the invitee's verified login.

## Invitation email

<a id="g5.organization.invitation-email"></a>
[D · g5.organization.invitation-email]

When a new pending invitation commits, G5 requests one [outbound email](../software_architecture/email.g5.md#g5.email.handoff) to its address, naming the organization, the inviter and the offered role. The link opens the organization's page, which requires the invitee's verified login, and that login accepts the invitation under [invitation authority](#g5.organization.invitation-authority); the email carries no token and grants nothing.

- No email is sent when the invite fails or rolls back, when the address already belongs to a member or a pending invitation, or when an existing user is added immediately.
- A valid invitation is still created and kept when email is unavailable or the address is one G5 cannot send to.
- Membership-added notifications are outside this slice.

<a id="g5.organization.invitation-email-validity"></a>
[D · g5.organization.invitation-email-validity]

An invitation email is [still wanted](../software_architecture/email.g5.md#g5.email.validity) only while its invitation exists, is pending and unexpired, and is addressed to the same recipient. A revoke, accept, decline or expiry that commits before sending cancels the queued email.

<a id="g5.organization.invitation-resend"></a>
[D · g5.organization.invitation-resend]

Resend extends a pending invitation's expiry and requests a new invitation email; only creation and an administrator's resend request email, and [automatic retries](../software_architecture/email.g5.md#g5.email.identity) never count as resends.

- If the previous email was sent, failed or cancelled, the recipient gets another.
- If an earlier email is still queued or sending, resend makes it due now instead of adding one, so repeated presses during an outage produce a single message.

<a id="g5.organization.invitation-email-limit"></a>
[D · g5.organization.invitation-email-limit]

An organization can request at most 100 new invitation emails, from invites and resends together, in any rolling 24 hours ([volume limits](../software_architecture/email.g5.md#g5.email.volume-limits)); an invite or resend beyond that is refused as rate limited, with a retry time, and nothing changes. A resend that coalesces into a live email does not count. No limit applies with email disabled, or in a local development stack that turns volume limits off.

<a id="g5.organization.invitation-email-status"></a>
[D · g5.organization.invitation-email-status]

The administrators' pending-invitation list shows each invitation's latest email status (queued, sent, failed, possibly sent or cancelled), so an administrator can see a failure and resend.


## Collaboration scope

<a id="g5.organization.viewer-collaboration-question"></a>
[D · g5.organization.viewer-collaboration-question]

An organization viewer who can read a workspace may discuss there: start threads and reply in any thread, including a review suggestion's, under [comment authority](documents/discussion.g5.md#g5.documents.comments.authority), and propose or withdraw [reader suggestions](documents/files.g5.md#g5.documents.reader-suggestions). Neither changes workspace content until the owner decides, so neither is a write under the [viewer rule](#g5.organization.viewers).

- Viewers may not cast or change [question votes](documents/discussion.g5.md#g5.documents.question-votes) or [approve](workspaces.g5.md#g5.workspace.approval-human-authority), because votes and approvals weigh on decisions, in which viewers take no part.

## Organization administration

<a id="g5.security.org-admin-settings"></a>
[D · g5.security.org-admin-settings]

Only current organization administrators can see the Admin page and its entry. Reading or writing its settings also requires `org:admin` and a current Admin role in the target organization. Linked administrative pages enforce their own access rules; workspace owners retain their separate authority over workspace settings.

<a id="g5.settings"></a>
[I · g5.settings]

Settings are an organization's configurable defaults and controls, owned by the Admin page under administrator authority.

<a id="g5.settings.organization-configuration"></a>
[D · g5.settings.organization-configuration]

The Admin page owns organization runtime defaults, settings, secrets, provider-account management and organization API-key controls under [administrator authority](#g5.security.org-admin-settings); workspace settings provide no organization mutation controls. Organization defaults still participate in effective configuration, and visible organization provider accounts remain selectable for workspace use. Moving the management surface does not delete organization data or change configuration precedence.

<a id="g5.data"></a>
[D · g5.data]

G5's durable data is organized by store, each store owning its own schema, validation and tenant isolation.

<a id="g5.data.configuration"></a>
[D · g5.data.configuration]

Configuration data — an organization's typed settings and defaults — has one validated owner per store, so reads and writes go through it rather than ad hoc columns.

<a id="g5.data.configuration.org-admin-settings"></a>
[D · g5.data.configuration.org-admin-settings]

One typed, database-validated configuration owner governs the organization's runtime defaults; missing keys use platform defaults, editing one field preserves the others, and schema upgrades neither seed nor rewrite existing settings. The Admin page exposes these values and validation feedback under [administrator authority](#g5.security.org-admin-settings).

- It governs the [personal idle default](personal_workspace.g5.md#g5.personal.capacity), VM quota and [per-owner limits](#g5.organization.owner-limits).
- It governs managed-host disk allowances H and V, growth, machine type, free-slot floor, removal and removal delay.
- It governs whether interrupted chat turns [continue automatically](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start.automatic-continuation).
- Each setting's effect is owned elsewhere: [idle defaults](../software_architecture/workspace_vm.g5.md#g5.lifecycle.idle.settings), [quota](../software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity), [owner VM limit](../software_architecture/host_control_plane.g5.md#g5.runtime.placement.owner-limit), [owner workspace limit](workspaces.g5.md#g5.workspace.owner-limit) and [managed capacity](../software_architecture/host_control_plane.g5.md#g5.architecture.host_demand_capacity).


## Platform operator

<a id="g5.organization.platform-operator"></a>
[I · g5.organization.platform-operator]

Platform operators oversee the whole deployment, unlike organization administrators. They are modeled as one well-known organization, `operator`, created at bootstrap, so its ordinary memberships, roles and invitations form the operator roster.

- Administrators invite platform operators.
- Members create and list organizations, monitor the platform and adjust provisioning.
- Viewers see deployment state but change nothing.

<a id="g5.organization.platform-operator.home"></a>
[I · g5.organization.platform-operator.home]

The operator organization's Home, at `/operator`, is about platform operation rather than development tasks. It lists every organization's [platform facts](#g5.organization.platform-operator.tenant-boundary), and members and above can [create an organization](#g5.organization.platform-operator.create-org) from it. Its Base workspace, docs repository, Members and Admin pages are the ordinary ones.

<a id="g5.organization.platform-operator.identity"></a>
[D · g5.organization.platform-operator.identity]

An organization kind (`tenant` or `operator`), not its slug, identifies the operator organization, and at most one exists; only [platform seeding](#g5.organization.platform-operator.bootstrap) creates it, apart from the dev and E2E test-token fixture, so authority never derives from a name a tenant could choose. The operator organization's slug is always `operator` and cannot change.

- A new or renamed tenant organization cannot take a reserved slug: `operator`, `admin`, `api`, or a top-level route segment (`login`, `landing`, `account`, `cli`, `forbidden`, `settings`, `projects`, `no-organization`, and the retired `create-org`).
- Existing slugs are kept.

<a id="g5.organization.platform-operator.bootstrap"></a>
[D · g5.organization.platform-operator.bootstrap]

Platform seeding provisions only the operator organization and creates no tenant organization. It creates the operator organization with its [Base repositories](#g5.organization.platform-operator.base-seed) and gives the deployment's first operator email a pending administrator [invitation](#g5.organization.invitation-authority) unless that person is already a member. Later operators are invited by operator administrators, and tenant organizations are created afterward through [operator creation](#g5.organization.platform-operator.create-org).

- The seed's service identity writes these rows and is the inviter of record. It holds only the member role, because every live workspace needs an eligible member owner; it cannot sign in, so it never exercises [operator authority](#g5.organization.platform-operator.authority). The operator organization is exempt from [administrator continuity](#g5.organization.ownership).
- Seeding is idempotent: re-running restores administrator access if the last administrator is removed or demoted, and a single run gives an existing deployment its operator organization without touching its tenants.
- Self-hosted installers keep creating their single customer organization and provision no operator organization, pending the [self-hosted question](#g5.organization.platform-operator.self-hosted-question).
- Local dev and preview stacks mirror seeding: the seed makes the dev administrator persona an operator administrator.

<a id="g5.organization.platform-operator.base-seed"></a>
[D · g5.organization.platform-operator.base-seed]

Platform seeding gives the operator Base the structure [organization creation](#g5.team.onboarding) gives tenants: a docs repository and an internal code repository, each with a first commit, so children of Base start on the organization default image ([image selection](../software_architecture/hosts.g5.md#g5.host.image-selection)).

- The first commits need the seed to reach the API's repository storage; without it, the seed writes the repositories, warns that Base cannot be forked yet, and a later run writes the commits.
- Seeding again adds only a missing code repository or first commit. It never replaces an existing repository or its history, and a Base whose code repository an administrator attached keeps it.
- Seeding refuses rather than invent history when the organization's `code` repository is provider-backed and Base has no code repository, or when a nonempty repository mirror lacks its branch and needs restoring from backup.

<a id="g5.organization.platform-operator.authority"></a>
[D · g5.organization.platform-operator.authority]

Platform authority is the caller's current [active membership](../concepts/security.g5.md#g5.security.active-membership) in the operator organization, checked from a person session for each operation. Any role may observe; changes require Member or Admin. The operator roster is managed with ordinary administrator authority in the operator organization.

<a id="g5.organization.platform-operator.tenant-boundary"></a>
[D · g5.organization.platform-operator.tenant-boundary]

Operator membership grants nothing inside another organization: platform reads expose only each tenant's id, name, slug, creation time and active member count. Workspaces, repositories, chats, documents and hosts stay behind [tenancy](#g5.organization.tenancy) and [row-level access](../concepts/security.g5.md#g5.security.tenant.rls). Every platform read and change is audited with the acting user.

- Any break-glass access to tenant content would need its own audited mechanism.

<a id="g5.organization.creation"></a>
[D · g5.organization.creation]

Only platform operators [create organizations](#g5.organization.platform-operator.create-org), and only [platform seeding](#g5.organization.platform-operator.bootstrap) creates the operator organization; no self-serve creation route, page or Account-flyout entry exists. A person with no active organization sees that they need an invitation from an organization administrator. Self-hosted deployments create no organizations after installation.

<a id="g5.organization.platform-operator.create-org"></a>
[D · g5.organization.platform-operator.create-org]

An operator creates an organization for an initial Admin named by email, using the ordinary [cascade](#g5.team.onboarding) and slug rules; the operator gains no membership, and self-hosted deployments refuse it as they refuse other additional organizations.

- An existing account with that email, matched case-insensitively, becomes that Admin and owns Base; organization administration and workspace ownership remain separate.
- Otherwise G5 creates an account with no sign-in identity, and the person's first verified sign-in with that email links to it.

<a id="g5.organization.platform-operator.self-hosted-question"></a>
[Q · g5.organization.platform-operator.self-hosted-question]

In self-hosted mode, does the single organization's initial Admin act as platform operator, or does a second operator organization exist? Provisionally neither: self-hosted deployments have no platform operator.

<a id="g5.organization.platform-operator.operations-question"></a>
[Q · g5.organization.platform-operator.operations-question]

Which provisioning settings and deployment-health views do operators own, and should the license-provisioning token move under operator control? Candidates are per-organization [host limits](#g5.organization.host-limits) and the default base image track, while organization administrators keep their [Admin page settings](#g5.data.configuration.org-admin-settings). Provisionally none: the console lists and creates organizations, and the token stays deployment configuration.

## Metadata

### g5.organization

- agent
- Defines the `g5.organization` ID level: a tenant of G5. Composed from [Ben's ontology](../../../docs/ben_new_ontology.md#L25) membership and tenancy lines; detail is in [membership](#g5.organization.membership) and [tenancy](#g5.organization.tenancy).

### g5.organization.membership

- human-provided
- Source: [Ben, line 25](../../../docs/ben_new_ontology.md#L25).
- Activity distinction: [member-deactivation request](../../chat_intent/member-removal/request.md#human-instructions). [Issue #1106](https://github.com/G5Labs/G5-Lean/issues/1106) retires the former protected Owner role under the [retirement design](../../pr_ontology_diff_design/issue-1106/ontology-plan.md).

### g5.organization.viewers

- human-provided
- Source: [Ben, line 26](../../../docs/ben_new_ontology.md#L26).
- [ViewerCeiling](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/chat-identity/ViewerCeiling.tla) exposed [CI-G3](../../../docs/code_gaps.md): additive grants let a modeled viewer exercise write/admin authority, while a role ceiling prevented it. This bounded result concerns the grant mechanism, not a bypass of workspace-owner checks or a runtime exploit; [source evidence](../../uplift/ben-ontology/investigations/chat-identity.md) retains the policy conflict.

### g5.organization.viewer-ceiling

- agent
- Source: [viewers](#g5.organization.viewers) and the [grant rule](#g5.organization.grant-lifecycle-question); [issue #660](https://github.com/G5Labs/G5-Lean/issues/660), [design](../../pr_ontology_diff_design/issue-660/ontology-plan.md#4-viewer-ceiling-on-additive-grants). Closes CI-G3 for the viewer role.
- `PermissionService::scope_within_role_ceiling` is the one rule; `grants_within_role_ceiling` applies it to configured grant strings for locked admission, runtime predicates and effective-scope DTOs, and `AuthService::resolve_effective_scopes` (login, refresh, CLI exchange, test tokens) applies it to issued scopes. `api_credential_authority.rs` covers the populated `org:admin` fixture, a write grant and an execute default on a viewer; `api_org_admin_authority.rs` keeps a viewer with an `org:admin` grant refused while a member gains it.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 22 (OR-Q3).

### g5.organization.grant-lifecycle-question

- human-provided
- Source: [Identity coverage, unresolved policy](../../uplift/ben-ontology/reviews/coverage-identity.md#unresolved-policy-and-rejected-apparent-gaps); prior security retirement design, current PermissionService consumers and Ben's viewer ceiling.
- The [viewer ceiling](#g5.organization.viewer-ceiling) settles the viewer role (issue #660); administrators and members retain additive grants, including former Owners converted to Admin under issue #1106.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 23 (OR-Q3).

### g5.organization.members

- human-provided
- Source: [Ben, line 27](../../../docs/ben_new_ontology.md#L27).

### g5.organization.admins

- human-provided
- Source: [Ben, line 28](../../../docs/ben_new_ontology.md#L28).

### g5.organization.ownership

- human-provided
- Supersedes Owner handover: [issue #1106](https://github.com/G5Labs/G5-Lean/issues/1106), [retirement design](../../pr_ontology_diff_design/issue-1106/ontology-plan.md). Continuity is decided atomically with the membership change ([design](../../pr_ontology_diff_design/issue-1106/ontology-plan.md#concurrency-and-supported-writers)); migration 153 follows normal deployment under the owner's [review follow-up](../../chat_intent/issue-1106/review-followups.md). The same follow-up leaves removal after required workspace transfers unmet: Base transfer and completed transfer/fork references blocking membership deletion are existing main behavior outside PR #1111's scope. Reason: ordinary administrator offboarding must not leave a tenant without an administrator.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 24 (OR-Q1).

### g5.organization.invites

- human-provided
- Source: [Ben, line 29](../../../docs/ben_new_ontology.md#L29).
- Former code gap [CI-G1](../../../docs/code_gaps.md) (OAuth acceptance could outlive revocation) is closed by [issue #660](https://github.com/G5Labs/G5-Lean/issues/660) under [invitation authority](#g5.organization.invitation-authority). Existing accounts join at once: [existing-user invitation](#g5.organization.existing-user-invitation-question).

### g5.organization.host-limits

- human-provided
- Source: [Ben, line 30](../../../docs/ben_new_ontology.md#L30).

### g5.organization.owner-limits

- human-provided
- Source: [owner request and decisions](../../chat_intent/owner-resource-limits/request.md) (chat, 2026-10-22); the active-or-deactivated sentence is the owner's clarification.
- Enforced by [running VMs per owner](../software_architecture/host_control_plane.g5.md#g5.runtime.placement.owner-limit) and [workspaces per owner](workspaces.g5.md#g5.workspace.owner-limit).

### g5.organization.tenancy

- human-provided
- Source: [Ben, line 31](../../../docs/ben_new_ontology.md#L31).

### g5.team

- agent
- Defines the `g5.team` ID level: the working structure created with an organization. Source is the onboarding cascade; see [onboarding](#g5.team.onboarding).

### g5.team.onboarding

- human-provided
- Source: [Ben, line 32](../../../docs/ben_new_ontology.md#L32).
- The internal code repository is the existing cascade (`OrgService::create_org`, `bootstrap_base_code_repo`), recorded with [issue #911](https://github.com/G5Labs/G5-Lean/issues/911) so the operator seed can reference it.

### g5.organization.agent-member-directory

- human-verified
- Source: the owner's [2026-10-26 request](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace) for an agent user lookup that supports search, for assigning issues, and their answer to include each member's connected login in a provider-agnostic form. Reading the roster any member already reads, matching words and leaving lookups unaudited are agent choices ([design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-6-fork-with-an-issue-and-member-lookup)). The connected login is new to other members: the UI shows a person only their own.
- Implemented: `GET /api/internal/workspace-agent/:id/users` (`lookup_users` in the [workspace-agent routes](../../../backend/g5-api-routes/src/routes/workspace_agent.rs)) composes `OrgService::list_members` with `github_user_grants::connected_logins`; `g5d user search|view`.
- Evidence: `an_agent_looks_up_its_organizations_members_with_their_connected_accounts` ([tests](../../../backend/g5-api/tests/workspace_agent_parity.rs)).

### g5.organization.deactivation

- human-provided
- Source: [member-deactivation request](../../chat_intent/member-removal/request.md#human-instructions).
- Implemented by migration 099, `OrgService::set_member_active`, audited organization routes and member controls. [Implementation evidence](../../pr_ontology_diff_design/member-removal/ontology-plan.md#implementation-evidence) separates access denial from physical shutdown.

### g5.organization.deactivation.retention

- human-provided
- Source: [member-deactivation request](../../chat_intent/member-removal/request.md#human-instructions).
- Existing `org_memberships.owner_eligible_user_id` and `workspaces_owner_same_org_fk` enforce role eligibility. Activity must not be added to that generated key.

### g5.organization.reactivation

- human-verified
- Source: explicit reactivation and stopped-workspace requirements in the [request](../../chat_intent/member-removal/request.md#human-instructions); implicit-reactivation exclusion follows from the explicit action requirement.
- Agent decision: [recovery design](../../pr_ontology_diff_design/member-removal/ontology-plan.md#shutdown-and-concurrency) permits organization access during unfinished cleanup while retaining its compute barrier. Implemented with a retained cleanup identifier; exercised through HTTP and lifecycle integration tests.

### g5.organization.invitation-authority

- agent
- [Invitation](../../uplift/ben-ontology/investigations/chat-identity.md) showed OAuth's captured invitation granting membership after revocation (CI-G1); the intended shared-current-invitation boundary prevented that race and allowed valid acceptance. The bounded check assumes verified identity and database semantics.
- Implementation: [issue #660](https://github.com/G5Labs/G5-Lean/issues/660), [design](../../pr_ontology_diff_design/issue-660/ontology-plan.md#2-oauth-acceptance-resolves-against-the-current-invitation). `OrgService::accept_pending_invitations_on_conn` locks the matching pending invitations and accepts each conditionally inside the login transaction; `api_credential_authority.rs` races a real OAuth callback against an in-flight revoke. CI-G1 is closed.

### g5.organization.existing-user-invitation-question

- human-provided
- Source and implications: [chat/identity audit](../../uplift/ben-ontology/investigations/chat-identity.md), CI-Q1, org_service635–808.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 25 (OR-Q2).

### g5.organization.invitation-email

- human-verified
- Source: [issue #621 invitation integration](../../chat_intent/issue-621/request.md). The link target is the agent's reading of the "existing authenticated acceptance flow": no acceptance page or invitation token exists, and login acceptance is [invites](#g5.organization.invites). Design: [issue #621](../../pr_ontology_diff_design/issue-621/ontology-plan.md).
- Implementation: `OrgService::invite_member` requests the email in the invite transaction; the Members page shows the latest status.
- Evidence: `backend/g5-api/tests/email_delivery.rs` (including invite → message → OAuth login) and `frontend/e2e/flows/invite-members.spec.ts` "Invitation email — end to end" ([validation results](../../pr_ontology_diff_design/issue-621/ontology-plan.md#validation-results)). Production delivery is unverified until the owner provisions Resend.

### g5.organization.invitation-email-validity

- agent
- Source: issue #621 asks how revoked, expired or accepted invitations affect queued delivery. Revocation deletes the invitation row; expiry is the `expires_at` deadline whether or not the hourly sweep has marked it.

### g5.organization.invitation-resend

- agent
- Source: issue #621 asks for an explicit resend delivery contract. The older storage design said resend "re-sends the email" (`docs/reference/designs/API_AND_STORAGE.md`); before #621 resend only extended expiry.

### g5.organization.invitation-email-limit

- human-provided
- Source: owner answer `outbound-email` / `invitation-email-cap` = "Cap at 100 per organization per 24 hours"; the question's context said beyond-cap invites and resends are refused with 429 before anything is written (decision, 2026-10-17).
- Owner instruction (2026-10-19): "lift the 100 per org per day cap for local dev" ([chat intent](../../chat_intent/issue-621/request.md#local-cap-and-owner-email-command)).

### g5.organization.invitation-email-status

- agent
- Source: issue #621 requires terminal failures to be observable; [design](../../pr_ontology_diff_design/issue-621/ontology-plan.md).

### g5.organization.viewer-collaboration-question

- human-provided
- Source: [chat/identity supplement](../../uplift/ben-ontology/investigations/chat-identity.md). `CommentAuthority` checked action-specific author/owner/reader authority, and `QuestionVotes` checked voter-owned advisory changes without changing an answer.
- Neither model enforced a viewer-role ceiling, so these historical checks do not establish the viewer participation rule adopted here. Grant enforcement remains under the [viewer ceiling](#g5.organization.viewer-ceiling).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 26 (DC-Q1, OR-cell-16).

### g5.security.org-admin-settings

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/admin-page/request.md).


### g5.settings

- agent
- Defines the `g5.settings` ID level: an organization's configurable defaults and controls. See [organization configuration](#g5.settings.organization-configuration).

### g5.settings.organization-configuration

- human-verified
- Source: [issue #577 owner/configuration decision](../../chat_intent/issue-577/request.md), [implementation design](../../pr_ontology_diff_design/issue-577/ontology-plan.md).
- Agent validation: [issue #577 results](../../pr_ontology_diff_design/issue-577/ontology-plan.md) cover Admin-only organization controls, organization writes, retained inheritance, role refusal and real Chromium API-key creation. Production bundling remains limited by the test VM's memory ceiling.

### g5.data

- agent
- Defines the `g5.data` ID level: how G5 keeps durable data, each store owning its schema, validation and tenant isolation. Children: [configuration](#g5.data.configuration).

### g5.data.configuration

- agent
- Expands: [data](#g5.data).
- Defines the `g5.data.configuration` ID level: typed organization settings with one validated owner per store.

### g5.data.configuration.org-admin-settings

- agent
- Expands: [configuration](#g5.data.configuration).
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/admin-page/request.md).
- `org_admin_settings.rs` owns thirteen registered org keys; migrations 083, 086 (`min_free_slots`, [free-slot floor](../software_architecture/host_control_plane.g5.md#g5.architecture.host_demand_capacity)), 110 (`default_personal_idle_timeout_secs`), 118 (`max_running_vms_per_owner`, `max_workspaces_per_owner`, [owner limits](#g5.organization.owner-limits)), 141 (`automatic_continuation_enabled`) and 146 (`host_disk_allowance_gib`, `vm_disk_allowance_gib`) register their schema without rewriting old rows. Do not introduce a parallel org column or untyped settings bag. The disk allowances follow [G5-Lean#1049](https://github.com/G5Labs/G5-Lean/issues/1049) and its [design](../../pr_ontology_diff_design/issue-1049/ontology-plan.md).
- [G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984) adds `automatic_continuation_enabled` (migration 141, on by default) on the owner's decision in the `chat-auto-resume` question set; [design](../../pr_ontology_diff_design/issue-984/ontology-plan.md). Its effect is owned by [automatic continuation](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start.automatic-continuation).
- [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624) (human-verified) adds the personal-workspace idle default setting; [design](../../pr_ontology_diff_design/issue-624-personal-workspace/ontology-plan.md). Implemented; locations and tests are in the [personal workspace metadata](personal_workspace.g5.md#metadata).

### g5.organization.platform-operator

- human-provided
- Source: [issue #776, original intent (Ben)](https://github.com/G5Labs/G5-Lean/issues/776). The issue renames Ben's "admin" to "operator" because "admin" already names an organization role.
- Implemented: migration 122 (`orgs.kind`, `orgs_single_operator`, `orgs_operator_slug`), `scripts/seed-prod-org.sh`, `platform_operator.rs` in `g5-api-core` (authority, directory, owner resolution) and `g5-api-routes` (`/api/v1/platform/orgs`, the E2E operator fixture), and `frontend/src/components/platform/PlatformHome.tsx`. Evidence: `backend/g5-api/tests/platform_operator.rs`, `PlatformHome.test.tsx`, `frontend/e2e/flows/org-create.spec.ts` (console spec, run against a native stack) and two real seed runs against a migrated Postgres ([design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#validation-results)). Of the member capabilities, only listing and creating organizations exist; monitoring and provisioning are open in the [operations question](#g5.organization.platform-operator.operations-question).

### g5.organization.platform-operator.home

- human-verified
- Source: Ben's intent and comment on [issue #776](https://github.com/G5Labs/G5-Lean/issues/776) ("I propose the Home page for admin/ is the admin home page"); the owner chose `/operator` and kept Base and docs ([chat intent](../../chat_intent/issue-776/request.md#owner-decisions-in-chat-2026-10-24)).

### g5.organization.platform-operator.identity

- agent
- Source: issue #776 additions 3 and 5, the owner's `/operator only` answer and the platform-seeding instruction ([chat intent](../../chat_intent/issue-776/request.md)), which also reserves `admin`. The route segments mirror `frontend/src/lib/shellRoutes.ts` `STATIC_ROUTES`; keep both lists aligned. [Design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#why-a-kind-column-not-the-slug-alone).

### g5.organization.platform-operator.bootstrap

- human-verified
- Source: owner instruction "Update the design to ensure that platform seeding just provisions the operator org" ([chat intent](../../chat_intent/issue-776/request.md#design-review-2026-10-24)). It supersedes the earlier "first listed sign-in" answer. No `operator` organization exists in any deployment, so a slug collision just errors. The ownerless roster, last-administrator recovery by re-seeding and the self-hosted exclusion are agent decisions. The service identity's member role was discovered during implementation: `workspaces_active_owner_required` and the deferred `workspaces_owner_same_org_fk` refuse an unowned Base.
- Dev and preview mirror: owner instruction "Let's match prod. give Alex operator membership" ([chat intent](../../chat_intent/issue-776/request.md#dev-and-preview-seeding-2026-10-26)). `scripts/seed-dev-org.sh` (`DEV_OPERATOR_EMAIL`, default Alex `demo@g5labs.ai`) has the test-token fixture operator persona provision the operator org, then add Alex as administrator through the member API.
- Platform seed: `scripts/seed-prod-org.sh` (`DEFAULT_ADMIN_EMAIL`). Consumers `scripts/reset-qa.py` and `scripts/reset-dev-environment.sh` create their tenant organizations afterwards. [Design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#bootstrap-platform-seeding-provisions-the-operator-org).

### g5.organization.platform-operator.base-seed

- human-provided
- Source: [issue #911](https://github.com/G5Labs/G5-Lean/issues/911) (@bvandiver-g5), discovered on QA after #811: an operator Base with only docs refused every child Start (`IMAGE_INVALID`, no code repository members). The issue chose a consistent seeded structure over relaxing the [image-selection](../software_architecture/hosts.g5.md#g5.host.image-selection) code-member requirement for docs-only workspaces. Keeping an administrator's code repository and the two refusals are agent decisions. Children forked from a docs-only Base keep docs-only membership and are discarded, not repaired (owner, 2026-10-28; [design](../../pr_ontology_diff_design/issue-911/ontology-plan.md#not-covered)).
- Implemented: the Base code-member step and `repo_mirror_init` in `scripts/seed-prod-org.sh`. Evidence: `scripts/test_seed_prod_org.py`, `platform_operator::a_child_of_the_seeded_operator_base_starts_on_the_org_default_image` and `a_child_of_a_docs_only_operator_base_cannot_start`, and real seed runs against a migrated Postgres ([design](../../pr_ontology_diff_design/issue-911/ontology-plan.md#validation-results)).

### g5.organization.platform-operator.authority

- agent
- Source: role tiers from [Ben's intent](#g5.organization.platform-operator); current-authority rule from [g5.security.current-authority](../concepts/security.g5.md#g5.security.current-authority). [Design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#authority).

### g5.organization.platform-operator.tenant-boundary

- agent
- Source: issue #776 additions 1 and 7. Audit uses the existing `g5_audit` user-action events; `GET /api/v1/platform/*` is on the curated read list.

### g5.organization.creation

- human-provided
- Source: owner, 2026-10-24: "currently I think that successfully login for a user without an organization offers to create an organization. With operators this route shouldn't exist." ([chat intent](../../chat_intent/issue-776/request.md#self-serve-creation-and-the-allowlist)). This resolves issue #776's hosted self-serve question. Before this, `/create-org` was open to any signed-in person without an organization, and the `@g5labs.ai` check hid only the Account-flyout entry.
- Removed `POST /api/v1/orgs` (listing stays), `/create-org`, `CreateOrgDialog`, `useCreateOrg` and `canCreateOrganization`. An organization-less session lands on `/no-organization`. `platform_operator::self_serve_org_creation_does_not_exist` checks the route. [Design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#organization-creation-is-operator-only).

### g5.organization.platform-operator.create-org

- human-verified
- Source: the owner's answer "If the user doesn't exist, create it" ([chat intent](../../chat_intent/issue-776/request.md)); account linking by verified email is existing `AuthService::handle_callback` behavior. No notification email tells the initial Admin; membership-added notifications stay outside [invitation email](#g5.organization.invitation-email).

### g5.organization.platform-operator.self-hosted-question

- agent; uncertain
- Source: issue #776 open questions; the issue comment recommends the single organization's owner.

### g5.organization.platform-operator.operations-question

- agent; uncertain
- Source: issue #776 open questions; related [issue #746](https://github.com/G5Labs/G5-Lean/issues/746).
