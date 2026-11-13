# Owner-only workspace configuration and organization administration

Implements the user's [scope decision](../../chat_intent/issue-577/request.md)
for [issue #577](https://github.com/G5Labs/G5-Lean/issues/577).
Authorized branch: `issue-577-security-workspace-writers-redirect-credential`.
Starting revision: `09d00685`, whose tree matches main at `c3fe52b7`.

The workspace's [configuration authority](../../ontology/features/workspaces.g5.md#g5.workspace.configuration-authority)
requires current ownership, including for organization administrators who do not
own that workspace. The [organization administration surface](../../ontology/features/organizations.g5.md#g5.settings.organization-configuration)
owns organization configuration controls. Existing configuration inheritance,
account visibility, personal settings and organization-admin API checks remain.

Reuse the existing transaction-held workspace owner check for runtime configuration
and provider-account create/update/delete. Check the current owner and pending
transfer while holding the workspace authority row through commit, so an old owner
cannot mutate after transfer. Provider-account scope and workspace identity are
immutable; account locking still serializes mutation and assigned-account deletion.
Read pending-transfer state with a fresh statement snapshot after acquiring the
workspace lock: a transfer can commit a pending row while a writer waits, without
changing the owner yet. The concurrency regression exercises this ordering.
No new permission, credential registry or migration is needed. A permission named
`workspaces:write` and workspace read access do not establish ownership.

Workspace settings show personal and workspace controls, with workspace controls
editable only by the current owner. Organization account management, raw settings
and secret editing, and organization API keys live in the existing admin page.
Reuse the current editors with an explicit workspace/admin context: Admin edits
only organization data; workspace settings cannot select the organization as a
write target. Effective inherited values and shared account choices remain usable.
Organization data is not deleted or migrated when moving its management controls.

Validate through real API requests with distinct owner, member, non-owner admin,
former owner and new owner. Rejected creation, endpoint/secret updates and deletion
must leave persisted state intact; authorized owner operations and org/personal
operations still work. Exercise transfer-pending refusal and the transaction lock
against a competing transfer. Cover ordinary workspace runtime-config writes too.
Frontend tests must establish the absence of organization mutation controls from
workspace settings, working organization writes in Admin, role gating, owner-only
workspace controls, and retained inheritance/account selection.

The owner's existing authority inside their MicroVM governs credentials delivered
there. This repair closes the non-owner mutation path; it adds no credential
re-entry policy. The checks below cover the owner/configuration change; the
accepted proxy removal is described separately below.

Validation on 2026-10-16:

- 23 live-Postgres provider-account integration tests passed, including the new
  [non-owner mutation and transfer-race regressions](../../../backend/g5-api/tests/api_provider_accounts.rs).
  Rejected writes leave the destination, name and encrypted credential intact.
  Existing account visibility, personal/org authority, assignment, dispatch and
  credential-rotation tests pass. The fixtures use synthetic credentials.
- 21 additional integration tests passed across `api_config_*`,
  `api_org_admin_settings` and `api_workspace_member_authz`, covering scoped
  configuration concurrency/hints and retained organization/workspace authority.
- 119 UI tests passed across `RuntimeEnvConfig`, `ProviderAccountAssignment`,
  `ProviderAccountsSection`, `WorkspaceSettingsContent`, `OrgAdminPageContent`
  and `OrgApiKeyManager`. These cover organization-only Admin writes, workspace
  owner controls, inheritance and organization-account selection.
- The [Chromium golden path](../../../frontend/e2e/flows/golden-path.spec.ts)
  passed all seven checks against the local API and PostgreSQL, including real
  organization/workspace creation and API-key creation in Admin. The updated
  browser test requires the Admin controls instead of skipping absent controls.
- TypeScript checks, Rust formatting, changed-source Prettier checks and
  `git diff --check` passed. ESLint reported no errors and 12 existing warnings
  in unchanged code within the affected components.

Remaining validation gap: the production Vite bundle did not finish in this VM.
Its transform phase fails with Node/native memory-allocation errors under the
process's hard 8 GiB data limit, including retries with adjusted heap and thread
settings; raising that limit was refused. The production TypeScript check passed,
but a successful production bundle still needs verification in a build environment
with sufficient memory. No provider calls or LLM proxy removal were needed for
these configuration-authority checks.

## LLM proxy removal assessment

The [follow-up request](../../chat_intent/issue-577/request.md) evaluates the
proxy's value independently of the completed permission repair. Recommendation:
remove the relay and configure each native provider client to contact its selected
endpoint directly. The user accepted this design on 2026-10-17 and authorized
implementation, validation, PR publication and shipping checks.

[Native provider transport](../../ontology/features/chat.g5.md#g5.chat.provider-transport)
owns this decision. New launches use direct egress and a new agent module/catalog
version; existing VMs need the updated runtime before launching this module.
Retain the historical proxy egress value only to read old stored launch records,
never to select a transport for a new launch. Pass the frozen auth mode explicitly
to prevent native credential precedence from changing the resolved account.
The API previously omitted the projected endpoint from dispatch, passing only
secrets. Carry the selected endpoint as a non-secret typed provider input alongside
auth mode; use that value instead of a stale environment endpoint. This is necessary
for the accepted custom-endpoint behavior, including title generation.

The composed native check also exposed Codex metadata warnings on custom-provider
turns. Its [exec contract](https://github.com/openai/codex/blob/main/codex-rs/exec/src/exec_events.rs)
defines error items as non-fatal; stream errors and failed turns remain fatal.
Preserve that distinction in the adapter so a successfully completed direct
provider turn is not reported as failed because its model used fallback metadata.

Before removal, the launch path started a Claude Unix-socket relay and a Codex
loopback relay inside the owner's VM. Keeping
real keys out of the provider CLI's environment reduces accidental exposure, but
does not establish a boundary against the owner under
[guest authority](../../ontology/concepts/security.g5.md#g5.security.guest-authority).
Direct launch deliberately gives the selected credential to the provider process;
retain scoped selection, cleared inherited environment and secret redaction.

The [pre-removal relay](https://github.com/G5Labs/G5-Lean/blob/9c966d21/backend/g5d-agent/src/llm_proxy.rs) also adds fallback cache
markers for Anthropic-model OpenRouter Messages requests and logs first/no-request
diagnostics. Both production launch branches pass no accounting context, so its
OpenRouter generation-cost lookup, Claude subscription-quota reporting and
context-derived session header are inactive in this path. These hooks do not
justify keeping a live relay. Existing
[usage collection](../../ontology/features/chat.g5.md#g5.chat.usage) comes from
native provider output and must remain. The first-request diagnostic can be
retired; existing turn progress and timeout handling remain.

Local probes on 2026-10-16 used the installed pins, isolated temporary homes,
synthetic credentials and a loopback HTTP endpoint returning 401 after capturing
the request. No real provider credential was used. Claude Code 2.1.285 sent API-key,
gateway bearer and OAuth requests directly to the configured Messages path with
the expected auth header; OAuth also supplied its beta header. All three requests
already contained native cache markers, including the Anthropic-model gateway
request. The proxy's fallback would not alter those bodies. Remove that fallback
and its G5-only cache switches, relying on native caching. This does not establish
equal costs or cache-hit rates for every model and client setting.

Codex 0.159.3 did not use `OPENAI_BASE_URL` in the clean-home probe; it attempted
the built-in OpenAI endpoint, which the local blocking proxy refused. Overriding
the built-in `openai` provider was also rejected. An explicitly selected custom
provider, with the exact base URL, Responses wire API, environment credential key
and WebSockets disabled, successfully sent the expected bearer credential to the
mock `/api/v1/responses` endpoint. Therefore removal requires explicit per-launch
Codex provider configuration, rather than just deleting `configure_proxy` or
relying on the old environment variable. Keep credentials out of argv and config
files. Native ChatGPT subscription login retains its existing direct route.

The implementation removes relay listeners, forwarding, placeholders, runtime
proxy switches and unwired relay helpers. Launch resolution, egress
validation, dispatch capability names and presentation now use direct transport;
stored historical launch records remain readable. The native transport adapter
preserves endpoint prefixes, selected auth mode and credential precedence,
including API-key use after a subscription login in the same native home. The
additional Claude title call shares that routing. Agent module `1.0.5` and catalog
`2026.10.17-agent-direct-provider` require the updated runtime; replace existing
VMs as part of rollout before launching the new module.

Validation uses the actual launch path against local mock upstreams for each
supported auth/route combination, native cache markers, streaming, usage and
title generation, plus dispatcher regressions for cancellation and resume.
It also covers ownership, account selection, stale native login/configuration and
credential redaction. No replacement relay or new credential broker is needed
for the accepted owner-trusted operating model.

Validation on 2026-10-17:

- The [native transport system check](../../../scripts/test-native-provider-transport.py)
  passed all six cases with the pinned Claude Code 2.1.285 and Codex 0.159.3:
  Claude API key, bearer, OAuth and OpenRouter; Codex API key and OpenRouter.
  Each real CLI received synthetic streamed provider responses through `g5d
  agent-turn`, completed successfully and reported usage. Requests retained the
  configured path prefix and selected credential despite competing environment
  values. Claude retained native cache markers and used the same endpoint for
  its title call. Codex ignored a conflicting provider configuration and a native
  login confirmed as ChatGPT; it preserved that login and wrote no key to config.
- 44 live-Postgres API regression tests passed, including owner/transfer checks
  and the selected account's custom endpoint reaching dispatch. Both new typed
  provider-input unit tests passed.
- All 391 contract, 139 agent and 143 dispatcher unit tests passed, including
  historical egress deserialization, new-launch rejection of retired egress,
  auth precedence, non-fatal Codex diagnostics and normalized-key redaction.
- The dispatcher module integration suite passed 85 tests with one existing
  ignored test, covering cancellation, native-session restoration, steering,
  process cleanup, bounded title output and cached-login precedence. All 45
  architecture checks passed. Native setup fixtures now carry the same explicit
  auth mode and synthetic credential as production input instead of expecting
  relay placeholders. Their Clippy check also passed.
- All 40 API source-boundary checks passed after removing the retired relay
  from their HTTP-owner allowlist.
- 143 focused UI tests and TypeScript checks passed. Affected-source ESLint has
  no errors and 13 existing warnings. Rust formatting, affected-package Clippy,
  changed-source Prettier and shell environment/canary checks passed.

The native checks use local mock responses, not live provider authentication.
They do not establish external redirect behavior, provider billing/cache-hit
rates or successful deployment. The production-bundle memory limitation recorded
above remains a local validation gap pending CI.

PR publication is blocked as of 2026-10-17 00:41 UTC: authenticated `g5d pr
create` returned HTTP 500 / `INTERNAL_ERROR` four times, including after a
successful `g5d workspace refresh` and with the description omitted. `g5d pr
view` and `g5d pr list --state all` confirm no saved PR. Both workspace checkouts
are clean and code is pushed to the authorized branch. No GitHub CI or review
result is claimed. Once publication is restored, create the PR with the owner
permission fix and direct-provider removal as its scope, then monitor checks,
review threads and merge conflicts for the published head. This platform failure
does not change the accepted implementation scope.

On 2026-10-17 the owner identified the workflow-file change as the publication
blocker and requested deferring it. Restore the single `G5D_LLM_PROXY=1` line in
`.github/workflows/self-hosted-azure-install.yml`, leaving that workflow identical
to main. The new runtime has no consumer for this variable, so it cannot enable
the removed relay. Removing this inert workflow setting is deferred; direct
provider transport and the owner-only permission fix remain the accepted scope.

PR #613 review confirmed a stale host catalog digest and a UI save race after
ownership changes. Align the host's pinned digest with the shipped catalog and
refuse saves with staged edits at a scope that is no longer editable; keep those
edits unsaved. The review also identified mutable endpoint lookup during retry.
Under the existing frozen-attempt contract, capture the projected endpoint in
the immutable attempt policy at reservation and reuse it on redispatch.
Credentials are still reauthorized and rehydrated from the selected account,
preserving credential rotation. Rejecting endpoint changes in the retry builder
would strand an unconfirmed attempt and stop that organization's dispatch pass;
retaining its original endpoint preserves attempt identity without adding such
a failure path. A later attempt may use the owner's updated endpoint. This adds
no credential re-entry or credential-origin binding policy. Pre-upgrade snapshots
without the new field retain their existing endpoint resolution behavior.

The review's additional runtime HTTPS restriction is deferred. Named provider
accounts already require HTTPS on create/update and projection. Raw runtime
endpoints previously accepted HTTP, and native invocation runs inside the
owner-controlled VM with its credential. Adding a new transport policy is not
needed for the accepted non-owner authorization repair and would change existing
owner-controlled endpoint behavior.

Review-fix validation: all 24 live-PostgreSQL provider-account/redispatch tests,
9 host-control tests, both dispatch-input unit tests, 38 runtime configuration
UI tests, TypeScript, affected-package Clippy and formatting passed.
The retry regression changes both endpoint and credential after reservation and
proves the same attempt retains its provider input and policy while using the
rotated credential. Both settings and secret edits remain unsaved after loss of
workspace ownership and can be saved after authority is restored. CI on the
initial published head also passed the production frontend build and all 4,905
frontend tests, closing the local production-build memory limitation.

The first full Rust CI run on the reviewed implementation exposed four stale
test expectations. Two lifecycle cases still expected module `1.0.4`, and a
VM-loss continuation case still expected authentication mode to be omitted.
Update these to the accepted `1.0.5` direct-provider contract with an explicit
admitted authentication mode. The SCM boundary guard also assumed at least nine
provider source files; proxy removal leaves eight. Keep the recursive SCM scan
and require discovery of both native runners and the new provider transport,
making its non-vacuity assertion follow the boundary rather than a file count.
These corrections change tests only and preserve their recovery, dependency,
and SCM-isolation assertions.

All 77 lifecycle integration tests and all 9 consumer-boundary tests pass with
these corrections. The boundary test target also passes Clippy, and formatting
passes. CI on `ca899d08` passed all 4,907 frontend tests, the production frontend
build, Clippy, SQL checks, release artifacts, the self-contained runtime check,
and the VM smoke test; its four stale Rust test assertions are corrected here.
