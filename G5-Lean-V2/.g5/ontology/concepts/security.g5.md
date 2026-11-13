# Security

<a id="g5.security"></a>
[I · g5.security]

Security invariants say what must hold for G5 to stay trustworthy: who may act, what stays isolated between organizations and people, and how credentials and secrets are protected. [System invariants](system_invariants.g5.md#g5.security.tiers) ranks them in two tiers.

<a id="g5.security.encryption"></a>
[I · g5.security.encryption]

Secrets are encrypted at rest and in transit, and customer data is encrypted at rest.

- Recoverable secrets in the database are encrypted with the deployment's secret key; refresh credentials and API keys are stored only as digests.
- Customer data at rest is encrypted so the cloud provider cannot read repositories, documents or chat, which requires customer-managed keys for every store that holds them.
- In transit, browsers, CLIs, worker hosts and VMs reach the platform only over TLS, and the API reaches the database only over encrypted connections.
- Runnable deployments reject public fixture keys ([deployment keys](#g5.security.deployment-keys)).

<a id="g5.security.tenant.rls"></a>
[D · g5.security.tenant.rls]

Force row-level security (RLS) on tenant tables and use scoped transactions. Even privileged system-role operations require explicit typed organization, owner and resource checks. Deployment table-owner remediation uses the bounded cleanup form or an explicitly identified historical execution context; neither grants runtime access, and both restore FORCE RLS atomically without disabling RLS.

<a id="g5.security.redaction"></a>
[I · g5.security.redaction]

Redact configured secret values before public output while preserving exact private native-request state.

## Credential authority

<a id="g5.security.credential-classes"></a>
[D · g5.security.credential-classes]

A person's session ([browser or CLI](#g5.security.browser-session-actions)), a delegated API key and a host or VM identity have distinct authority; a service identity cannot act as a person.

- Person-account operations require a person session: reading or editing a profile; listing or creating organizations; accepting or refusing invitations; SCM connection, disconnection or discovery; a person's GitHub App connection status, connection or disconnection; approving CLI login; minting browser tickets; and creating or revoking organization API keys.
- Even an all-scopes organization API key cannot gain that account-wide authority or any credential: it cannot mint, extend or revoke keys, so nothing it delegates outlives its own scope, organization or revocation.
- A workspace agent's [preview credential](../software_architecture/workspace_vm.g5.md#g5.preview.agent-access) is a separate class: the VM credential mints it for one preview task, only a non-browser request on that preview's origin presents it, and it never acts as a browser ticket, person session or platform API credential.


<a id="g5.security.browser-session-actions"></a>
[D · g5.security.browser-session-actions]

A person session is a browser session when web sign-in issued it, and a CLI session when CLI login issued it; refreshing keeps the kind. A role check alone cannot stop an agent or script from making a decision reserved for a person in the UI.

- An action reserved for the web UI — today only merging with an override ([G5's](../features/workspaces.g5.md#g5.workspace.merge.admin-override), [GitHub's](../software_architecture/git.g5.md#g5.git.provider-override)) — requires a browser session sent by a browser: a same-origin or same-site `Sec-Fetch-Site`, which browsers set on every request and page scripts cannot.
- `Origin` is not consulted: browsers omit it on same-origin reads, and a relay or proxy in front of the platform changes it.
- CLI sessions, API keys, browser tickets, preview, host and VM credentials are refused whatever their role or scopes. A session issued before sessions recorded their kind counts as not a browser session until the person signs in again.
- This attests how the session was issued and that a browser sent the request, not that a person is present: someone who copies their own browser session out of the browser can still present it.

<a id="g5.security.browser-cookie-origin"></a>
[D · g5.security.browser-cookie-origin]

In secure deployments, refresh and GitHub App attempt handlers use cookies confined to the exact platform host, so a same-site preview page cannot transplant another account's credential. Secure cookies use `__Host-` names, `Secure; HttpOnly; SameSite=Lax; Path=/`, and no `Domain`.

- Custom refresh names obey the same rule and stay distinct from login, SCM and GitHub App attempt cookies; a secure configuration that would overwrite one is refused before serving requests.
- Secure readers never fall back to an unprefixed legacy name. Replacement and clearing responses expire the former host-only cookie at its original path without accepting its value.
- Plain-HTTP development may keep unprefixed cookies; [preview isolation](../software_architecture/workspace_vm.g5.md#g5.workspace.preview-origin-security) excludes previews there.

<a id="g5.security.browser-cookie-selection"></a>
[D · g5.security.browser-cookie-selection]

Credential-cookie readers (refresh, preview re-authentication, logout and login/SCM/GitHub App attempts) accept a cookie only when exactly one nonempty occurrence of the selected name exists across all Cookie headers.

- Duplicate occurrences — including empty or identical values — and undecodable credential values supply no authority.
- Unrelated cookie bytes, including non-ASCII data, cannot block selection or hide another occurrence of the name.
- Selection cannot depend on header or path ordering.
- Explicit body credentials keep their own validation; clearing an ambiguous cookie does not authorize revoking one of its candidate tokens.

<a id="g5.security.current-authority"></a>
[D · g5.security.current-authority]

Admit user operations against current [active membership](#g5.security.active-membership) and permissions, not only previously issued token claims. Respect revocation completed before admission, and preserve the authority a transactional mutation relies on until it commits.

- After membership, activity, ownership or lifecycle changes revoke read access, the event WebSocket stops sending and closes affected connections within one heartbeat interval (30 seconds).
- Deactivation additionally requires [membership stream admission](#g5.security.membership-streams) at every protected delivery.
- Already transmitted frames and other admitted finite effects are not recalled.

<a id="g5.security.active-membership"></a>
[D · g5.security.active-membership]

Every user-authorized organization boundary requires an active membership, including service calls, delegated credentials, reused database capabilities, background admission and runtime use. Role, workspace ownership, grants and token possession cannot bypass inactivity.

- Account-wide discovery and delivery exclude inaccessible organization data without revoking the person's other memberships.
- Historical identity and administrative membership records stay readable by authorized active people.
- Trusted cleanup keeps only its [incarnation-bound lifecycle authority](../software_architecture/workspace_vm.g5.md#g5.lifecycle.member-deactivation.recovery).

<a id="g5.security.membership-streams"></a>
[D · g5.security.membership-streams]

Existing browser/event, terminal, preview, log and file streams cannot continue organization access after [deactivation](../features/organizations.g5.md#g5.organization.deactivation).

- Recheck current activity at each protected input or bounded output batch; on denial, discard unsent protected data and close the stream. Browser event delivery rechecks per batch of at most 256 queued events, at least once a second, and at the next batch when this API changed a membership's activity.
- Bind subscriptions to the membership activity revision so reactivation cannot validate a pre-deactivation subscription.
- A missed invalidation event or failed authority lookup cannot permit continued forwarding.
- Already admitted finite batches and previously delivered bytes cannot be recalled.

<a id="g5.security.browser-tickets"></a>
[D · g5.security.browser-tickets]

Personal browser sessions acquire short-lived WebSocket tickets bound to a distinct purpose and exact audience. Access, preview and service credentials cannot substitute for them.

- Terminal tickets also bind the workspace; possession does not replace current owner or resource-access checks.
- A ticket is not single-use: it may open further connections until it expires, because each upgrade, and each subscription on a connection it opened, re-checks [current authority](#g5.security.current-authority) for what it opens, organization channels included.
- [Credential classes](#g5.security.credential-classes) and [current authority](#g5.security.current-authority) govern subscriptions and the bounded revocation of existing event streams.
- Deactivation additionally requires [membership stream admission](#g5.security.membership-streams); ticket expiry alone does not enforce it.

## Workspace execution

<a id="g5.security.guest-authority"></a>
[I · g5.security.guest-authority]

The workspace owner may run arbitrary code and administer their MicroVM, including passwordless sudo to guest root.

- The dedicated [formal review workspace](../features/agent_approvals.g5.md#g5.agent-approval.human-read-only) is workflow-controlled: humans have no guest access.
- Module identities, the dispatcher and ExecutionOwner are no security boundary against an ordinary workspace owner.
- The [MicroVM boundary](../software_architecture/hosts.g5.md#g5.host.isolation) must contain this authority and protect the host and hypervisor; [owner admission](../software_architecture/workspace_vm.g5.md#g5.security.owner), [tenant isolation](system_invariants.g5.md#g5.security.tenant) and [guest egress](#g5.security.guest-egress) remain governing.

<a id="g5.security.agent-workspace-scope"></a>
[D · g5.security.agent-workspace-scope]

A workspace agent operates for its workspace's current owner on any live workspace in the same organization that the same person currently owns: its own workspace and the owner's others, forked from it or not, the owner's [personal workspace](../features/personal_workspace.g5.md#g5.personal.identity) included. Base, formal review workspaces and review copies are excluded, though Base stays a fork source.

- The agent forks its own workspace, another workspace in this scope, or Base, as the owner could in the UI.
- Each new request checks that the caller incarnation is [current](#g5.security.current-authority) and the target's organization, kind and owner, then runs the ordinary operation under the owner's authority, whose own checks (such as [personal refusals](../features/personal_workspace.g5.md#g5.personal.refusals)) still apply.
- A target identifier alone grants nothing, and the VM credential never becomes a [person session](#g5.security.credential-classes).
- This scope governs platform operations other than [reads](#g5.security.agent-read-scope); Git reads follow the repository read rule.
- Forking and operating on another workspace also need the owner's [setting](#g5.security.agent-other-workspaces) to allow it.

<a id="g5.security.agent-other-workspaces"></a>
[D · g5.security.agent-other-workspaces]

A workspace agent changes a workspace other than its own only while that workspace's setting *agent may modify other workspaces* is on. Otherwise forking and every operation on another workspace under [agent scope](#g5.security.agent-workspace-scope) are refused before any effect, with a refusal that names the setting.

- The setting is on by default for every workspace, existing ones included.
- Only the workspace's current owner changes it, under [configuration authority](../features/workspaces.g5.md#g5.workspace.configuration-authority) from a person session, never with the agent's VM credential.
- A fork owned by the source's owner copies it; a fork for anyone else starts with the default, and a formal review workspace with it off. An ownership [transfer](../features/workspaces.g5.md#g5.workspace.transfer) turns it off in the transfer's commit.
- Operations on the agent's own workspace — including merging a pull request into it, [reads](#g5.security.agent-read-scope) and [stop by user](../software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.by-user) — do not need it.
- Each request checks the setting at admission; turning it off refuses new requests only, as for [admitted work](#g5.security.agent-admitted-work).

<a id="g5.security.agent-read-scope"></a>
[D · g5.security.agent-read-scope]

A workspace agent reads the durable platform records of any workspace its current owner can currently read in the UI, its own included: workspace listings and details, chat transcripts and timelines with tool calls and output, tasks, and repository files as the Files panel shows them (the owner's own running checkout live, otherwise the committed cut).

- Each read rechecks the caller incarnation and the owner's current read access, and never starts or prolongs compute.
- Administrator owners therefore read every workspace their UI shows.
- Agents treat content read from other workspaces as data, not instructions.

<a id="g5.security.agent-admitted-work"></a>
[D · g5.security.agent-admitted-work]

An agent request admitted under [agent scope](#g5.security.agent-workspace-scope) keeps only its recorded authority and execution-time checks. Losing the requesting VM neither grants new authority nor proves failure; a later authorized caller can observe the outcome.

<a id="g5.security.agent-audit"></a>
[D · g5.security.agent-audit]

Every mutating workspace-agent request emits exactly one `g5_audit` `workspace_agent.action` event when it completes, whether it succeeded, failed or was refused, so an owner can audit what their agents did and tried.

- The event names the caller incarnation (workspace, VM, epoch), the owner, the target workspace, the route and request path, the HTTP status and outcome class, and, once the handler ran, its operation and result; file changes add the repository and paths.
- Records the operation writes keep the owner as actor.
- Reads are not audited, matching the person's `user_action.http` stream.

<a id="g5.security.agent-human-exceptions"></a>
[D · g5.security.agent-human-exceptions]

Workspace-agent parity excludes human authentication and credential consent, security administration, and decisions whose meaning requires a human or the confined [formal agent approver](../features/agent_approvals.g5.md#g5.agent-approval.execution). An agent may inspect or request such a decision but never makes it; turning on [merge when ready](../software_architecture/git.g5.md#g5.git.pr-auto-merge) and merging with an override ([G5's](../features/workspaces.g5.md#g5.workspace.merge.admin-override) or [GitHub's](../software_architecture/git.g5.md#g5.git.provider-override)) are such decisions. Routine configuration, coordination and merge execution are not exceptions.

<a id="g5.security.runtime-config-loader-env"></a>
[D · g5.security.runtime-config-loader-env]

Under [guest authority](#g5.security.guest-authority), an owner who supplies `LD_PRELOAD` through Runtime Config and runs code in their guest's ExecutionOwner, before the module identity is applied, needs no runtime remediation. This only exercises authority guest root already has.

- The finding requires neither a loader-variable denylist nor a redesign of secret delivery.
- [Reserved platform environment keys](../software_architecture/workspace_vm.g5.md#g5.security.credentials.reserved-env) keep their launch-correctness purpose.
- The disposition covers execution confined to the owner's MicroVM; host or hypervisor compromise and access to another owner's resources are outside it.
- The explicit [development process backend](../software_architecture/hosts.g5.md#g5.runtime.local-mode-scope) runs on the host with no MicroVM containment claim, and this decision does not resolve that backend's open scope.

<a id="g5.security.nested-virtualization"></a>
[D · g5.security.nested-virtualization]

Exposing hardware virtualization to a workspace guest ([the host switch](../software_architecture/hosts.g5.md#g5.architecture.microvm_backend.nested)) makes the host kernel's nested-virtualization paths reachable from tenant guest root, which otherwise reaches only ordinary KVM exits. It is a per-deployment attack-surface decision, off by default; Cloud Hypervisor moved its own default to off for the same reason.

- Enabling it relaxes no obligation of the [MicroVM boundary](../software_architecture/hosts.g5.md#g5.host.isolation): passt confinement, Landlock, the read-only runtime, seccomp and cgroup delegation are unchanged.
- Inside the guest, a world-usable `/dev/kvm` grants nothing beyond [guest authority](#g5.security.guest-authority), which already includes root.
- The guest kernel also carries Landlock, the `inet` nf_tables family and loop/XFS so an [inner host](../software_architecture/hosts.g5.md#g5.architecture.microvm_backend.nested) can apply the same launch to its own guests; these are guest-side mechanisms available to guest root and change no host boundary.

## Deployment boundaries

<a id="g5.security.deployment-keys"></a>
[D · g5.security.deployment-keys]

Runnable deployments reject known public fixture signing, verifying and secret-encryption identities, including E2E deployments. Deterministic keys remain available to isolated tests. An unconfigured verifier rejects all credentials rather than accepting a public default.

<a id="g5.security.secret-lifetime"></a>
[D · g5.security.secret-lifetime]

Retain deployment credentials for as long as the data they protect survives.

- Managed bootstrap serializes initialization and durably stores the credential bundle before exposing it; concurrent consumers and retries reuse that bundle.
- If protected data remains, missing or corrupt credentials require deliberate restoration or migration, not automatic regeneration.
- Operator-provided credentials and deliberate rotation keep the same lifetime obligation.

<a id="g5.security.guest-egress"></a>
[D · g5.security.guest-egress]

Before customer execution, Linux microVM networking blocks cloud metadata, link-local and unintended host-local access; enforcement failure prevents boot. This is not general private-network isolation.

- Preserve required DNS, narrowly configured cache/API access and replies to host-initiated connections.
- Guest loopback stays private to each VM; keep the explicit trusted-process and macOS development exceptions.
- The policy stays installed during execution, assuming trusted external firewall administration; an external ruleset flush is not repaired until another boot.

## Metadata

### g5.security

- agent
- security
- Defines the `g5.security` level for this and other pages.
- Children on this page: [Encryption](#g5.security.encryption), [Redaction](#g5.security.redaction), [Credential classes](#g5.security.credential-classes), [Browser-session actions](#g5.security.browser-session-actions), [Cookie host-confinement](#g5.security.browser-cookie-origin), [Cookie selection](#g5.security.browser-cookie-selection), [Current authority](#g5.security.current-authority), [Active membership](#g5.security.active-membership), [Membership streams](#g5.security.membership-streams), [Browser tickets](#g5.security.browser-tickets), [Guest authority](#g5.security.guest-authority), [Agent scope](#g5.security.agent-workspace-scope), [Agent other-workspaces setting](#g5.security.agent-other-workspaces), [Agent read scope](#g5.security.agent-read-scope), [Agent admitted work](#g5.security.agent-admitted-work), [Agent audit](#g5.security.agent-audit), [Agent human exceptions](#g5.security.agent-human-exceptions), [Loader env disposition](#g5.security.runtime-config-loader-env), [Nested virtualization](#g5.security.nested-virtualization), [Deployment keys](#g5.security.deployment-keys), [Secret lifetime](#g5.security.secret-lifetime), [Guest egress](#g5.security.guest-egress). The `g5.security.tenant` and `g5.security.tier*` levels are defined in [system invariants](system_invariants.g5.md#g5.security.tenant).

### g5.security.encryption

- human-provided
- security
- Source: [Ben, line 287](../../../docs/ben_new_ontology.md#L287); the owner's tier 0 bullets on encryption at rest and in transit, with the 2026-10-29 meeting's reading that at rest means the cloud provider cannot read repositories ([request](../../chat_intent/security-invariants/request.md#the-owners-threat-model-verbatim)).
- Implementation and gap (2026-11-06, main `89eeea5e4`): customer-managed keys for Cloud SQL, GKE secrets, GCS, Filestore and Secret Manager (`infra/terraform/modules/kms/main.tf`); the Filestore module passes the key only for non-BASIC tiers while QA and the production default use `BASIC_HDD`, host instances set no disk encryption key, and the GKE module sets no node disk key while the API keeps its working trees under `/tmp` (`G5_STORAGE_WORKTREE_DIR` in `infra/helm/g5/values.yaml`), so repository content on the file store and on host, VM and node disks rests under provider-managed keys (audit 2026-11-02; [follow-up](../../pr_ontology_diff_design/security-invariants/ontology-plan.md#follow-up-work)).
- Gap and open boundary (review of 2026-11-06): customer-managed keys are necessary, not sufficient, for "the cloud provider cannot read" the data. Cloud KMS keys are used server-side by the storing service, which decrypts data while the key grant stands, so they give revocable, audited customer control rather than provider-blind storage. Provider-blind storage would need client-side encryption of repository, document and chat content or an external key manager with key-access justifications. Which boundary the statement requires, and which control meets it, is the owner's decision ([follow-up](../../pr_ontology_diff_design/security-invariants/ontology-plan.md#follow-up-work)). Cloud SQL requires `ssl_mode = "ENCRYPTED_ONLY"`; the host tunnel uses `wss://` (`backend/g5d-host/src/tunnel.rs`); digests per [session authority](../features/users.g5.md#g5.user.session-authority) and [API key authority](../features/users.g5.md#g5.user.api-key-authority).
- Agent audit: Inspected recoverable secret encryption, key-hash and private credential owners; no cryptographic or deployed key-management proof. [workspace evidence](../../uplift/ben-ontology/investigations/workspace.md).

### g5.security.tenant.rls

- human-provided
- Source: [Ben, line 288](../../../docs/ben_new_ontology.md#L288).
- Implemented, background maintenance: jobs that read tenant tables, including the repository GC that repacks loose objects so clones stay fast (`services/git/repo_gc.rs`), list organizations on the system pool and read each organization's rows through the maintenance actor's scoped tenant transaction (`db/maintenance.rs`). The system role has no direct read of `repos`; a GC sweep that enumerated them on the system pool silently packed nothing ([issue #1074](https://github.com/G5Labs/G5-Lean/issues/1074)). Test: [`test_maintenance_repo_gc_packs_repos_hidden_from_the_system_role`](../../../backend/g5-api/tests/maintenance_integration.rs#L1111).

### g5.security.redaction

- human-provided
- Source: [Ben, line 289](../../../docs/ben_new_ontology.md#L289).
- Agent audit: Inspected configured-value scanner and structured-output/settlement owners; arbitrary strings and all private/native consumers are not formally verified. [workspace evidence](../../uplift/ben-ontology/investigations/workspace.md).

### g5.security.credential-classes

- agent
- Source: [Identity coverage F2](../../uplift/ben-ontology/reviews/coverage-identity.md#f2), merged repair `4c9aa418` and retained service/delegated identity contracts; [PR #549 account-route refinement](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/security.g5.md).
- [CliAuthorization](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) and [BrowserTickets](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/platform/README.md) checked that delegated/service credentials cannot approve human CLI login or mint browser tickets. Hashing, entropy and signed parsing were assumptions; those checks do not cover every account route later enumerated here.
- GitHub App person connection: [issue #579](https://github.com/G5Labs/G5-Lean/issues/579). `routes/github_app.rs` requires `extract_session_user_id` for user-connection status, start and disconnect; the OAuth callback keeps its signed-state and PKCE protocol. `api_external_issues.rs::api_keys_cannot_touch_the_person_s_github_connection` refuses zero-scope, admin and all-scope keys from either organization without grant, attempt or GitHub effects. Attempts started by a key before this change lapse with their 10-minute TTL.
- API-key management: [issue #660](https://github.com/G5Labs/G5-Lean/issues/660), [design](../../pr_ontology_diff_design/issue-660/ontology-plan.md#1-delegated-api-keys-cannot-mint-or-revoke-credentials). `routes/api_keys.rs` requires `extract_session_user_id` for create and revoke; a bounded parent/child key lineage was rejected as an unrequired credential hierarchy. Keys a key created before this change cannot be attributed (no creating-credential fact is stored); they remain ordinary organization keys bound to their user's active membership and revocable from the administrators' list.
- Workspace agent preview credential: owner decision `agent-child-preview-access` (2026-10-26) for [G5-Lean#710](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace); see `g5.preview.agent-access`.

### g5.security.browser-session-actions

- human-verified (that the server must tell an interactive browser session from a CLI or API token: [issue #873](https://github.com/G5Labs/G5-Lean/issues/873)); agent (the issued-kind mechanism, the browser request headers, pre-existing sessions refused, the stated limit).
- Design: [plan](../../pr_ontology_diff_design/issue-873/ontology-plan.md#browser-sessions). Amended 2026-10-28: the first guard also required a trusted `Origin`, which hid the override from every real browser (same-origin GETs carry no `Origin`; the preview relay changes it); found when the owner could not see the override in the workspace preview. Implemented · migration 132 `refresh_tokens.client_kind`; `JwtClaims::client`; `auth_middleware.rs::require_browser_session`. Tests: `api_auth.rs::sessions_record_whether_the_web_sign_in_or_cli_login_issued_them`, the guard's unit test, and the refusals in `workspace_merge_override.rs`.

### g5.security.browser-cookie-origin

- agent
- Source: [issue #917](https://github.com/G5Labs/G5-Lean/issues/917); [design and cutover](../../pr_ontology_diff_design/issue-917/ontology-plan.md). Extends the existing login/SCM host-prefix boundary to refresh and GitHub App attempts, preserving custom refresh names.
- Implemented by `AuthService`'s effective refresh name and the GitHub App cookie helper, with matching route issuance/expiry and no legacy fallback. Service composition rejects collisions against the attempt-cookie owners' actual names ([review finding](https://github.com/G5Labs/G5-Lean/pull/938#pullrequestreview-5692831474)), preserving the Identity/Git dependency boundary. The cutover requires sign-in/restart for legacy secure sessions/attempts. [Verification](../../pr_ontology_diff_design/issue-917/ontology-plan.md#implementation-and-verification) distinguishes the isolated Chromium boundary probe, API/database tests and full-stack browser coverage.

### g5.security.browser-cookie-selection

- agent
- Source: [issue #917 and design](../../pr_ontology_diff_design/issue-917/ontology-plan.md); [owner review of unrelated-cookie lockout](https://github.com/G5Labs/G5-Lean/pull/938#pullrequestreview-5692595766).
- `routes/cookies.rs::credential_cookie` owns selection for all affected routes. Unit tests reject duplicates across headers, empty candidates and undecodable credential values while preserving selection amid unrelated cookie bytes; API tests assert successful session operations and that rejected credentials have no renewal, revocation, grant, binding or attempt-consumption effects. See the [verification record](../../pr_ontology_diff_design/issue-917/ontology-plan.md#implementation-and-verification).

### g5.security.current-authority

- agent
- [Deactivation design](../../pr_ontology_diff_design/member-removal/ontology-plan.md) adds activity to current authority and scopes the existing-connection exception. Prior finite checks below do not establish the new activity or stream contract.
- Source: [Identity coverage F7](../../uplift/ben-ontology/reviews/coverage-identity.md#f7), retained current-membership contract and merged correction `ba8a577d`.
- Event-stream bound: [issue #610](https://github.com/G5Labs/G5-Lean/issues/610), [design](../../pr_ontology_diff_design/issue-610/ontology-plan.md). Terminal and preview transports retain their separate contracts.
- Reader writers: [issue #660](https://github.com/G5Labs/G5-Lean/issues/660), [design](../../pr_ontology_diff_design/issue-660/ontology-plan.md#3-reader-writes-hold-their-audience-grant-until-commit). Review replies, FILES pushes, withdrawals, round publication and question votes hold the reader's current membership and workspace authority (`AccessHold::UntilCommit`) on the writing transaction before their first write; `api_workspace_reviews.rs` and `api_question_votes.rs` exercise competing authority changes against each writer; [issue #1107](../../pr_ontology_diff_design/issue-1107/ontology-plan.md) removes only invitation-specific authority.
- Implementation: [event hub](../../../backend/g5-api-core/src/ws/mod.rs) records resolved subscription scope, renews current authority and enforces writer deadlines; committed membership, activity and transfer changes evict readers who lost access. [API regressions](../../../backend/g5-api/tests/api_ws_revocation.rs) exercise real sockets and PostgreSQL, including missed hooks and stalled reauthorization. Hub tests cover full queues, interrupted sends and concurrent subscription cleanup. The historical model checks below predate this stream-revocation contract.
- [CurrentAuthority](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked current authority at admission and protection through mutation commit under the modeled database locks; expiry after admission may still allow completion. [BrowserTickets](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/platform/README.md) exposed SEC-WS-ORG-ADMISSION at fresh organization subscriptions, so these bounded checks do not establish every endpoint's conformance.

### g5.security.active-membership

- agent
- Derived from the [authorization requirement](../../chat_intent/member-removal/request.md#human-instructions), current-authority locks and distinct human/service credentials.
- Implemented across capability minting, transactional authority, credentials, runtime and database acquisition guards. `authz.current_org_ids` filters inactive memberships even inside an already-open scoped transaction. [Evidence and limits](../../pr_ontology_diff_design/member-removal/ontology-plan.md#implementation-evidence).

### g5.security.membership-streams

- agent
- From existing-session denial in the [request](../../chat_intent/member-removal/request.md#human-instructions); [design](../../pr_ontology_diff_design/member-removal/ontology-plan.md#authorization-through-every-path) defines admission and finite in-flight limits.
- `MembershipActivityGuard` binds delivery to a current activity revision; event delivery reauthorizes its resource organization per bounded batch ([follow-up design](../../pr_ontology_diff_design/live-relay-followups/ontology-plan.md#2-membership-is-rechecked-per-bounded-batch-of-delivered-events)), and terminal, preview, archive and Git output check each bounded batch. [Stream evidence](../../pr_ontology_diff_design/member-removal/ontology-plan.md#implementation-evidence) uses real API transports with a scripted guest.

### g5.security.browser-tickets

- agent
- Deactivation now follows [membership stream admission](#g5.security.membership-streams), beyond the ticket and new-subscription checks described in the archived evidence below; current delivery guards implement this additional boundary.
- Source: [browser transport coverage](../../uplift/ben-ontology/reviews/coverage-ui-transport.md), merged lifecycle correction `6556d241163f5b6a959b349be25627b0a3081424`.
- [BrowserTickets](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/platform/README.md) checked session-only minting, purpose, audience, expiry and current terminal ownership across separate mint, revoke and upgrade steps. Cryptographic authenticity and effective expiry validation were assumptions; single-use tickets and cancellation of existing streams were not established.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 1 (SC-Q1).

### g5.security.guest-authority

- human-provided
- Source: user instruction in the G5-Lean#576 workspace, 2026-10-16: owners may do anything inside their MicroVM, including passwordless sudo, provided they do not break the hypervisor. Consistent with [accepted guest-user scope, row 14](../../../docs/v9-mvp-scope-amendment.md) and the blanket sudo rule in [the guest image](../../../infra/docker/Dockerfile.g5d-g5dev).

- Formal-review exception: [g5.agent-approval.human-read-only](../features/agent_approvals.g5.md#g5.agent-approval.human-read-only), [owner amendments](../../chat_intent/issue-640/review-decisions.md).

### g5.security.agent-workspace-scope

- human-provided
- Source: the owner's 2026-10-26 instruction "target workspace owner is the same as current workspace owner", so that a personal workspace, which has no children, can manage the platform for its user ([record](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)). It replaces the earlier [interpretation](../../chat_intent/issue-710-workspace-agent-parity/request.md#interpretation) of direct same-owner children; the fork sources are the owner's decision 2 (Base plus owned workspaces). Excluding Base as a target and review copies, and applying the rule to every caller rather than only personal ones, are agent choices in the [design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-8-owner-scope). Platform-only scope follows the [issue 423 decision](../../chat_intent/issue-423-git-read-boundary/request.md).
- Implemented: [`db::workspace_agent_scope`](../../../backend/g5-api-core/src/db/workspace_agent_scope.rs) (`agent_owner_db`, `agent_target`, `agent_relation`; `AgentTargetNeed::SelfOrOther` and `OtherOnly`) behind the [workspace-agent routes](../../../backend/g5-api-routes/src/routes/workspace_agent.rs); the fork route's `sourceWorkspaceId` (`g5d workspace fork --from`). Records keep the owner as actor; requests are audited under [agent audit](#g5.security.agent-audit).
- Gap: pull-request publish, merge, close and refresh still act only on pull requests into or out of the agent's own workspace (publishing a child's pull request into it), not between the owner's other workspaces.
- Evidence: `operations_reach_this_workspace_and_its_owners_other_workspaces`, `a_personal_agent_forks_base_and_manages_its_owners_workspaces` ([tests](../../../backend/g5-api/tests/workspace_agent_parity.rs)).

### g5.security.agent-other-workspaces

- human-provided
- Source: [owner's request](../../chat_intent/agent-other-workspaces/request.md#human-instructions), 2026-10-23: owner-only, inherited by forks, cleared by transfer, blocks forking, never blocks merging into the agent's own workspace. On by default for every workspace is the owner's later 2026-10-26 instruction in the same record, replacing the earlier off default and its personal-workspace exception. Which operations count as modifying another workspace, keeping stop by user, cross-owner forks taking the default and formal review workspaces starting off are [agent interpretation](../../chat_intent/agent-other-workspaces/request.md#interpretation); [design](../../pr_ontology_diff_design/agent-other-workspaces/ontology-plan.md).
- Implemented: `workspace_settings.agent_may_modify_other_workspaces` (migration 119, column default on), edited through the owner-only settings route; `agent_target` in [`db::workspace_agent_scope`](../../../backend/g5-api-core/src/db/workspace_agent_scope.rs) refuses every non-self target and the fork route refuses before forking, whatever its source (`AGENT_OTHER_WORKSPACES_DISABLED`); the fork copy in `workspace_fork_service` and `WorkspaceVmService::transfer`.
- Evidence: `other_workspaces_need_the_owners_opt_in` ([tests](../../../backend/g5-api/tests/workspace_agent_parity.rs)), `the_opt_in_follows_same_owner_forks_and_transfer_clears_it` ([tests](../../../backend/g5-api/tests/workspace_agent_parity.rs)) and the personal workspace's default in `api_personal_workspaces.rs`.

### g5.security.agent-read-scope

- human-provided
- Source: [owner's read-scope and content choices](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace), 2026-10-22. Treating read content as data restates the issue's mount-push guidance for this wider read.
- Files were added by the owner's [2026-10-23 request](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace) for the file routes.
- Implemented: `agent_read_target` applies `ensure_user_workspace_access` to the owner for the list, probe, chats, messages, timeline and file routes. PR #645's deferral of delegated reads (`g5.security.credentials.delegated-read`) must be reconciled with this block when it merges.
- Evidence: `reads_reach_every_workspace_the_owner_can_read_and_start_nothing` ([tests](../../../backend/g5-api/tests/workspace_agent_parity.rs)).

### g5.security.agent-admitted-work

- agent
- Source: [interpretation](../../chat_intent/issue-710-workspace-agent-parity/request.md#interpretation); restates for agent callers what [`g5.lifecycle.stop.claim`](../software_architecture/workspace_vm.g5.md#g5.lifecycle.stop.claim) and fork recovery already guarantee for their own operations.

### g5.security.agent-audit

- human-verified
- Source: the owner's [2026-10-23 request](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace) to audit refused and failed attempts as well as successes, and file changes. The event fields and leaving reads unaudited (as for people) are agent choices.
- Implemented: the `audit_agent_request` route layer on the [workspace-agent router](../../../backend/g5-api-routes/src/routes/workspace_agent.rs); handlers add operation, target, result and file paths through `audit` and `note_file`. A request refused by the authentication middleware (bad or expired token) never reaches the router and is logged only as a warning. No stored agent-origin marker yet: deferred to owner scope ([design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-4-audit-and-files)).
- Evidence: `every_mutating_agent_request_is_audited_whatever_its_outcome`, the file and pull-request tests in [workspace_agent_parity.rs](../../../backend/g5-api/tests/workspace_agent_parity.rs).

### g5.security.agent-human-exceptions

- human-verified
- Source: [human exception examples](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-instructions-recorded-by-earlier-sessions): logout, security-related actions and approval on behalf of a human. Naming the formal agent approver follows `g5.agent-approval.execution`. The owner's decision 4 ([record](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)) confirms that requesting a formal agent review is not making the decision.
- Implemented: the agent's approval routes read state and [review evidence](../features/agent_approvals.g5.md#g5.agent-approval.human-read-only.agent-inspection) on any workspace the owner can read, and the [activity feed](../features/reviews.g5.md#g5.governance.approval.activity-feed) reports approval events across that scope; the agent requests human approvals and formal agent reviews and cancels the owner's requests on its own and owned workspaces; no agent route records a decision (`approvals_read_follows_the_owner_read_scope`).
- Turning on merge when ready as such a decision: agent design from the [merge-when-ready request](../../chat_intent/merge-when-ready/request.md) ("a VM credential may never turn it on", accepted by the human).
- Merge with override as such a decision: human-verified from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873).

### g5.security.runtime-config-loader-env

- human-verified: applies the user's 2026-10-16 instruction to make no runtime fix when the reported issue stays inside the MicroVM. Source audit is agent-derived.
- Scope: read [#576](https://github.com/G5Labs/G5-Lean/issues/576) with `g5d issue show` and traced current source at `f4b89617eb38e72e121fdad3da5ff64a50e5f0e8`. Static inspection only; no canary library, live MicroVM reproduction, deployed-state check or hypervisor-escape test was run.
- Admission and delivery: [config routes](../../../backend/g5-api-routes/src/routes/config.rs) use current-owner workspace write admission in [scoped database access](../../../backend/g5-api-core/src/db/scoped.rs); organization config writes require organization administration. [Config merging](../../../backend/g5-api-core/src/services/agents/config_service.rs) and [dispatch validation](../../../backend/g5-api-core/src/services/agents/agent_dispatch.rs) accept `LD_PRELOAD`. `build_agent_dispatch` sends secrets as module-invocation data to the owner-bound VM, rather than setting the API or host process environment.
- Guest execution: [module admission](../../../backend/g5d-dispatch/src/modules/mod.rs) accepts that key; [the runner](../../../backend/g5d-dispatch/src/modules/runner.rs) applies secrets to the wrapper after `env_clear`. [ExecutionOwner](../../../backend/g5d-foundation/src/process_termination/execution_owner.rs) re-executes the dispatcher binary at the caller identity and applies the module UID/GID only to its supervisor. Loader execution remains conditional on a visible loadable library and an executable that honors `LD_PRELOAD`.
- Containment scope: [VM specification](../../../backend/g5-api-core/src/services/workspace/workspace_vm_service.rs) selects `/opt/g5/bin/g5d dispatch` with module UID/GID 1000; [host boot](../../../backend/g5d-host/src/workspace_vm.rs) launches that dispatcher inside the MicroVM. [Host mode selection](../../../backend/g5d/src/cli/host.rs) defaults to MicroVM with no fallback; process mode requires explicit selection and acknowledgement. These source facts locate the reported sink in the guest for MicroVM execution; they do not prove general host or hypervisor security.

### g5.security.nested-virtualization

- human-verified: the owner confirmed the deployment-level, default-off switch (decision 1 of the [nested-virtualization design](../../pr_ontology_diff_design/nested-virtualization/ontology-plan.md)) in the Project Docs question set `nested-virtualization` on 2026-10-27; the exposure analysis is agent-derived.
- Source: the owner's handover `request_investigation.md` (Project Docs, workspace `003f1792-2438-4c99-b200-53bbd2841ee0`) asks for the attack-surface trade-off to be stated; Cloud Hypervisor's default change is upstream PR #9045 (v53).
- No hypervisor-escape or nested-KVM security test is planned; the decision records exposure, not a proof.
- Guest-kernel additions for inner hosts (2026-10-28, agent): [nested-virt-followup design](../../pr_ontology_diff_design/nested-virt-followup/ontology-plan.md) §3.1; the strict KVM probe creates a Landlock ruleset and installs the egress policy inside the guest as hard checks.

### g5.security.deployment-keys

- agent
- Source: [SEC-005 instruction and interpretation](../../chat_intent/sec-005/stack-secrets.md), [accepted design](../../pr_ontology_diff_design/sec-005/ontology-plan.md).
- [DeploymentSecrets](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/platform/README.md) rejected public fixture credentials before admitting a consumer. Key-identity parsing and startup call sites were inspected separately; the bounded check did not prove cryptographic security.

### g5.security.secret-lifetime

- agent
- Source: [SEC-005 lifetime design](../../pr_ontology_diff_design/sec-005/ontology-plan.md) and managed bootstrap in `scripts/stack_secrets.py`.
- [DeploymentSecrets](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/platform/README.md) checked two competing initializers, durable publication before use, refusal to regenerate over retained data and successful retry after a failed flush. Atomic rename/fsync and existing-data detection were assumptions; operator migration and remote stores were outside the bounded check.

### g5.security.guest-egress

- agent
- Source: [SEC-002 instruction and interpretation](../../chat_intent/sec-002/guest-egress.md), merged `67ad4b5d20ac1a291382733854292fa42a494fdf`; [active scope amendment](../../../docs/v9-mvp-scope-amendment.md). Organization-controlled egress policy remains deferred.
- [GuestEgress](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked installation before boot, refusal on installation failure, representative denied destinations and required exceptions. Kernel packet behavior, TSI, external firewall changes and development isolation were outside the model; no runtime/network test was run.

- Current Cloud Hypervisor qualification: the dedicated passt UID policy precedes guest traffic; passt runs non-root with zero capabilities, enforced seccomp/AppArmor and an empty root. Real QA packet tests cover metadata/other-host-port/cross-VM denial, DNS, permitted doors, ingress and private guest loopback. The TSI socket path is removed under the [confirmed replacement](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#final-design-for-confirmation--2026-10-14); historical finite-model results do not establish this runtime's packet behavior.
