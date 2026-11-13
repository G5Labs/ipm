# G5-Lean v8 security review

Reviewed candidate: `6191bdbb7c0f327aca499086edc688dc770d9a85` on `docs/backend-design-v8`, 2026-09-17. Scope: [design v8](lean-backend-design-v8.md), its three normative API companions, the [implementation plan](lean-implementation-plan-v8.md), and the [sharing clarification](workspace-sharing-v8.md). Source references and line numbers below refer to that candidate. The design and plan are unchanged by this review.

Recommendation: resolve the five P1 contract/delivery gaps below before implementing or enabling the affected surfaces. Also specify revocation for established streams. The passt network exposure is a separate high-impact validation question whose exploitability has not been established.

This is a design review with targeted inspection of retained code, not a penetration test or proof of a deployed vulnerability. Each finding distinguishes the documented behavior from its security consequence if implemented as written. No live user data, credentials, hosts or cloud metadata were accessed. Proposed regressions below have not been run against the future v8 runtime.

| ID | Priority | Finding | Affected delivery scope |
|---|---|---|---|
| S1 | P1 | Workspace read permissions do not isolate shared Git objects | Git read boundary, P4.3a, P4.5, restore/cache |
| S2 | P1 | Same-origin preview executes with the viewer’s platform origin | P5.1 and retained preview UI/relay |
| S3 | P1 | Proxied module calls lack a distinct authenticated capability boundary | P4.2a/P4.2b, P4.5/P4.6, P4.11/P4.12 |
| S4 | P1 | Host SCM contract permits writes that bypass checked publication | P4.12, P1.5, P6.1/P6.2 |
| S5 | P1 | Transfer’s interim guard misses queued legacy work | P4.5 before P4.7/P4.8 cutover |
| S6 | P2 | Removing read access does not revoke established streams | Workspace authorization, runtime/event subscriptions, P5.1 |

## S1 — Isolate Git contents at the workspace read boundary

**Evidence.** Design §5.1 gives invite-only workspaces independent read permissions (lines 227–229), but §5.3 requires only workspace read access before `git-upload-pack` (line 271). P4.3a explicitly leaves internal Git reads unchanged (plan line 534). In the retained handler, [read authorization](../backend/g5-api-routes/src/routes/git.rs#L449) succeeds when the caller can access any workspace using the repository; [ref advertisement](../backend/g5-api-routes/src/routes/git.rs#L548) and [upload-pack](../backend/g5-api-routes/src/routes/git.rs#L596) then operate on the entire repository without a workspace-specific serving boundary. Design §5.5 also permits clone/reflink materialization from the shared store/cache (line 324).

**Security consequence.** A reader of workspace A can be authorized to a repository that also holds invite-only workspace B’s branch history and flush snapshots. The unchanged Git path can disclose B’s contents even though the file/chat APIs deny B. Giving A’s VM a reflink or alternate object store containing B’s objects creates the same exposure through local Git inspection. This does not require crossing an organization boundary.

**Required correction.** Define the confidential unit explicitly and make Git serving, receive-pack object admission, snapshot reads and guest materialization honor it. To preserve workspace confidentiality, serve object sets isolated at the read-permission boundary and keep shared bare caches inaccessible to guest commands. Merely hiding advertised refs is insufficient: Git’s own [namespace security guidance](https://git-scm.com/docs/gitnamespaces#_security) warns that namespaces are not read-access isolation. An alternative product decision to make repository contents readable across all its workspaces would change the sharing contract and must be explicit; this review does not assume it is accepted.

**Regression.** Create differently authorized workspaces A and B sharing a repository, with a secret file committed and separately flushed only in B. As A’s reader and VM, test ref discovery, ordinary fetch, known-OID fetch, flush refs, crafted push/fetch negotiation and local object/cache inspection. No B-exclusive content is returned; explicitly shared source history remains readable.

## S2 — Require a separate browser origin for previews

**Evidence.** Design §6.8 serves previews at the platform API path and makes subdomain mode optional (line 581). The retained [PreviewBrowserView](../frontend/src/components/preview/PreviewBrowserView.tsx#L640) deliberately omits the iframe sandbox in same-origin mode. Platform [token refresh](../frontend/src/hooks/useAuth.tsx#L168) is a same-origin request using the browser’s refresh cookie. Stripping authorization headers before forwarding to the preview server does not restrict JavaScript executing in the browser.

**Security consequence.** Repository-controlled preview JavaScript in that mode shares the platform origin with the logged-in viewer. It can access the parent application, call the token-refresh endpoint and issue platform requests with the viewer’s authority, including permissions in other workspaces. An owner-controlled preview viewed by a more privileged reader/admin is a concrete trust crossing. Browser origin identity excludes the URL path; see the [same-origin policy](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Same-origin_policy).

**Required correction.** Make an isolated preview origin mandatory for shared deployments, including self-hosted installations, and fail closed when it is unavailable. Keep platform authentication off preview origins; scope preview cookies to their host, restrict platform CORS/CSRF and `postMessage` handling, and preserve isolation for both embedded and new-tab navigation. Check cookie-domain and service-worker scope behavior so a preview cannot affect the platform or another workspace. A UI-only iframe sandbox is not sufficient for the existing open-in-new-tab path.

**Regression.** A hostile preview attempts parent DOM/storage access, platform refresh/API calls, forged messages, cookie injection and service-worker registration. Run it embedded and in a new tab as a viewer with access to other workspaces. None of those attempts gains platform or cross-workspace authority. Verify the deployment without preview-origin configuration refuses the unsafe fallback.

## S3 — Separate module tool authority from dispatcher authority

**Evidence.** Design §6.5 gives launched processes the same VM proxy secret (line 513). Its platform proxy injects the workspace token and accepts plain execution/session identifiers (line 519); the dispatcher uses that same workspace credential for lifecycle, receipts and harvests (line 522). The [host companion](worker-host-api-v8.md#65-how-each-client-is-steered-to-the-proxy), line 243, and P4.12 steps 1/4 repeat this shape. In contrast, design §6.7 promises capability-limited module invocations (line 555). The documents do not specify a request-authenticated invocation identity, route/resource capability enforcement at this proxy, or a separate credential audience for dispatcher-only endpoints.

**Security consequence.** The documented VM identity proves which workspace sent a request, but not which module or execution may perform it. A compromised repository process can submit a sibling execution ID or request private operations through the same credential-injecting proxy. Checking that the supplied execution belongs to the current VM cannot establish the requesting process’s authority. This is an incomplete security contract, not proof that every existing private endpoint currently accepts such calls: retained question routes already have user/task distinctions that must be preserved.

**Required correction.** Specify separate trusted-runtime and repository-process tool principals/audiences. Authenticate the invocation context outside process-supplied IDs and enforce operation, resource, task, attempt and incarnation scope at the proxy and API. Reject dispatcher lifecycle/durability routes from repository processes; keep human approval/answer authority distinct from agent reports. Preserve the trusted pinned `agent.turn` adapter’s authenticated, own-session restore/harvest path (§8.5); it is distinct from the untrusted provider/repository child it supervises. Define how the execution binding and trusted-runtime credentials remain inaccessible to other guest processes, or explicitly narrow the security guarantee if all guest processes share one trust boundary. This requires execution capabilities, not per-user provider-account grants.

**Regression.** From preview and provider/repository child processes, attempt guessed sibling execution/session IDs, forged sibling task/question events or review-actor identities, dispatcher-only receipt/outcome calls, and unauthorized or foreign-session harvests. Assert rejection before mutation, including stale attempts and delayed retries. Legitimate scoped tools, owner-authorized PR review, the dispatcher’s private path and the trusted adapter’s own-session restore/harvest must still work.

## S4 — Resolve the contradictory external Git grant policy

**Evidence.** Design §5.6 reserves provider publication for explicit checked API operations (line 335) and permits private external dependencies only through read-only grants (line 339). The [host companion](worker-host-api-v8.md#64-credential-proxy-for-git-and-gh), line 230, expressly permits direct GitHub/GitLab pushes through credential injection. Its grant shape names `target_host`, without a repository or read-operation restriction (lines 136–139), and P4.12 step 3 describes host-based routing.

**Security consequence.** Following the companion with a write-capable credential lets guest code bypass the API’s initial-publication collision checks, recorded publication ownership and lease policy. Even a read-only credential scoped only by host can expose unrelated repositories covered by that credential. The main design takes precedence, but leaving the concrete host contract inconsistent makes a security-critical implementation decision ambiguous.

**Required correction.** Align the companion/spec/plan with §5.6: external dependency grants permit clone/fetch only for approved repositories; reject external receive-pack and unrelated API paths. Publication credentials stay on the control-plane path that performs the intended checks. Bind injection to canonical scheme, host, port, repository and operation, including redirect handling. Internal Git keeps its workspace-specific policy and the S1 confidentiality requirement.

**Regression.** An approved private dependency fetch succeeds. A direct external push, another repository on the same host, a path-encoding escape and a redirect outside the grant are denied without credential forwarding. Checked publication still enforces create-if-absent and subsequent owned-publication leases.

## S5 — Include queued legacy work in the transfer cutover guard

**Evidence.** P4.5’s temporary guard covers active legacy executions (plan line 627). P4.6 adds new-dispatcher account selection and post-transfer holds while the legacy kernel remains until P4.7/P4.8 (lines 655–657). The retained [claim path](../backend/g5-api-core/src/services/task_service.rs#L8158) selects pending tasks. Assignment uses [frozen provider-account selection](../backend/g5-api-core/src/services/config_service.rs#L609), whose [resolver](../backend/g5-api-core/src/services/provider_account_service.rs#L462) deliberately omits a fresh caller-visibility check. P4.5’s general current-owner route requirement does not explicitly migrate these background claim/retry/account consumers.

**Security consequence.** A stopped workspace can have an old-owner job waiting for capacity without an active legacy execution. If transfer becomes available with only the specified active-work guard, the retained scheduler can launch that queued job later, bypassing the new owner’s explicit Resume and potentially using the old owner’s frozen provider account. This is a delivery-plan gap; v8 transfer is not implemented in the reviewed candidate.

**Required correction.** Before P4.5 exposes transfer, either block it for all dispatchable/nonterminal legacy work, including queued tasks, retry/requeue states and unacknowledged claims, or migrate every legacy admission/claim/assignment/account path to the transfer barrier and current-owner binding in that PR. Perform the check atomically with claim admission. Keep the guard until the relevant legacy producers are actually removed.

**Regression.** Queue an old-owner job with no capacity and request transfer. With the expanded guard, transfer is refused until that work is resolved; with migrated legacy scheduling, transfer may succeed but restoring capacity without new-owner Resume assigns no task and spends no former-owner account. Repeat with retry/requeue, claim-versus-transfer races and delayed assignment delivery.

## S6 — Revoke established read streams when access changes

**Evidence.** Design §9.1 checks access on every request and stream admission (line 819). §6.8 checks preview relay opening/reopening (line 581). Neither defines reauthorization or closure of already-open file/chat/module subscriptions or preview WebSockets when an invite, organization membership or visibility changes. The incarnation fence does not help when the same VM remains current.

**Security consequence.** A reader can open a long-lived stream before removal and continue receiving new private content after losing read access. Rejecting its next connection does not revoke the existing one. This concerns future delivery; already downloaded content cannot be recalled.

**Required correction.** Bind subscriptions and relay connections to their user/workspace authorization state, and invalidate or reauthorize them on access changes. Define the revocation ordering and any bounded delay, including cache invalidation and reconnect behavior. Keep authorized readers connected and the owner’s workspace running.

**Regression.** Open file/chat/module streams and a preview WebSocket, revoke the caller’s access, complete the revocation barrier, then emit new content. The removed reader receives no subsequent content; old tickets/reconnects fail while an authorized reader continues normally.

## Additional high-impact validation — passt access to host infrastructure

The [host companion](worker-host-api-v8.md#6-host-side-proxies), lines 175–177, exposes host loopback through passt and protects the G5 proxy using a VM secret. That secret does not protect other host listeners or cloud metadata. The retained [passt policy attachment](../backend/g5d-isolation/src/isolation/passt.rs#L83) and [launch path](../backend/g5d-isolation/src/isolation/passt.rs#L289) explicitly install no network rules at parity. The host [service account](../infra/terraform/modules/worker-host/main.tf#L40) can read the worker registration secret. Compute Engine documents obtaining a service-account token through its [metadata server](https://docs.cloud.google.com/compute/docs/access/authenticate-workloads#authenticating_applications_directly_with_access_tokens).

**Observed gap:** v8 does not state a mandatory host-enforced guest egress policy covering metadata, infrastructure and other loopback listeners. **Unverified hypothesis:** guest traffic forwarded by passt may reach the host’s metadata service and obtain a token usable for that registration secret. Guest-to-metadata connectivity and any deployment-level filtering were not tested; this review does not claim credential access occurred or that all deployments permit it.

Assign the validation to P4.4b/P4.12 before VM admission is enabled. Specify a fail-closed policy that permits required endpoints and blocks metadata, host/sibling listeners and infrastructure destinations outside the approved set. Verify direct IPs, DNS aliases, redirects/rebinding, IPv6 where enabled, and both proxied and direct guest traffic on the actual SaaS and self-hosted network paths. Do not assume application-level proxy authentication constrains traffic that bypasses that proxy.

## Accepted choices and review limits

The source explicitly accepts reader preview start/cancel, workspace secrets and owner provider context in previews, no enforced tracked-write sandbox, no preview wall-clock limit, and owner transfer instead of multiple writers. Those choices are not findings here. The findings concern authority or data beyond those accepted permissions.

The exact shared branch name, fork-only creation, create-if-absent checks, durable fork receipts and partial cleanup rules provide useful integrity constraints. Transfer’s stop/flush/release ordering and the incarnation fence also provide sound foundations. They do not substitute for object-level confidentiality, browser-origin isolation, authenticated execution capabilities or access revocation.

No architectural changes, implementation changes, PRs or issues were created by this review. Closing a finding requires an explicit design/plan correction and the relevant production-boundary regression on the eventual implementation; document checks alone do not establish runtime security.
