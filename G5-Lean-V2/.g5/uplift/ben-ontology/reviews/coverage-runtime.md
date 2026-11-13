# Runtime ontology coverage audit

Target: `/Users/ziniuw/Desktop/projects/G5-Lean`, HEAD `2d3bb3c586e0423d04bb2565a946f39602ba3f2b`. Read-only investigation; this report is the sole written file. No skill, application, ontology, model or test changes; no tests/model execution, commits or network publication.

## Method and limits

Read the current ontology bodies before relevant metadata. Used the uplift execution/source guides and authoring format/structure guides. Independently inventoried runtime entry points, state owners, alternate release writers, source documents, local chat-intent records and focused Git history. A search hit is discovery, not inspected-code coverage. A missing model clause is not counted as missing intent.

The decisive reconstruction question is: could an implementation obey every current ontology block yet omit or materially alter this outcome? The findings below pass that test. Local chat-intent documents reproduce human instructions and corrections, but the original complete conversations and referenced external issue discussions were not available in this pass. I distinguish those excerpts from their agent interpretations and from agent-authored design documents. No finding relies on a Git author's identity as evidence of human authorship. No explicit retirement of the substantive decisions below was located; the ontology rebuild itself is not evidence that the human revoked each omitted policy.

This is a bounded runtime pass, not whole-system completeness or implementation assurance. It found seven distinct missing areas, with a secondary resource-accounting candidate. Numeric percentages would be misleading.

## Prioritized missing intent

<a id="r-int-1"></a>

### R-INT-1 — Guest metadata and host-local network protection has no current owner (high confidence; security)

**Existing nearest commitments:** `g5.host.isolation` (`software_architecture/hosts.g5.md:93–96`) promises microVM isolation/no fallback; `g5.host.tenancy`, `g5.security.tenant`, and `g5.concept.host` promise tenant/VM separation. None says that an otherwise isolated guest cannot access the cloud metadata service or the host's local services, or that enforcement failure blocks boot. Same-organization host metadata is not necessarily a cross-tenant access, and VM isolation does not imply an outbound firewall.

**Evidence of intended meaning:** [.g5/chat_intent/sec-002/guest-egress.md:3–5](../../../chat_intent/sec-002/guest-egress.md#L3) records the human instruction to fix SEC-002 and the explicitly bounded interpretation: enforce metadata/host-loopback protection before execution; preserve DNS/cache/API/preview needs; retain process/macOS development exceptions; do not claim general private-network isolation. [docs/v9-mvp-scope-amendment.md:32,43,51](../../../../docs/v9-mvp-scope-amendment.md#L32) explicitly activates this bounded exception to earlier deferred network hardening. Commit `67ad4b5d20ac1a291382733854292fa42a494fdf` added it as `g5.security.mvp-exposure`, including trust in external firewall administrators and the lack of automatic repair after an external ruleset flush. That owning rule is absent from the current body.

**Current code owners inspected:** `g5d-isolation/src/isolation/guest_egress.rs:21–54,68–89,92–172` derives local API exceptions, builds the host policy, rejects zero/root inputs, denies loopback/link-local/cloud-metadata/local destinations and fails on installation errors. `isolation/passt.rs:266–313` installs the complete policy before spawning passt/delivering the guest network boundary. The Linux TSI patch's execution path in [scripts/build-libkrun.sh](../../../../scripts/build-libkrun.sh) was found in the commit but not directly audited here; do not claim full networking conformance.

**Counterexample permitted by current ontology:** run every workspace in its own libkrun VM, with its own disk and correct owner credentials, while allowing requests to cloud metadata and arbitrary host-loopback services. This can satisfy the written VM/tenant rules while undoing the human-requested repair.

**Minimal proposed wording / owner:** restore one security owner, e.g. `g5.security.guest-egress`: “Before customer execution, Linux microVM networking denies access to cloud metadata and link-local services, while preserving required DNS and narrowly configured cache/API access and replies to host-initiated connections. Enforcement failure prevents boot. Retain the documented TSI guest-local translation and trusted process/macOS development exceptions; this is not general private-network isolation.” Preserve the policy-maintenance/external-firewall assumption in the body or a child if it still governs. This is recovered intent, not a proposal to reinstate the larger deferred organization egress-policy system.

<a id="r-int-2"></a>

### R-INT-2 — Stop's bounded save and force-completion contract disappeared (high confidence; lifecycle)

**Nearest commitments:** `g5.host.stop` allows graceful/kill operations; `g5.vm.flush` says capture is supported; `g5.workspace.transfer(.formal)` requires old compute termination before owner change; `g5.vm.uniqueness` says a failed VM cannot block a new one. They do not say that *every* stop must proceed after saving fails, how long ordinary saves may hold Stop, or that Stop must not reopen admission. A system that indefinitely retries a poisoned flush, or aborts Stop back to running, can satisfy those individual capabilities.

**Human evidence with correction:** [.g5/chat_intent/nuclear-stop/request.md:5–21](../../../chat_intent/nuclear-stop/request.md#L5) contains the user's complaint, “Every stop” choice, later rejection of a new loss display, and final choice of two minutes. Lines 34–39 explicitly identify the superseded retain-running-on-save-failure policy and the unreachable-host exclusion. Commit `527886a5` (“Make every workspace Stop complete within its save window”) implements this; the previous lifecycle ontology at `67ad4b5d:.g5/ontology/software_architecture/lifecycle.g5.md:39–79` separately owned stop fencing, forced completion, window, loss recording and release. Earlier v9 paragraphs that retain the VM on flush failure are superseded by this later explicit correction; they must not be restored accidentally.

**Current code inspected:** `workspace_vm_service.rs:48–58` defines two-minute ordinary save, separate deadline-stop save and host reap budgets; `746–1033` resolves current stop, leaves unreachable-host stops pending, closes admission, settles attempts, drains, flushes inside the window, records loss and chooses immediate kill on failure/expiry. `2474–2533` also cancels queued Start while already stopped. `2629–2710` rechecks idle state, distinguishes future deadline nudges from actual stopping, and advances lifecycle generation. Host `workspace_vm.rs:1285–1407` and exit monitor `1430–1452` are alternate release paths. They expose a separate physical-proof question, not an absence of stop intent.

**Minimal proposed wording / owner:** under workspace VM lifecycle: “Stop fences new work and older Start consent before settling and saving. Owner, idle, transfer and ordinary Delete stops attempt saving for at most two minutes; deterministic save failure or expiry ends guest capture and forces termination. Never return to running merely because saving failed. Record unsaved loss without requiring a new loss display; the next explicit Start restores the last complete receipt. Deadline and force-Delete paths retain their distinct windows. An unreachable host remains pending under the host-loss/custody rules.” Keep independent window/loss/ordering blocks if needed; do not turn the two-minute save budget into a promise of physical termination despite an unreachable host.

<a id="r-int-3"></a>

### R-INT-3 — Fresh-root materialization and deletion after release are no longer required (high confidence; security/resource lifetime)

**Nearest commitments:** `g5.host.storage` gives each VM its own writable filesystem; `g5.host.checkout`, `g5.vm.boot-checkout` and `g5.vm.resume` bind repository/restoration content. They do not forbid reusing guest-written repository metadata on the host or keeping released ignored content indefinitely. `g5.design.durability` asks to define durability but does not choose this system's root lifetime.

**Human/source evidence:** [.g5/chat_intent/retained-roots/request.md:5–19](../../../chat_intent/retained-roots/request.md#L5) records the concern about old files, history investigation, and explicit choice “B: stop keeping roots” plus “Implement and ship.” Lines 23–26 preserve the fresh-root/isolation rule and distinguish Docker scratch retention and the host Git cache from the requested change. [.g5/pr_ontology_diff_design/retained-roots/ontology-plan.md:26–31,35–50,52–93](../../../pr_ontology_diff_design/retained-roots/ontology-plan.md#L26) records the final option, alternatives, authoritative owner, failure retry and shared-cache exception. Commit `3e0b771e` (“Delete a workspace root when its VM is released”) corroborates it. Later [.g5/chat_intent/cross-vm-cache-sharing/request.md:38–47](../../../chat_intent/cross-vm-cache-sharing/request.md#L38) still treats fresh roots/no retained roots/no host Git in guest-written content as governing constraints; the cache feature did not revive root reuse.

**Code inspected:** host `workspace_vm.rs:199–348` claims only unheld host-owned roots, serializes claims against Start, deletes asynchronously without following symlinks, and retries failures; `486–503` retries cleanup on disk refusal; `725–776` retains unreaped root holds; `1154–1194` explains and implements fresh materialization before receipt restore; `1318–1343,1385–1407,1430–1452,1494–1497` connect stop, natural exit, monitor and startup sweeps. `g5d-foundation/src/workspace/restore.rs:65–152` builds a fresh staging tree, uses the trusted bare cache, swaps it into place and never runs host Git in the previous guest tree.

**Counterexample:** retain a per-workspace root forever, then run host Git against its guest-authored hooks/config during Resume, while ensuring no other workspace mounts it and checking out the required branch. Current wording does not reject the first behavior and does not clearly express the host-trust prohibition governing the second.

**Minimal proposed wording / owner:** host storage/materialization: “Every Start builds a fresh root from trusted host materialization and the selected durable receipt; host Git never executes in a tree previously writable by a guest. After release, remove workspace roots no live or unreaped incarnation holds, safely against concurrent starts, retrying failed cleanup. Released roots are not a warm cache; keep the separate trusted repository/artifact caches.” This should not claim physical secure erasure or forbid temporary cleanup leftovers.

<a id="r-int-4"></a>

### R-INT-4 — Host boot-disk sizing, override continuity and one calculation authority are missing (high confidence; capacity)

**Nearest commitments:** `g5.host.slots` advertises CPU/memory/disk and `g5.runtime.placement.sizing` explicitly specifies measured CPU/RAM equal shares. Neither governs the *host's shared boot disk*, derives it from slot requirements, preserves an operator's explicit disk choice, or keeps automatic defaults dynamic across replacement.

**Human evidence:** [.g5/chat_intent/scale-worker-host-disk-with-slots/host-disk-per-slot.md:9–21,25–64,66–99](../../../chat_intent/scale-worker-host-disk-with-slots/host-disk-per-slot.md#L9) records issue #344, the QA disk exhaustion motivating it, per-host sizing/override behavior and the human correction at line 70 rejecting duplicated `65` constants: “we should Derive.” The requested outcome at lines 56–64 expressly includes `.g5` ontology. Focused history identifies initial design commit `d06753e2`, correction `e2159280`, and merge `4e71e4b7`; this pass read the source record rather than attributing intent from those authors. The subsequent cache-door design allocates an additional cache share, so restoring the original 40 GB-only base literally would be stale.

**All material provisioning writers checked:** `g5-contracts/src/types/worker_host.rs:350–456` owns the coefficients, deriving per-slot GB from configured scratch plus 512 MiB and adding the cache-door allocation; `cloud/bringup.rs:65–135` owns common resolution and loud cap/invalid-persisted-override handling; `routes/worker_hosts.rs:624–648,888–926` validates explicit requests and resolves Add-host provisioning; `cloud/rotation.rs:890–908,965–1006` copies explicit overrides and re-derives NULL values; `cloud/stale_host.rs:650–692` applies the same resolution to provisioning retry; `cloud/gcp.rs:882–900,1235–1266` receives the per-host size. Host `workspace_vm.rs:486–503` uses the corresponding free-space admission requirement.

**Counterexample:** provision every host with one fixed disk size, or turn omitted defaults into frozen persisted values, while reporting correct slot CPU/RAM/disk resources. Both obey the current ontology and recreate the reported failure or defeat future sizing updates.

**Minimal wording / owner:** HCP provisioning/host capacity: “Derive a host's shared boot disk from its resolved VM-slot count, the configured per-VM scratch start requirement and explicit host/cache allowances, with the supported floor/cap. An explicit bounded operator size overrides the default and survives replacement; an omitted size is re-derived at each provisioning/rotation/retry. Publish the same sizing coefficients to clients instead of duplicating the rule, and make capped or unusably small sizes visible.” Numeric present policy can be a small child; preserve the human derivation correction rather than freezing today's computed 65 GB.

<a id="r-int-5"></a>

### R-INT-5 — Explicit Start consent and durable queued-start authority are missing (high confidence that contract is absent; medium-high provenance confidence)

**Nearest commitments:** owner-only VM use (`g5.security.owner`), ability to Start (`g5.host.start`), scheduling (`g5.hcp.scheduling`) and incarnation identity do not require a fresh deliberate owner Start/Resume before attaching compute. Current preview admission records a preview's start intent, but it does not govern generic workspace wake-up or queued chat.

**Document/history evidence:** [docs/workspace-vm-api-v9.md:194–202](../../../../docs/workspace-vm-api-v9.md#L194) explicitly binds durable Start consent to owner/generation, distinguishes authorized rollout continuation, gives queued starts a bounded lifetime, and says generic dispatch/queued tasks cannot authorize waking. [docs/v9-meeting-decisions.md:19,22,24,35](../../../../docs/v9-meeting-decisions.md#L19) separates meeting decisions from later agent draft choices; exact Start-record details are design-originated, not falsely claimed human quotes. `nuclear-stop/request.md:7,38` reinforces explicit Start *after* Stop. Historical lifecycle body at `67ad4b5d` owned this separately. No investigated later correction revokes it.

**Code inspected:** `workspace_vm_service.rs:451–535` records intent before placement and queues lack of capacity; `2100–2157` binds owner/Base/transfer admission; `2184–2275` preserves exact replay and rejects superseded queued intent; `2474–2533` cancels pending intent on Stop. `worker_host_service.rs:284–410` serializes organization admission, checks durable pending intent and expiry, and places only on ready hosts with reported slot sizes and free occupancy. The one-consumption/three-failure/reconciler FIFO implementation was not traced end-to-end in this pass, so the minimal finding is explicit durable consent, not proven conformance to every historical retry number.

**Counterexample:** any queued chat silently boots a stopped workspace under the current owner's account, or an old queued request boots after Stop. Owner-only and incarnation isolation can still hold.

**Minimal wording / owner:** workspace lifecycle: “Attaching compute requires durable, current-owner and lifecycle-generation-bound explicit Start/Resume consent, including clearly labeled compound actions. Queue that consent through capacity waits/API restart with bounded expiry; superseded or cancelled intent cannot be revived by retries. Ordinary reads, Save, queued work, Stop and transfer do not independently authorize waking. Previously authorized rollout continuation remains a separately scoped exception.”

<a id="r-int-6"></a>

### R-INT-6 — Idle means actual owner activity; a starting preview has a special exemption (high confidence for preview rule; broader idle detail needs source reconciliation)

**Nearest commitments:** `g5.hcp.api` only names reclaiming idle slots. `g5.preview.service` exempts previews from bounded-work slots/wall-clock limits; it says nothing about the idle reaper. A system may stop an explicitly requested slow preview build merely because the owner has not clicked again, while satisfying every current preview block.

**Human/source evidence:** [.g5/chat_intent/preview-idle-stop/request.md:5–20](../../../chat_intent/preview-idle-stop/request.md#L5) records the diagnosed thirty-minute stop during build, the human “change it,” and accepted distinction: not-ready previews prevent idle selection; unused ready previews do not; output/keepalives are not activity. Commit `75b12ed0` added `g5.lifecycle.idle.preview-startup` and narrowed `g5.preview.service` accordingly; both details vanished. [docs/workspace-vm-api-v9.md:132](../../../../docs/workspace-vm-api-v9.md#L132) independently distinguishes actual authorized preview use from output/keepalives. Historical lifecycle body has broader owner-file/terminal/native-wait policy; do not restore its old timeout blindly, because later code/source use eight hours.

**Code inspected:** `workspace_vm_service.rs:262–280` recognizes running agent/command and queued/running not-ready preview as blockers; `2035–2097` selects and retries idle stops; `2629–2643` rechecks idle under the workspace/VM locks, preventing a stale selection from interrupting newly active work. `workspace_runtime.rs:45–67,594–604,1715–1737` owns incarnation-fenced owner activity recording. The terminal's full periodic activity writer and every native-wait projection were not re-audited, so do not claim their matrix complete.

**Minimal wording / owner:** workspace lifecycle idle rule, referenced by preview service: “Idle reclamation follows configured owner inactivity and rechecks current work before stopping. A requested preview prevents idle stop until ready; once ready, unused serving/output/keepalives do not keep the workspace alive. Count actual authorized owner interaction, and preserve the ordinary bounded-work/open-terminal rules.” The last clause should reference a recovered canonical activity owner; it is not supplied by current HCP wording.

<a id="r-int-7"></a>

### R-INT-7 — Preserve why a preview ended, separately from the raw module disposition (high confidence; observable lifecycle behavior)

**Nearest commitments:** `g5.vm.preview-stop` and `g5.runtime.preview.admission` govern Close/cleanup and admission; `g5.preview.service` requires a visible cancellable task. None requires the outcome to distinguish owner cancellation from idle stop, transfer, host drain, required restart or VM loss.

**Evidence:** [.g5/chat_intent/preview-idle-stop/request.md:14–18](../../../chat_intent/preview-idle-stop/request.md#L14) makes the second accepted fix explicit: record and display the stop reason rather than a bare “cancelled.” Commit `75b12ed0` added `g5.preview.service.stop-reason`. Current `task_lifecycle.rs:2072–2100,2192–2213,4450–4487` applies the reason to pre-start cancellation, VM loss and module cancellation; `workspace_activity.rs:117–125,372` carries it to the durable activity projection. The frontend renderer was not re-inspected in this runtime pass.

**Counterexample:** every interrupted preview ends as “cancelled,” while Close works and previews remain visible tasks. That obeys current ontology but repeats the specific confusing behavior the user asked to fix.

**Minimal wording / owner:** preview lifecycle: “When a workspace stop or loss ends a preview, retain the lifecycle reason in the task outcome and show it to the owner; a raw module cancellation alone is insufficient.” Keep this separate from R-INT-2's explicit *no new unsaved-loss display* decision: these are different notices.

## Secondary incomplete commitments / handoffs

- **Organization VM quota and occupied-capacity accounting:** current `g5.host.slots`/`g5.hcp.scheduling` describe capacity but never name the organization-wide limit or that starting, running and stopping custody consume it. `worker_host_service.rs:284–410` checks `max_running_vms` (default observed 10), serializes admission, records a distinguishable quota wait and counts all three statuses for org and host occupancy. [docs/workspace-vm-api-v9.md:196](../../../../docs/workspace-vm-api-v9.md#L196) requires capacity queueing subject to quota. This is a good candidate addition to R-INT-5's capacity owner, but no direct human source for the particular default or fairness rule was found; preserve the configurable quota obligation without treating `10`/specific SQL as human intent.
- **Default sixteen workspace executions and VM headroom:** explicit human decision in [.g5/chat_intent/workspace-execution-slots-16/request.md:5–37](../../../chat_intent/workspace-execution-slots-16/request.md#L5); current `g5.chat.dispatch-eligibility` only says capacity is shared. Sent to chat/identity auditor for its code trace. Do not count this report as inspecting `agent_dispatch.rs` or proving current scheduling limits.
- **Process-mode scope:** current `g5.runtime.local-mode-scope` already captures the core conflict and provisionally restricts production to microVMs. [.g5/chat_intent/process-isolation-local/request.md:5–18](../../../chat_intent/process-isolation-local/request.md#L5) and its saved design specify explicit opt-in, acknowledgement, local-only API admission, no fallback, advertised exposure and host-native runtime validation. These deserve reconciliation with that Q, but this is not honestly a wholly undiscovered missing domain. Full process-census/platform validation was not audited.
- **Sizing publication stability:** `host_control_state.rs:141–245` requires a measured size before placement and prevents changes while active custody exists; legacy protocol hosts retain custody/control but cannot receive new placement. `g5.runtime.placement.sizing` only specifies host-start derivation. Its paired human-source excerpt mentions publishing actual sizes and keeping them stable while VMs run (`host-equal-resource-slots/resource-sizing.md`). A short child could preserve this cross-version/live-custody condition; it is not a separate top-level resource system.

## Obligation inventory and dispositions

This denominator is the *identified runtime obligations*, not every source file or every feature of G5. “Present” means governing intent exists, not that implementation/tests pass.

| Capability / independently significant obligation | Current owner(s) | Disposition | Actual code/source owner inspected or bounded limit |
|---|---|---|---|
| One org per host, isolated per-VM execution, no silent fallback | `g5.host.tenancy`, `g5.host.isolation`, `g5.concept.host` | Present; process exception already Q | Host/VM admission and process design; full isolation backend not audited |
| Guest metadata/host-local deny boundary | no current specific owner | Missing R-INT-1 | guest_egress, passt startup, SEC-002 history |
| Immutable image selection, boot spec and runtime catalog readiness | `g5.host.image-selection`, `.immutable-image`, `.boot-spec`, `.runtime-bundle`, `.readiness` | Present, no new intent finding | boot preparation/spec entry inspected; OCI extraction/registry auth unexamined |
| Equal measured CPU/RAM slots | `g5.runtime.placement.sizing` | Present; publication stability partial | host hello resource contract; human sizing excerpt |
| Shared host disk sizing/override continuity | none (`.sizing` is CPU/RAM) | Missing R-INT-4 | shared types, Add, bringup, retry, rotation, GCP body, host free-space gate |
| Organization quota and custody accounting | HCP scheduling/host slots only | Partial candidate | placement transaction 284–410 |
| Explicit owner Start and durable/replay-safe queued consent | no general lifecycle owner | Missing R-INT-5 | resume, record_start_intent, place, Stop cancellation |
| Stop fencing/save/force/loss/next-start behavior | host stop, VM flush, transfer only | Missing R-INT-2 | Stop admission through host call; alternate host exit writers |
| Owner inactivity, pending preview build exemption | only API “idle” label | Missing R-INT-6 | idle query, locked recheck, activity writer |
| Fresh checkout from trusted host state, released-root deletion | host storage/checkouts only | Missing R-INT-3 | fresh materializer; root reaper, every located release family |
| Lease current identity, increasing replies, send-time/credential bound, publication order | `g5.runtime.lease.renewal` | Present | guest lease 1–126; API renewal 550–637 |
| Complete authenticated host inventory vs partial heartbeat and connection fencing | `g5.runtime.host.inventory` | Present | hello 129–267; linked current metadata; heartbeat/event owner inventory discovered but not fully reread |
| Physical singleton vs logical lease expiry | `g5.vm.physical-uniqueness-question` | Explicit unresolved choice, not missing | existing Q and current kill/proof/release code |
| Cloud rotation/accepted insert/recovery/compensation ownership | `g5.runtime.host.rotation`, `.cloud-linkage`, `.cloud-recovery`, `.compensation` | Present | relevant metadata plus provisioning/rotation/retry sizing path; full network/compensation flow not reread |
| Cache no-result-change, trusted compiled producers, org fleet scope, image recipe keys | `g5.host.cache-door`, `.cache-trust`, `.cache-fleet`, `.image-tiers` | Present | cache door facade/config 1–135 and cross-VM source; not all store/registry methods |
| Temporary bytes, eviction/read leases, cancellation/startup accounting | `g5.runtime.cache.accounting` | Present | current body/metadata only in this pass; no new conformance claim |
| Files restricted to checkout/nonignored; mutation CAS, completion/replay, bounded watch recovery | `g5.vm.filesystem`, `g5.runtime.files.mutation`, `.observation-recovery` | Present; known ignore divergence is code gap | files 1–190; server dispatch 1438–1515; metadata source constraints |
| Terminal owner authority and attachment generation/tail offset reset | `g5.terminal.*`, `g5.runtime.terminal.attachment` | Present | full current body/metadata entry; terminal session implementation not reread here |
| Preview exclusivity, ready port admission, origin/owner security, Close cleanup | `g5.vm.preview-exclusive`, `.preview-stop`, `g5.runtime.preview.admission`, `g5.workspace.preview-origin-security` | Present | source request and current bodies; task stop paths; full relay unexamined |
| Meaningful preview stop reason | no current owner | Missing R-INT-7 | task cancellation/prestart/loss writer families; activity projection |
| Declared module admission/capabilities/concurrency; journal before effects; retained exact outcomes | `g5.vm.catalog`, `g5.workspace.module-execution` | Present; capacity default handoff | server admission, dispatch-runtime documentation; runner implementation discovered, not comprehensively reread |

## Rejected apparent omissions and separate gap categories

1. **No runtime/model execution is not missing ontology.** This pass executed no tests or models. Existing metadata distinguishes finite properties, code inspection and missing runtime evidence. I make no claim that the former TLA campaign validates every inventory row.
2. **Filesystem ignores:** `g5.vm.filesystem` already explicitly excludes gitignored files. Existing RUNTIME-001 is an implementation divergence; adding another block would duplicate intent. The root/path checks read here do not establish ignore enforcement.
3. **Delayed release touches a successor:** `g5.data.identity.incarnation` already covers it. Existing RUNTIME-002 is a code/transaction-scope gap, not missing intent.
4. **Preview cleanup before verified app exit:** `g5.vm.preview-stop` already says stop application before cleanup; RUNTIME-004 is not an intent omission.
5. **Fleet token broader than one organization:** `g5.host.cache-fleet` already requires organization-scoped reader credentials. RUNTIME-006 is deployment/implementation mismatch; the metadata-boundary omission is separate.
6. **Rotation placement race and orphan compensation:** current cloud owners expressly require fresh serialized custody and exclusion through deletion. Existing RUNTIME-007/008 do not justify new intent findings. I did not re-prove them here.
7. **Host-enforced lease expiry / per-request process isolation:** the current physical-uniqueness and request-isolation Q blocks already expose the uncertainty. Absence of a host timer or threaded filesystem implementation is not a newly missing domain.
8. **No guest writes to fleet compiled cache, trusted producer restriction, recipe-only image tier keys:** all are explicit current commitments. Exact cache protocol/layout/path filters are mechanisms unless additional authority evidence requires them.
9. **General organization-controlled default-deny egress, credential broker, durable billing replay:** older v9 text explicitly marks these post-MVP; SEC-002 expressly does not reinstate broader network policy. Do not promote their code absence into either missing current feature intent or a new security requirement.
10. **Preview cleanup on whole-workspace stop/VM loss:** [.g5/chat_intent/preview-close-cleanup/request.md:21](../../../chat_intent/preview-close-cleanup/request.md#L21) excludes it. The Close/natural-exit cleanup promise must not be broadened into all lifecycle termination paths.
11. **Mandatory loss UI for forced VM stop:** explicitly rejected by the later human correction (`nuclear-stop/request.md:17–21,37`). Preview reason display is separately required and compatible.
12. **Retain roots for performance:** explicitly rejected; the cache-sharing feature preserves fresh roots and separate artifact caches. Earlier warm-root prose is stale evidence, not an omitted current requirement.

## Inspected scope / unexamined consequential areas

Current ontology bodies read: root, concepts data_model/security/system_invariants/system_design_principles, architecture hosts/host_control_plane/workspace_vm, features terminal/workspaces/operations/chat; relevant Git body search and owning references. Relevant metadata inspected for lease, inventory, CPU/RAM sizing, cloud ownership, files, preview, module execution, physical/request isolation and existing runtime gaps. Did not treat other feature files as completely read based on search.

Direct code ranges supporting findings are listed under each finding. Additional source inventories: all filenames under API core/foundation/routes, host/isolation/dispatcher/foundation; public functions in principal runtime owners; focused history for host/isolation/dispatch and chat-intent changes. Large output that was truncated was not treated as full-file inspection. No whole-file/source-line coverage percentage.

Important areas left for further investigation: full cloud API retry/rotation reconciliation including all deprovision callers; host control journal/idempotency/event replay and mux resource cleanup; OCI auth/extraction/image cache invalidation and runtime bootstrap; process-mode census/restart recovery and host admission; terminal session implementation and every idle activity writer; preview HTTP/WebSocket relay/profile validation; filesystem full tree/watch/mutation methods and all native writers; compiled-cache store/upstream allowlist/credential handling; module journal corruption/budget/replay/cancellation and runner descendant cleanup; provider usage/billing accounting (chat/identity owner); deployment/runtime qualification lanes. These could contain additional important intent absent from both this report and the ontology.

Recommended next amendment scope is small owning commitments for the seven concrete areas, with source lineage retained and the two provenance/behavior distinctions above preserved. This report does not authorize or perform those amendments.
