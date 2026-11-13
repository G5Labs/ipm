# Dev deployment gates: inspection and proposed Lean changes
> Implementation update: the seven approved gates are being retired. See [standard-features-retirement.md](standard-features-retirement.md) for scope and validation. Other removal candidates below remain proposals.


Status: **proposal only**. No application code, deployment configuration, tests, or database schemas were changed for this request. The earlier batches 1–4 retirement remains separate. This proposal assumes a fresh Lean installation and does not attempt to upgrade an existing G5 database/fleet.

## Evidence and limits

On 2026-09-13, the public hosted-dev endpoint `https://dev.g5labs.ai/api/v1/instance/info` returned:

```
crdt, search, chat, cloud_workers, codex_ui, g5_ui_v3, top_panel,
provider_accounts, platform_issue_reporting, make_remote, oauth
```

It reported `authProviders: ["github"]`, `selfHosted: false`, `devLoginEnabled: false`, and `defaultWorkspaceLayout: "both"`. `issue_tracking` was absent. The handler adds that feature directly from `state.issue_tracking_enabled`; the internal issue routes enforce the same state. This is live evidence for the internal ticket module being disabled, not merely an absent YAML setting.

The shipped frontend `/assets/index-CRhbNF0D.js` contains `VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS: "true"` and `VITE_G5_ENABLE_INTENT_LOG_VIEWER: "false"`. Its dev-mode helper compiles to `function c_(){try{return!1}catch{return!1}}`, with consumers in settings and the `/ide` route. Takeover banner strings are absent after dead-code elimination.

**Live cluster/worker settings are not yet verified.** `gcloud container clusters list --project=g5-dev-1ccc` failed with “Reauthentication failed.” Reauthentication was requested; no authenticated cloud calls were retried. Therefore tables below explicitly distinguish live observations from checked-in dev intent or code defaults. A missing YAML key alone is not proof that a live pod lacks an envFrom/Secret override. In particular, the exact-v2 persistence gate must not be retired on that assumption.

Evidence inventory: [deployment-gate-inventory.json](deployment-gate-inventory.json). Its candidate references include operational and test controls; they are not all deletion targets.

## 1. Remove these disabled surfaces/modules

| Candidate | Gate / evidence | Proposed removal and retained boundary |
| --- | --- | --- |
| **Internal ticket / issue tracking** (removed; replaced by [external issue integration](external-issue-integration-design.md)) | `G5_ISSUE_TRACKING_ENABLED`, default false; live `issue_tracking` absent. API capability and route admission share the predicate. | Remove IssuesPage, issues list/detail/badges, `useIssues`, issue tab/URL/WS wiring, org issue routes and internal issue worker endpoints, IssueService and issue attachments, issue inference/writeback producers, DTOs and exclusive fixtures. Remove agent tools `file_issue`, `search_issues`, `add_issue_comment`, `update_issue`, issue prompt/context exposure and MCP allowlists. Include the hardcoded false `ISSUE_STATUS_ENFORCEMENT_ENABLED` branch. Keep generic comments, notifications, job lifecycle, approvals and platform bug reporting. |
| **IDE plugin mockup** | `/ide` is under `isDevMode()`; live frontend helper returns false. | Remove IDEPluginView, route and mockup-only fixtures/assets. This is not removal of the real editor or external agent integrations. |
| **Hidden notification/delivery settings UI** | WorkspaceSettingsContent `devMode && renderDevMode()`; same live false helper. | Remove the hidden task-complete/agent-blocked toggles, delivery-channel add/remove/toggle form, exclusive view state/icons/tests. Trace persisted settings consumers before removing fields. Keep the active notification bell, mentions, toasts and operational Slack reporting. |
| **Legacy takeover/intervene UI remnants** | BottomDrawer has `const takeoverUiEnabled = false`; `onTakeOver` is only a declared prop there, with no active invocation. | Remove the two unreachable banners, unused takeover callback/state/URL plumbing and exclusive fixtures. Keep pause/resume endpoints used by active jobs/chat and keep the unrelated question-document interaction and agent approval paths. This is not authorization to delete every backend method containing “takeover.” |
| **Google, GitLab and Microsoft platform sign-in** | Live `authProviders` is GitHub-only; dev `G5_OAUTH_ENABLED_PROVIDERS=github`. | For dev-parity Lean, retire these platform-login adapters, callback/config/secret mappings, login icons/catalog entries and exclusive tests; preserve GitHub login. Keep SCM repository integrations and Claude/Codex provider-account OAuth unless separately proven unused: those are different authentication domains. |
| **Automatic golden-builder scheduler** — pending live indexer confirmation | Dev indexer explicitly sets `G5_GOLDEN_BUILDER_TRIGGERS_ENABLE=false`, with a comment documenting the deliberate pause. | Remove `maintenance/golden_builder_triggers.rs`, its loop registration, trigger-only cap/backoff knobs and tests. **Do not remove Docker golden storage/consumption or the host builder lane**: dev enables those separately, including preview golden-miss behavior. |
| **Cargo/Node golden-family placeholders** — bounded candidate | `G5D_CARGO_GOLDEN_ENABLE` / `G5D_NODE_GOLDEN_ENABLE` default off; neither is provisioned by dev. Host `V1_DISPATCHABLE_FAMILIES` contains only `docker`; Cargo requests are rejected. | Remove unsupported family switches and family-exclusive recipe scaffolding after checking all family callsites. Preserve shared cache-key, sealing, ownership, receipt, capacity and eviction code, plus normal Cargo/Node build caches. This is not removal of Docker warm caches. |
| **Exact-v2 CRDT persistence barrier extension** — **unconfirmed; hold** | `G5_CRDT_PERSISTENCE_BARRIER_EXACT_V2_ENABLED` defaults false and is explicitly enabled in CI, but is absent from the checked-in dev values. | Verify actual dev API env and worker negotiation first. If off, propose removal of this extension and its exclusive negotiation/test branches; retain task CRDT durability and the currently active barrier protocol. If live-on, move it into the always-on list instead. |

Internal-issue implementation owners include `backend/g5-api-routes/src/routes/issues.rs`, issue-specific endpoints in `routes/internal.rs`, `backend/g5-api-core/src/services/issue_service.rs`, `issue_attachment_store.rs`, the former `g5d-runtime-core` issue-tracking module (already removed) and `backend/g5d-tooling/src/tools/{issue_ops,issue_context,search_issues,add_issue_comment,update_issue}.rs`. The tool registry must be changed with both Claude/Codex tool exposure. Database candidates start at migrations 088/090/091/092/093/094; later foreign keys, grants, views and queries require a full dependency check. An empty installation does not make deleting isolated migration files safe. Preserve migration-chain integrity and verify a database bootstrap from zero.

## 2. Keep these features and remove rollout/off branches

“Dev values” below means checked-in deployment intent, still awaiting live pod/host confirmation where the public endpoint cannot prove it.

| Feature / switches | Dev evidence | Proposed Lean behavior |
| --- | --- | --- |
| Fork/carry chat: `G5_FORK_CHAT_ENABLED`, scoped `fork_chat_enabled`, response `forkChatEnabled` | Dev values true. Code default false, scoped override can disable it again. | Enable supported Claude session carry by default; remove both deployment and hidden scoped feature-off handling and obsolete disabled sentinel paths. Keep user's choice whether to carry individual chats, authorization, session ownership and clone validation. The supplied name `fork_chat_carry` is not a literal current env key; this is its active config/clone path. |
| Codex UI: `G5_CODEX_UI_ENABLED` | Live `codex_ui` present | Keep provider selection and supported account flows; remove deployment-hidden alternatives. |
| Provider accounts: `G5_ENABLE_PROVIDER_ACCOUNTS` | Live `provider_accounts` present | Keep account catalog/assignment/launch UX; remove feature-off alternatives. Keep account scope, credentials, permissions and readiness validation. |
| Cloud workspace workflows: `VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS`, test-only shortcut `VITE_G5_CI_WORKFLOW` | Live bundle true; cloud workers live | Make workflow UI standard. Remove build/test enablement shortcuts. Treat `SELF_HOSTED` and cloud-provider availability separately: they describe deployment topology, not rollout completion. |
| Make Remote: `G5_MAKE_REMOTE_ENABLED` | Live `make_remote` present | Always support adopt/make-remote flow; retain URL, credentials, branch and ownership checks. |
| Platform bug reporting: `G5_PLATFORM_ISSUE_REPORTING_ENABLED` | **Live `platform_issue_reporting` present** | Keep bug-report button, screenshot capture, intake, outbox/GitHub delivery and their tests. Remove the rollout switch, but require actual delivery credentials/configuration; missing credentials must not falsely advertise working delivery in a new installation. This is separate from internal tickets. |
| Task CRDT persistence: `G5_TASK_CRDT_PERSISTENCE_ENABLED` | Dev values true | Always persist/recover task drafts; remove ephemeral task-session feature-off branch. Keep durability, ownership and recovery tests. |
| Zero-slot poll protocol: `G5_TASK_POLL_ZERO_SLOT_V1_ENABLED` | Dev values true | Keep supported zero-slot polling; remove deployment activation check. Retain capacity, authenticated-host, assignment and capability validation. |
| Sticky runtime discard fence: `G5_STICKY_RUNTIME_DISCARD_FENCE_V2_ENABLED` | Dev values true | Use current fence protocol. Remove old no-activation rollout path for fresh same-version installations; retain real discard fences and authoritative teardown/reap checks. |
| Discard rollback drain: `G5_STICKY_RUNTIME_DISCARD_FENCE_V2_DRAIN_ENABLED` | Dev false | Remove the deployment rollback/drain-mode switch. False here means normal enabled feature behavior, not a disabled feature. Keep cancellation, graceful shutdown, cleanup and normal runtime draining. |
| Execution lifecycle strict identity: `G5_EXECUTION_LIFECYCLE_STRICT_IDENTITY` | Dev values true | Always require modern identity; retire mixed-fleet weak-identity fallback. Keep identity checks, stale-event/retry/recovery handling. |
| Host rotation / stale detection / provisioning retries: `G5_HOST_ROTATION_ENABLED`, `G5_STALE_HOST_DETECTOR_ENABLED`, `G5_PROVISION_RETRY_ENABLED` | All dev values true | Enable on supported cloud deployments; remove opt-in branches. Keep cloud topology checks, retry budgets, idle thresholds and capacity limits. |
| Docker warm goldens: `G5_WORKER_GOLDEN_STORE_ENABLE`, `G5_WORKER_DOCKER_GOLDEN_ENABLE`, corresponding `G5D_*` switches | Dev Helm and Terraform set both to 1 | Keep store and Docker family on capable worker hosts; remove rollout off branches. Keep cold fallback on an actual cache miss, readiness, provenance, disk budgets, receipts and hygiene guards. Scheduler remains a separate disabled feature above. |
| Sticky runtime reuse: `G5D_STICKY_RUNTIME` | Code default on; live host override not yet checked | Candidate to remove worker-local rollout opt-out. Keep user-configured sticky hold timeout and actual provider readiness. |
| Mention notifications: `G5_ENABLE_MENTION_NOTIFICATIONS` | Code default on; no dev-values override | Candidate always-on notification production, pending live override check. Do not confuse with the hidden delivery-settings UI. |
| Passive auto-pull: `G5_PASSIVE_AUTO_PULL_ENABLED` | Code default true; no dev-values override | Candidate always-on scheduler capability, pending live override check. Keep per-branch/workspace user auto-pull preferences, pause status and explicit sync behavior. |
| Wait tool: `G5_CHAT_WAIT_TOOL_ENABLED` | Tool registry default true | Candidate unconditional registration of the supported wait tool; keep lifecycle checks. Verify deployed guest overrides first. |
| Provider runtime setup: `G5D_CLAUDE_CODE_ENABLED`, `G5D_CODEX_ENABLED` | Host assignment writer unconditionally sends both as 1 | Remove enable-only setup switches in the shipped Lean worker; still fail correctly if required provider executables are absent. This does not make every arbitrary external worker capable of every provider. |

## 3. Misleading switches and compatibility remnants

- **Intent-log viewer:** remove the unused `VITE_G5_ENABLE_INTENT_LOG_VIEWER` variable, capability member, build arguments and tests for the unused switch. Do not remove IntentLogView merely because the flag is false: no production consumer reads `intentLogViewer`; App mounts the route and WorkspacePage opens/renders its tab. The old TreeNode rail callback is discarded with `void onOpenIntentLog`, but direct URL restoration remains. Keep the viewer pending any separate decision about user navigation.
- **Question stamping:** `G5_SYNC_QUESTION_V1_STAMPING` is only exported/asserted by the Azure installer overlay; there is no backend consumer. Retire the no-op variable and misleading installer verification, not question-document support.
- **Legacy repo hints:** `G5_REJECT_LEGACY_REPO_HINTS` defaults off (tolerate-and-observe) and has no dev-values override. It is an old-client compatibility switch, not a separate user feature. For a fresh all-new client/server installation, propose rejecting retired node-targeted repo hints and removing the tolerance/logging branch; retain authoritative node→repo routing. This intentionally chooses the modern contract even though the rejection rollout flag has not been proven on in dev, so it is listed explicitly for review rather than silently treated as dev-on.
- **Codex subscription legacy setting:** `codex_subscription_enabled` defaults on, but it controls subscription versus API-key authentication and is hidden in the current account-driven UI. It is not safe to replace all false values with true: that changes credential routing. Trace account-authoritative versus legacy no-account producers before retiring the fallback; keep both supported authentication modes. This is a compatibility candidate, not confirmed dead product functionality.
- **Codex chat carry placeholder:** CloneDialog displays “Codex — carry coming soon” for uncarriable sessions. Remove the promise/unsupported selectable-row clutter if desired, but keep rejection of unsupported carry requests and keep ordinary Codex chat. No enabled Codex carry implementation was established.
- `G5_CONNECTED_HEAD_ADOPTION_READY_V1` was the stdout handshake sentinel of the `g5d connected-head-adopt` helper, **not an env feature flag**. P1.4 deleted that helper with `git/head_reconciliation.rs` and `connected_head_port.rs`, and retired its stage-runtime smoke gate and manual helper test, so neither the sentinel nor its handshake exists any more.

## 4. Controls not classified as staged feature protection

These were inspected but do not qualify for automatic deletion based on dev boolean values:

- `G5_RATE_LIMIT_ENABLED=false` in dev is a traffic policy. Removing all rate limiting, or silently treating dev's lax policy as a product feature decision, is outside this retirement proposal. Platform-report quotas are separate and retained.
- `G5_E2E_AUTH` / test-token fallbacks, mock flags and Docker test gates are test infrastructure; retain explicit isolation and the ability to run `just docker-ci`. Live dev reports passwordless dev login disabled. Do not enable it by removing its guard.
- `SELF_HOSTED`, `ENABLE_CLOUD_FEATURES`, provisioning topology and registry/license conditions determine available infrastructure or entitlement. Fresh storage does not supply missing cloud infrastructure. Normalizing Lean to one topology can be part of the later backend design.
- `G5D_LLM_PROXY` and provider-account/egress readiness depend on credential transport. Keep host-proxy versus direct-provider selection and credential checks.
- `G5_WORKER_VM_NET` / `G5D_VM_NET` use `passt` on hosted dev, but Linux/macOS isolation backends have different networking requirements. Retiring a deployment rollback toggle must not break local MicroVm/HVF. Scope network-backend removal separately.
- SSH/debug access, CORS/cookies, `G5_REQUIRE_RUNTIME_DB_SPLIT`, local-Git allowlists, Docker-cache bypass, telemetry controls, resource limits, timeouts and build feature flags are operational/security/platform configuration.
- Per-user layout, branch auto-pull, approval enforcement, sticky idle timeout, agent choice, account choice and scoped authorization remain product behavior. Do not confuse these with fleet rollout switches.

## 5. Implementation and validation plan after review

1. Verify non-public live dev settings after GCP reauthentication: resolved API/indexer env (including envFrom booleans without exposing secrets), current deployment images/revisions and worker host/guest overrides. Especially resolve exact-v2 persistence, scheduler, sticky, wait, passive-pull and mention switches. Do not infer these solely from checked-in values.
2. Remove disabled modules as bounded changes, updating every UI producer, API/internal route, worker/MCP tool, schema reference and fixture. Preserve shared service boundaries. Audit empty-database migrations and worker registration from zero.
3. Collapse enabled-feature gates and exclusive off/rollback tests. Preserve active-path tests and add regressions that start with all retired env variables unset: fork carry, durable CRDT restart, account launches, UI navigation, zero-slot capacity, strict identity and discard recovery must still work.
4. Retire no-op switches and update Helm, Compose, Terraform/host launchers, installers, `.env` examples, scripts and documentation together. Recheck advertised capabilities against actual prerequisites.
5. Run the repository script/CI contract suites and `just docker-ci`; preserve live/hardware conditionals documented in the previous retirement report. A fresh install is an additional acceptance test, not a substitute for the retained behavior suites.

No removals or flag flips have been made in this inspection. The internal ticket module is explicitly on the removal list; the live-enabled platform-report module is explicitly retained. Live GCP confirmation remains blocked on reauthentication, so this is a reviewable proposal with named verification gaps, not a claim that every deployed dev flag has been read.
