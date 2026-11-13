# V9 review-response verification audit

**Historical snapshot, superseded for current disposition.** This audit evaluated the other reviewing agent's verification before the latest user scope decision. Its findings remain as evidence of that earlier candidate, not a claim about the revised design. See [final scope and review reconciliation](v9-final-scope-and-review-reconciliation.md) for current addressed/deferred outcomes. The dedicated returned-results workflow is now explicitly excluded by the user.

Audited 2026-09-18 in the `G5-Lean-v9` worktree against the current uncommitted v9 candidate, including the data-model revision. This evaluates the follow-up comments supplied in the conversation against the [original review response](lean-backend-design-v9-review-response.md), current contracts, owning implementation prompts and retained production/CI paths.

**Conclusion: the verification is substantially correct. The previous completion statement overstated contract and implementation-plan closure.** Having a disposition for all 125 findings, valid links and a consistent dependency graph did not establish that each remedy had a concrete mechanism, propagated into its owning prompt, or had a discriminating acceptance test. In particular, N3 remains an unresolved MVP design blocker. Several other safety requirements have policy text but incomplete delivery contracts.

This is an audit, not another claim of remediation. The design, plans, companion contracts and both prior review responses were left unchanged in this pass. Only this audit was added. No runtime implementation, deployment or PR occurred. The recommendations below are proposed closure work, not landed fixes.

## Findings ranked by consequence

### 1. [P1] N3: the authenticated legitimate agent-write path is still undefined

**Confirmed.** [Design §6.5](lean-backend-design-v9.md#65-host-side-proxies-and-credentials) line562 requires a private provider tool/control bridge and disables shell writes without it. It never names a transport, lifecycle or demonstrated process/FD boundary for either pinned provider. Meanwhile §5.9/§11.1, decision choice8 and P4.14 promise `g5d pr result submit`. The principal restrictions are necessary, but do not make that command usable. Claude's native question protocol is not by itself a generic platform-write transport, and Codex exec has no described privileged alternative.

The retained launch path supports this objection: [process.rs](../backend/g5d-worker/src/runner/process.rs) launches the CLI with one UID/GID and process group; shipped MCP configurations launch ordinary subprocesses. That does not prove isolation from repository descendants. The retained CLI does not yet implement a `Pr` command; `g5d pr` is a proposed surface. The installed provider versions also differ from the repository pins, so installed-command capability checks would not close the pin-specific gate.

Bootstrap is a related authority gap: §6.7 permits child-task submission through execution identity, while §7.2 says only the owner creates agent/command work and the matrix omits the declared command module. The existing [bootstrap implementation](../backend/g5d/src/worker/bootstrap.rs) constructs and submits a task graph. The replacement should explicitly authorize that trusted module supervisor for the owner-approved job/workspace and declared graph, without passing the authority into build scripts.

**Required closure:** choose and specify the real transport and prove both legitimate writes and descendant/sibling denial at the pinned CLIs, or explicitly change the MVP result transport. A narrower fallback worth evaluating is strict parsing of the designated producer's complete final JSON output, captured by the authenticated supervisor with C3. Existing Claude/Codex final-text paths provide an implementation boundary; they do not yet provide the parser or proof. This would keep durable results and read APIs while removing shell submit from that path. It would **not** solve generic agent PR creation, task writes or agent replies; those promises need separate decisions. Do not silently remove result delivery from the MVP or disguise an unproven bridge as an implementation detail.

### 2. [P1] N6: object confidentiality lacks the serving/import/cache mechanism that its prompts must implement

**Confirmed, with one wording qualification.** [Design §5.3](lean-backend-design-v9.md#53-receive-pack-policy-the-git-boundary) expressly forbids treating hideRefs as the boundary. A hideRefs-only implementation would therefore violate the referenced design; it does not literally satisfy the whole contract. However, P4.3a's direct work and acceptance instructions fail to carry the concrete object-store requirement, making the implementation gap real.

Current [git.rs](../backend/g5-api-routes/src/routes/git.rs) authorizes repository access and invokes upload-pack/receive-pack on the full repository. Git quarantine does not restrict base-object lookup when that repository contains hidden objects. The host then compounds the problem: [repo_cache.rs](../backend/g5d-foundation/src/workspace/repo_cache.rs) hashes the repository URL for its key and fetches all heads; [materialize.rs](../backend/g5d-foundation/src/workspace/materialize.rs) reflink-copies that bare repository into guest `.git`. A narrower checkout or subsequent fetch does not erase previously cached hidden objects.

**Required closure:** P4.3a must name a self-contained, immutable serving repository built from the authorized root/OID closure, without global alternates, unrelated pack objects, refs or reflogs; bind it to current authorization generation, pin it during use, and specify build limits, storage owner, failure behavior and eviction. Receive-pack must operate in a scoped transaction repository, resolve thin bases only from permitted existing or supplied objects, and import verified objects/ref updates into the authoritative repository only after current-ACL/incarnation and old-OID checks. Global object lookup cannot repair missing hidden parents/bases.

P4.4b must replace URL or tenant/repository-only cache identity with authorized principal/workspace scope, exact roots and access generation, and materialize only that closure. Its acceptance must warm a private sibling first, then prove the other guest cannot `cat-file` its secret objects. The existing confidentiality row names P4.3a/P4.5; the reviewing agent's observation that **P4.4b is missing from that row is correct**. Include receive-pack laundering, thin bases, warm caches, revocation and eviction in that row and the direct prompts.

### 3. [P1] N8: the egress policy is not yet an implementable network boundary

**Confirmed.** The current [passt configuration](../backend/g5d-isolation/src/isolation/passt.rs) uses DNS at `169.254.1.1` and the loopback door at `169.254.1.2`. A blanket link-local denial contradicts the intended proxy route unless the exact service/port exception and enforcement point are specified. Category names such as metadata, host and private destinations do not define rules for IPv4/IPv6, passt translation, DNS or redirect/rebinding behavior.

The target needs one explicit Linux host enforcement mechanism applied before passt/VMM admission, isolated per VM. It must define the narrow guest DNS/proxy tuples, deny metadata including `169.254.169.254`, sibling/host listeners and disallowed private/direct destinations, and prevent the guest from bypassing the policy through alternate DNS or proxy routes. Exceptions cannot permit the whole `169.254.0.0/16` range or unrestricted host loopback. DNS forwarding and external-destination rules also need named configuration/state owners and validation.

**Required closure:** put the mechanism, exact exception tuples, policy lifecycle and setup-failure behavior in P4.4b/P4.12 Do items. Name the real guest probe and required CI selector; an isolation test that may skip when KVM is unavailable is not proof. Acceptance must require allowed broker/DNS success and denied metadata, host, sibling, IPv6 and unauthorized egress, with zero skips. Add the missing §14 risk/mitigation entry. One concrete candidate is host nftables OUTPUT filtering keyed by an exclusive per-incarnation passt UID, using its existing UID launch hook. The rules must match passt’s translated host sockets, not blindly copy guest-facing addresses into OUTPUT. Platform owns the resolver/mandatory deny set; a registered org config key owns approved destination tuples and resolves to a VM policy digest. Rule installation/removal and UID/port reuse need fail-closed tests. This is a proposal, not a selected or proven implementation, and it does not supply a macOS enforcement backend. These probes have not been run by this audit.

### 4. [P1] N5: mandatory preview isolation has no complete environment or legacy-path cutover

**Confirmed; “every QA/CI preview fails” is a predicted consequence, not an observed result.** P5.1 Do5 already mandates separate origins, so this remedy is not solely a trailer. But the exact old serving paths and environment prerequisites are absent from its completion criteria.

Retained [preview.rs](../backend/g5-api-routes/src/routes/preview.rs) still returns a same-origin URL without the suffix and sets the legacy path cookie. Even with a suffix, GET/HEAD redirect while other methods can continue into the platform-origin relay. [PreviewBrowserView.tsx](../frontend/src/components/preview/PreviewBrowserView.tsx) and its tests preserve the same-origin branch. These paths need explicit removal for every method and WebSocket sibling.

The deployment gap is concrete: [QA platform Terraform](../infra/qa/platform/main.tf) sets `preview_subdomain_bases = []`; [QA Helm values](../infra/helm/g5/values-qa.yaml) disable the wildcard. The networking module uses the platform DNS zone, and CI startup does not configure the required suffix. Adding `preview.qa.g5labs.ai` would still not satisfy the separate **registrable** domain requirement. Implementing fail-closed serving alone would break whichever retained preview flows exercise it.

**Additional integration issue found:** [preview_host.rs](../backend/g5-api-core/src/middleware/preview_host.rs) emits `SameSite=Lax` tickets. Moving to a separate site creates a third-party iframe cookie problem. A suffix/TLS change alone does not establish working embedded authentication. Select the supported browser/cookie-handoff strategy and test it; do not restore same-origin fallback to make the iframe work.

**Required closure:** P5.1 must own/co-own QA DNS zone selection, controlled domain, wildcard TLS, routing and Helm outputs, plus a two-domain trusted-TLS CI fixture. Extend the retained preview fixture in [citation-repo-routing.spec.ts](../frontend/e2e/flows/citation-repo-routing.spec.ts) with positive authenticated frame/new-tab/POST/WS flows and hostile-origin cases. Acceptance must verify no platform authority or guest bytes through legacy platform routes, as well as configured success and missing-config 503. Not every cross-origin action necessarily throws SecurityError: assertions must check the absence of authorized platform effects, rather than relying only on browser error text.

### 5. [P1] N4: the deployment lease bound omits recovery and is scheduled too late

**Confirmed.** [Design §6.4](lean-backend-design-v9.md#64-incarnation-lease-and-fence) admits a deploy when remaining lease exceeds the 120-second outage budget plus 15 seconds. A 136-second remainder followed by a 120-second outage leaves 16 seconds, with no budget for reconnect, reconciliation or acknowledged renewal. TTL 300 and `TTL >= 2 × outage_budget` do not establish a safe remaining-lease admission test.

The preflight also lacks an authoritative input definition. The host's monotonic deadline can precede the API's `lease_expires_at`; a database timestamp alone can overestimate remaining authority. A fresh host report must be adjusted for age/uncertainty, or a confirmed pre-deploy renewal must establish the usable horizon. The acceptance formula must include outage, reconnect, reconciliation/renewal and an explicit safety allowance, with measured bounds and refusal when evidence is stale or incomplete. The bound must also include preflight-to-shutdown delay and revalidate placements admitted after the snapshot. A proposed `workspace-lease-preflight` step belongs immediately before `helm-deploy` in `infra/ci/cloudbuild-deploy.yaml`, with freshness checked when the destructive step actually begins. This is a lease-admission issue, not a reason to revive expired authority.

**Required closure:** introduce the first safe deploy check and positive/negative restart tests with the MVP host/lifecycle delivery and the actual deploy lane. P8.7 may extend fleet rollout operations later, but cannot be the first protection for QA deployments that already restart the only API replica. Restore a positive P4.5 fake-host reconnect test while the host lease is genuinely live, plus a longer-outage expiry/loss test. Retain rejection of the original unsafe post-expiry grace. The QA test must include a running turn, native question, in-progress stop and queued capacity start. P8.7's “passes once” acceptance also needs reconciliation with the per-candidate claim in its trailer.

### 6. [P1] N7/N9 and the trailer pattern: safety acceptance does not prove the promised paths execute

**Confirmed consequence, with a qualification.** Paragraphs labeled “Review requirements (part of this PR)” are normative; an implementer is not entitled to ignore them. The defect is fragmented, sometimes contradictory instructions and completion criteria that can pass without exercising the required boundary. Some rules, including P5.1 origin isolation, are already in Do items.

P4.4b's acceptance does not require an enforced egress probe. P4.9b retains generic docker-ci acceptance; P4.9a says lanes are replaced but does not name the replacement receipts or eliminate its conflicting “do not touch” instructions. P4.4a does not enumerate all existing worker-start/wait consumers. P8.7 keeps the lease gate in its trailer. A green aggregate is insufficient when a lane can skip the relevant VM test or still use legacy worker readiness.

**Required closure:** move each actionable requirement into its owning Do step and Acceptance, with an existing or explicitly planned test/file, exact CI step/selector and no-skip expectation. Preserve same-PR cutover of startup, wait, seed, scenario and receipt consumers before removing the producer. Name and extend [test_cloudbuild_waitfor_order.py](../scripts/test_cloudbuild_waitfor_order.py); it was not deleted from the repository, but its required plan ownership/assertions were omitted. I ran that existing script: **10 tests passed**. That result checks today's configuration; it does not verify the proposed v9 replacement steps or security scenarios.

The concrete existing chains in `infra/ci/cloudbuild-ci-e2e.yaml` are `start-workers → wait-workers`, `start-value-loop-worker → wait-value-loop-workers`, `start-rust-e2e-worker → wait-rust-e2e-workers`, and `start-pa-worker → wait-pa-workers`. Their org seeds and workspace fixture consumers also need cutover. Host registration/tunnel readiness must precede fixture creation; the checked fork and explicit fixture-owner Start must then precede VM/dispatcher readiness. In particular, `seed-pa-workspace` must depend on its dedicated host wait when it starts that VM, rather than relying on the unrelated main worker wait. `scripts/seed-ci-org.sh` still produces old credentials and base-workspace writes; CI cannot retain those as hidden exceptions.

Named closure targets should include the existing ordering and `scripts/test_ci_docker_contract.py` checks in each CI-changing PR, a planned strict `workspace-vm-egress` step after `setup-kvm-libkrun`, a planned `backend/g5d/tests/workspace_vm_egress.rs` real-network test, and explicit result receipts consumed by `assert-ci-status`. These new target names are proposals; they do not exist or pass merely because this audit names them.

### 7. [P2] N32: oversized-flush handling still allows indefinite idle resource retention

**Confirmed.** The [bounds paragraph](lean-backend-design-v9.md#54-durability-agents-push-g5-flushes) simultaneously says deterministic errors need repair and schedules idle retry after five minutes. It does not define an owner/operator notification for this condition, retry suppression until relevant change, or an escalation/resource policy. Generic deadline-loss notification is not idle-flush notification. A cooldown reduces retry frequency; it does not resolve an unchanged oversized file or free the slot.

The original review proposed **recorded** exclusions and visible UI, not silent omission. The previous response mischaracterized that proposal. Rejecting automatic partial snapshots can still be justified by the chosen complete-capture guarantee, but the reason should be stated accurately.

**Required closure:** distinguish deterministic `FLUSH_TOO_LARGE` from transient transport failures; persist a blocked fingerprint, notify the owner once per distinct failure and expose it to operators, and retry only after relevant content/limit change or explicit retry. Define visible recovery and escalation. Automatic destructive release would change durability policy and must not be inferred from “idle.” Tests must prove no repeated unchanged capture and eventual successful stop after repair. This remains open rather than “idle avoids retry loop.”

### 8. [P2] N67: preflight safety is defined, but unattended cutover is still conditional

**Confirmed gap; the proposed automatic rename is not automatically the right remedy.** Main §12 and P4.5 require reviewed per-workspace owner/name mappings and zero unresolved rows before strict activation. That correctly fails closed, but still allows one unresolved legacy mapping to block the upgrade and supplies no complete positive migration/default-resolution procedure.

The earlier rejection of silently changing every workspace's display name remains reasonable. A deterministic **operator-approved bulk policy** for common-name legacy mappings could address the routine case without hand-editing every row. Divergent member branches/invalid owners still need an explicit resolution or a deliberately designed quarantine/defer choice; inventing an owner, creating missing refs or relaxing the common-name invariant is not an acceptable fallback.

**Required closure:** separate pre-deploy assessment/resolution from the unattended schema job; name the command, reviewed inputs, resolution rules, freeze/rebuild validation and rollback behavior. Add a positive upgrade fixture for representative legacy data, not only a test proving unresolved data aborts. State explicitly that arbitrary unreconciled datasets cannot be promised an unattended cutover. No new quarantine mechanism has been approved by this audit.

### 9. [P2] N60: erasure is acknowledged but has no owned deliverable or completion gate

**Partially confirmed.** It is not literally absent from P8.8: the existing trailer says user erasure/Git-history policy is separate, and design §5.8 also acknowledges it. Neither gives the policy an owner, deadline, §17 open-item entry, product behavior or discriminating acceptance criteria. The consequence remains open.

The refusal to claim that replacing author IDs erases Git/org content is correct. The missing action is to assign the policy decision and subsequent work, enumerate relevant stores/backups/provenance and define what departure, account deletion, tenant deletion and erasure each do. Do not promise complete erasure while the scope and retention decisions are unresolved. Put that explicit open item in §17 and its required decision/output in P8.8 Do/Acceptance.

### 10. [P2] N27: the token-type test matrix is claimed without a concrete owning prompt step

**Confirmed.** The type/audience/route matrix exists in the design and host contract. The review response assigns negative tests to P4.5/P4.12, but their direct steps/acceptance do not enumerate the wrong-principal matrix claimed there. Generic stale-incarnation and guessed-exec tests do not substitute for user-vs-host-vs-incarnation token validation.

**Required closure:** P4.5 owns API validator/route tests for missing/wrong audience/use, expired/revoked credentials, wrong org/workspace and forbidden transfer/create/admin methods with no writes; P4.12 owns injected-token verification and broker refusal. Include a permitted dispatcher path as a positive control. Agent-write positive controls additionally depend on the unresolved N3 mechanism. An always-denying service cannot pass the intended test.

### 11. [P2] N20/N52/N73/N74/N91: item-level dispositions are stale after the data-model revision

**Confirmed overall, with one correction.** The supersession note at the top of the response identifies it as historical, but its individual “Addressed” and validation paragraphs still read like current closure instructions. A structural check that all 125 headings remain present is not renewed substantive validation.

| Finding | Current mismatch | Correct current meaning |
|---|---|---|
| N20 | 20-candidate/21st-key/selection tests | One immutable attempt candidate; same canonical payload is idempotent, different payload conflicts; retain payload/finding/reply bounds |
| N52 | Stored `continuation_pending` invariant | Nullable continuation cause, hold rules and current owner/generation-bound start intent |
| N73 | Retain submission rows | Retain tombstone, canonical result and referenced producer attempt/candidate provenance |
| N74 | Independent candidate-table timestamp/schema framing | Candidate is an attempt envelope; **`captured_at` still exists inside it**. The claim that this timestamp vanished is incorrect |
| N91 | Same-version reply tests | Exact request/native request IDs and immutable response digest; durable `reply_pending` and journaled settlement remain |

**Required closure:** update or explicitly supersede each affected response in place while preserving the original review. State the audited candidate and distinguish behavioral requirement, selected mechanism, implementation and observed proof. Do not repeat an unqualified all 125-rechecked claim based on heading/count/link checks.

### 12. [P2] N1 and N109: small-looking prompt inconsistencies affect positive-path implementation

**N1 confirmed.** `RESULT_NOT_DESIGNATED_PRODUCER` appears in main §5.9 and Agent API §2.4, but not the exact P4.14 test contract. A generic two-session credential or list test is not producer-authority coverage. Add a planned request/TaskService test such as `review_result_rejects_other_session_producer`: live designated session A succeeds, live provenance-only session B receives the explicit authority refusal without candidate/result mutation, stale A fails, repeated Start designates once. The legitimate success control must use the selected N3 transport.

**N109 confirmed.** P4.4a's “Do not” assigns host registry columns to P4.13 while P4.1 creates them. P4.13 wording also distinguishes GCE bring-up from registration tokens despite the current all-host-kind exchange contract. Correct the ownership to P4.1 schema, P4.4a common exchange/credential/tunnel core and P4.13 provisioning/issuance UX, and test registration after the legacy-token deletion for every host kind. A trailer elsewhere does not resolve contradictory direct instructions.

## Closure criteria for the next revision

The next revision should make each changed requirement traceable through **concrete mechanism → producer/consumer paths → owning Do step → exact test/lane → Acceptance**. This needs focused edits to existing prompts and tests, not a new generic specification framework.

Before calling the design ready for implementation, resolve the N3 transport decision, make N6/N8 mechanisms executable, provision the N5 environment and embedded-authentication path, move N4 deployment protection into the MVP sequence, and reconcile N7/N9 CI cutover and receipts. Track the migration, erasure and deterministic-flush decisions as open until their positive recovery paths are specified. Preserve the current authorization and durability invariants while closing those consequences.

## Evidence and limits

The audit independently inspected current contracts, prompts, retained provider launch/result extraction, Git serving/cache/materialization, preview routing/cookies, QA/Helm/CI configuration, and the cited review records. The existing Cloud Build ordering test ran successfully (10 tests). No pinned-provider bridge, KVM egress, scoped Git export, malicious preview browser scenario, migration cutover or API-restart rehearsal was executed. Proposed test names and mechanisms above are recommendations, not existing passing tests.

All pre-existing v9 documents and earlier versions were preserved in this audit. The original 71-agree/54-partial counts remain historical dispositions, not a current readiness or implementation-completion score.
