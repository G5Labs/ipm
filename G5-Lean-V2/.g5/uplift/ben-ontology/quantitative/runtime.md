# Runtime quantitative coverage audit

Baseline: `072a12d3e99032ee3a7356837542c5edeb825984`. Source and canonical ontology were unchanged by the intervening reference-repair commit from `0b50e8d8`.

The denominator is **19 deliberately reviewed consequential trajectories**. It is not an inventory of every system path, symbol or ontology obligation. Current intent: **15 adequate, one partial, two missing, one explicit question**. Enforcement: **three selected trajectories traced, 16 partial**. **11** rows have named historical formal associations; none is new assurance at this baseline. This audit changed only its two quantitative reports and ran no model or runtime test.

Status meaning: adequate concerns written intent, not implementation correctness. Traced describes the bounded static trajectory in its row; partial names omitted writers/consumers/recovery. A formal association identifies an exact previously checked predicate, not current code verification. The JSON is the machine-readable source-range and limit inventory. Formal links point to immutable archives: [ben-coverage runtime at 072a12d3](https://github.com/G5Labs/G5-Lean/tree/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime) and [earlier runtime at 38046b28](https://github.com/G5Labs/G5-Lean/tree/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime). A concurrent archival change may remove the working-tree harness; no runnable checkout is assumed.

| ID | Reviewed behavior | Intent | Enforcement | Formal association |
|---|---|---|---|---|
| RT-01 | Durable explicit Start consent survives capacity waits and reconciliation without allowing cancelled, superseded, expired or owner-replaced intent to wake compute. | adequate | partial | `Consent.AuthorizedAttachment`; `Consent.NoAttachment` |
| RT-02 | Stop fences work, captures within its persisted save window, retries transient failures, records unsaved loss without reopening capture, and waits for host termination/release. | adequate | partial | `StopWindow.Safety`; `StopWindow.Completes`; `StaleRelease.NoStaleSettlement` |
| RT-03 | Owner inactivity can reclaim ready unused preview compute but not bounded work, preview startup or an open owner terminal; idle admission rechecks current activity. | adequate | traced | `IdlePreview.IdleAdmission`; `TerminalIdle.OpenTerminalPreventsIdle` |
| RT-04 | Admission serializes organization quota and host slots, requires published stable sizing, selects eligible same-region hosts, and derives boot disk capacity with visible override/cap consequences. | adequate | partial | `Placement.Safety`; `HostDisk.DerivedOrExplicit`; `HostAdmission.SlotBound` |
| RT-05 | Queued authorized demand drives bounded durable managed-host claims, subtracting other usable/in-flight supply; cloud retry rechecks demand and preserves uncertain resource ownership. | adequate | partial | None claimed |
| RT-06 | Automatic idle retirement requires continuous healthy empty custody, same-region spare capacity and no queued demand; cleanup preserves records and retries failed cloud deletion fairly. | adequate | partial | None claimed |
| RT-07 | Every Start stages a trusted fresh workspace root; cleanup claims only unheld paths before asynchronous deletion and retains unreaped path holds until proof or fresh replacement. | adequate | partial | `RootLifetime.LiveRootPreserved` |
| RT-08 | Host control retries ambiguous commands with the same method/key/payload; concurrent same-key calls serialize, altered payload conflicts, and retained completion survives tunnel loss within its process-local retention window. | missing | traced | None claimed |
| RT-09 | Host VM facts and per-VM sequence survive host restart, replay after reconnect, deduplicate with payload integrity, apply current-incarnation effects transactionally, and are removed only after a committed contiguous acknowledgment. | missing | partial | None claimed |
| RT-10 | Lease renewal and complete inventory remain bound to the current host/connection/incarnation; duplicate renewal cannot extend expiry and partial heartbeat cannot establish missing-VM death. | adequate | partial | `LeaseRenewal.NoDuplicateExtension`; `LeaseRenewal.PublicationFirst`; `HostInventory.CurrentConnectionPublishes`; `HostInventory.PartialPreservesCustody` |
| RT-11 | OCI content is digest-verified and extracted within the cache root, layer directory semantics are preserved, and complete templates/layers publish atomically before fresh per-VM roots use them. | adequate | partial | None claimed |
| RT-12 | Shared cache serves only trusted imported/seeded compile artifacts, refuses guest export, reserves temporary bytes before write, holds readers against eviction, and recovers accounting after interrupted fills. | adequate | partial | `CacheAccounting.AccountedDisk`; `CacheAccounting.ReadersRetainAccounting`; `CacheTrust.NoGuestExport` |
| RT-13 | Linux microVM guest egress installs a fail-closed host-local/metadata/link-local denial policy before networking while preserving narrow DNS/cache/API/reply exceptions; explicit development backends retain their stated isolation limits. | question | partial | `GuestEgress.Boundary`; `GuestEgress.InstalledBeforeExecution` |
| RT-14 | Execution journal recovery preserves exact settlements/dispositions, closes lost supervision before listening, and prunes only durably acknowledged prefixes without reusing sequence numbers. | partial | traced | None claimed |
| RT-15 | Owner terminals authenticate through the runtime facade, generation-fence reattachment/input, retire a shell with old nonabortable writes, and resume or explicitly reset bounded output offsets. | adequate | partial | `TerminalOwnership.NoOldInputAfterReattach`; `TerminalOwnership.GenerationOwnsEffects`; `TerminalReplay.ExactReplay`; `TerminalReplay.ResetOnUnavailable` |
| RT-16 | Preview task intent serializes exclusivity and replay, requires current owner/incarnation and listener readiness, and separates owner Close cleanup from lifecycle shutdown while retaining application exit diagnostics. | adequate | partial | `PreviewLifecycle.OnePreview`; `PreviewLifecycle.CleanupAfterExit`; `PreviewLifecycle.OwnerReadyStreams` |
| RT-17 | Preview browser admission uses the isolated target origin and current owner/incarnation, strips platform authority, confines application cookies, and refuses sibling-origin WebSocket use. | adequate | partial | None claimed |
| RT-18 | Filesystem RPC mutations own serialized checked publication and bounded replay while native edits remain outside that lock; watches report uncertainty/rescan and retry degraded coverage. | adequate | partial | `FileMutationCAS.NoUnobservedOverwrite`; `WatcherRecovery.DegradedRetainsRetry`; `FileVisibility.NoIgnoredAccess` |
| RT-19 | Dirty workspace flush retains one exact member vector and immutable snapshot/local-head cut across retries before completing its receipt; restart recovery uses that saved state without advancing the user branch merely to save. | adequate | partial | None claimed |

## Supported missing-intent proposals

### RT-P01 — g5.runtime.control.replay (high)

Owner: `.g5/ontology/software_architecture/hosts.g5.md`. restore concise agent D.

Draft: Bind host-command replay to method, idempotency key and original payload; a changed payload conflicts. Treat timeout as an unknown result and retry with the same identity within a bounded wait/processing budget. Retain completed results across tunnel reconnection for the advertised retention window; this process-local replay contract does not promise persistence across host-process restart.

Why the current bodies are insufficient: Version and incarnation rules allow a timeout to be treated as failed and a fresh command identity to be issued; they do not constrain payload conflict, replay retention or uncertain results.

Direct evidence: docs/worker-host-api-v9.md:103–105; backend/g5d-host/src/ctl.rs:190–385,592–705; backend/g5-api-core/src/ws/host_ctl.rs:115–174.

History: `67ad4b5d:.g5/ontology/software_architecture/runtime.g5.md` lines 110–114,432–436. Prior agent D explicitly required key/payload replay and timeout-as-unknown; metadata explicitly distinguished in-memory from crash durability.

Confidence: high for a recovered consequential contract; no new human requirement inferred from implementation alone. Do not turn the current 600-second default into a fixed permanent policy without separate authority. Lifecycle recovery after host restart uses current custody/event facts; this amendment must not create a command-durability guarantee.

### RT-P02 — g5.runtime.events (high)

Owner: `.g5/ontology/software_architecture/hosts.g5.md`. restore concise agent D.

Draft: Durably journal host VM facts and per-VM sequence state across host restart until acknowledged. Replay identical facts without applying their effects twice; conflicting reuse of a sequence fails. Commit deduplicated facts and applicable current-incarnation effects together before acknowledging only the contiguous stored prefix. Stale-incarnation facts remain history and cannot mutate replacement custody.

Why the current bodies are insufficient: Inventory and stale-instance fencing do not establish event persistence, exactly-once transactional application, payload integrity or acknowledgment ordering.

Direct evidence: docs/worker-host-api-v9.md:124–124; backend/g5d-host/src/ctl.rs:45–198,592–705,750–795; backend/g5-api-core/src/services/hosts/host_control_state.rs:363–537,870–898.

History: `67ad4b5d:.g5/ontology/software_architecture/runtime.g5.md` lines 122–135,444–455. Prior events D explicitly required durable sequencing and atomic deduplicated effects before contiguous acknowledgment; org-scoped effect authority is separately retained by current security/tenant rules.

Confidence: high for missing intent; enforcement remains partial. No claim of end-to-end exactly-once transport, infinite retention or crash-durable command-result cache. Contiguous-prefix query arithmetic has an unproven-reachability concern recorded in RT-09; do not silently assume its enforcement.

### RT-P03 — g5.workspace.module-execution (medium)

Owner: `.g5/ontology/software_architecture/workspace_vm.g5.md`. amend existing agent D.

Draft: Before serving after dispatcher restart, durably settle executions whose supervision was lost without relaunching their identities; recovery persistence failure refuses startup. Prune only durably acknowledged event prefixes, preserving sequence watermarks and retaining unacknowledged events beyond the journal soft target.

Why the current bodies are insufficient: The existing D forbids re-invoking already cancelled/settled identities and retains exact outcomes, but leaves an undecided interrupted identity and unacknowledged intermediate events unconstrained.

Direct evidence: docs/dispatch-runtime.md:141–141; backend/g5d-dispatch/src/server.rs:278–284; backend/g5d-dispatch/src/modules/mod.rs:445–479; backend/g5d-dispatch/src/journal.rs:200–295,516–607.

History: `67ad4b5d:.g5/ontology/software_architecture/runtime.g5.md` lines 137–147,456–466. Prior agent D blocks events.supervision and events.pruning explicitly retained these policies.

Confidence: high for missing qualifiers; amend existing block rather than introduce competing journal owner. Do not copy the literal 64-MiB soft target, private paths or every append/replay implementation mechanism into canonical intent. Full replay/live-event interleaving and crash durability matrix were not audited or modeled in this pass.

## Enforcement and historical-assurance limits

- **RT-01**: Placement itself checks pending/queued status and expiry; correctness of owner/generation relies on the shared locking and cancellation writers. All transfer, delete and rollout writers were not freshly traced here. Finite historical model does not prove queue fairness or physical uniqueness.
- **RT-02**: Receipt lookup binds this stop operation, not an older complete snapshot. The final capture-to-release half and every settlement writer were not freshly traced. Reachable host, eventual accepted stop observation and fair reconciliation are premises for completion; logical lease loss is not physical death. Historical RUNTIME-002 is a separate implementation concern; this pass does not claim its current closure.
- **RT-03**: Traced means the selected predicate/recheck and terminal activity trajectory, not every activity writer or default-setting writer. Quiet Ping/Pong traffic still keeps the terminal transport alive without calling activity.record; this corroborates existing RUNTIME-IDLE-TERMINAL as a code divergence, not missing intent. No runtime reproduction. Historical intended renewal variant is a proposal, not production enforcement.
- **RT-04**: Current quota is explicitly 10, range 0–100; no inferred promotion of scratch, cache, floor or cap literals. Administrative settings, host measured CPU/RAM resolver, all resource-release/rotation writers and frontend coefficient consumers were not freshly traced. Historical Placement excludes retirement races; HostDisk does not guarantee every configured slot fits after a deliberate small override or cap.
- **RT-05**: New canonical block already covers demand FIFO, region, quota+1 uncertain-resource bound, deterministic retry and disabling growth. Selected claim/retry path traced; full supply query, database triggers, generic Add Host/cloud compensation, settings writers and multi-replica cloud races remain outside this pass. Existing CloudRecovery/CloudLinkage models concern predecessor provisioning paths; no exact model association is claimed for this newer demand allocator.
- **RT-06**: New canonical owner already names gaps/unknown inventory resetting continuity and serialization with placement. Only selected retirement and cleanup claim/attempt-order paths read; actual cloud deletion finish, continuity triggers, settings races and every rotation interaction were not traced. Old HostRotation model is not proof of this independent empty-host retirement path.
- **RT-07**: Fresh replacement retires an obsolete path hold but does not prove old process death. Process mode keeps broader custody during failed reap. Full checkout cache trust chain, host-start process census, cancellation between filesystem swaps, arbitrary symlink races and external deletion were not proved. Historical model abstracts successful stage/swap.
- **RT-08**: Completed command results and execution gates are memory-only and expire; this is not crash-durable command-result replay. Durable lifecycle intent/event facts have different owners. Selected client/host replay path traced; every caller deciding to allocate a new incarnation after retry exhaustion was not enumerated. No current exact formal replay/conflict/budget model association found.
- **RT-09**: Current-incarnation inventory is related but does not entail durable events, sequence watermarks, atomic event/effect commit or ack ordering. Static question: contiguous_event_sequence returns first_gap.seq−1; for stored {1,3}, that is 2 rather than contiguous stored prefix 1. Whether this gap is reachable through the real ordered carrier, reconnects or other writers was not established. This is not a claimed reproduced production defect. Storage corruption, disk durability guarantees, schema constraints/pruning writers and all event consumers were not exhaustively traced. HostInventory explicitly excludes replay dedup.
- **RT-10**: Server tuple/sequence and guest publish-before-custody path inspected. Fresh private-host/relay trusted-connection tuple adapters and every complete-inventory release consumer were not reread. JWT verification, monotonic deadline enforcement, packet delivery and physical partitioned guest death are not established by these historical models.
- **RT-11**: This is source association to artifact integrity/isolation and explicit layer semantics, not proof of OCI parser/path safety. Manifest selection and every cache-hit validation path, cross-process locks, whiteouts, hardlink races and platform clone behavior are not exhaustively traced. BootContract checks selected pins/version gates, not immutable artifact verification; it is intentionally not counted as formal coverage of this trajectory. No new directory-mode or template-version proposal: current ontology already states those semantics.
- **RT-12**: Selected sccache read/import/refusal and accounting paths read. Fleet IAM scope, malicious storage changes, same-key replacement/open-unlinked lifetime, complete publication error cleanup and actual free-space races remain outside the pass. Historical CacheCredentialScope finding is separate from absent intent; no deployment IAM conclusion is made here.
- **RT-13**: The explicit process-mode Q is already represented: production remains microVM-only provisionally. No additional Q or blanket private-network isolation rule is proposed. Kernel rule equivalence, socket UID ownership, TSI routing, firewall interference, process/macOS isolation, launcher/backend selection and production deployment restriction were not proved. Model Forbidden destinations are a policy premise, not verification of actual nft packet behavior.
- **RT-14**: Traced scope is restart settlement and ack/prune path; all invoke/cancel/settlement writers, crash matrix and replay/live-stream interleavings were not fully retraced. Current block already requires no relaunch of cancelled/settled identity and independent exact outcome retention. Proposed amendment adds lost-supervision settlement and unacknowledged event retention only. No exact formal association for these restart/pruning trajectories located; execution/admission model names alone would not establish it.
- **RT-15**: Resize/close/output cancellation implementation was sampled, not fully followed; PTY reap, detach grace expiration, environment-secret sanitization and browser offset/rendering were not revalidated. Transport keepalive versus idle policy is separately counted in RT-03, not an extra terminal missing-intent proposal.
- **RT-16**: Admission and guest outcome paths read; later database task insertion/settlement, descendant group oracle, log masking and every cancellation/VM-loss projection were not fully reread. Existing historical preview model excludes whole-VM shutdown and stronger literal process-tree absence. Current diagnostic block already owns stderr/status behavior.
- **RT-17**: Read admission/relay/sanitization boundaries, not complete cookie parser, handoff-ticket lifecycle, ongoing stream revocation or every legacy route. No exact runtime formal association covers these browser security behaviors. Separate registrable-domain isolation is explicitly deferred, so same-site status is not a newly missing requirement.
- **RT-18**: Direct path validator still lacks dynamic ignore evaluation; historical RUNTIME-001 is implementation divergence from explicit intent, not an ontology omission. All direct read/delete/rename/watch endpoints were not reread. Native edit after final check is an explicit boundary, not a proved CAS guarantee against every writer. Request-per-process ambiguity is already represented by Q; source uses spawn_blocking here. Watcher descriptor confinement, full ignore/glob semantics, registration repair and frontend rescans were not proved.
- **RT-19**: Guest capture coordinator, vector admission and fixed-ref push read. Server receipt proof, storage authorization, session bundles, artifact pruning and native restore fidelity belong to wider Git/storage owners and were not fully traced here. FlushProof is documented in the workspace/Git campaign, but no direct model/property verification was performed here; parent should avoid double-counting that domain association.

The possible contiguous-ack arithmetic issue in RT-09 is a static concern with unestablished real-path reachability. It is not a new confirmed production defect, and was not used to infer intent. The command-cache and event-journal lifetimes must remain distinct.

## Already covered or excluded

- Managed host demand, independent idle retirement, rootfs explicit-directory metadata, process-local full-stack preview, preview diagnostics, quota default/range, published sizing and guest egress are already explicit current commitments.
- Quiet terminal idle reclamation and ignored-file access are existing code divergences under written intent, not missing ontology.
- Unreachable-host completion, physical singleton proof, per-request process isolation and development process mode retain explicit limits/questions; do not invent a new global guarantee.
- Numerical scratch/cache/template/journal defaults and internal paths are not promoted merely because source contains them.
- An absent or stale model does not establish missing intent.

Unexamined consequential areas:

- Full cloud Add/rotation/deadline-drain/compensation reconciliation and every host eligibility writer; controller settings writers and database continuity triggers.
- Host startup process census, KVM/krun launch/termination, mount namespace and TSI patch, actual firewall packets and external IAM capabilities.
- End-to-end provider artifact registry/manifests, malicious cache storage, complete image whiteout/hardlink behavior and cross-process fetch ownership.
- Full execution catalog/capability admission, descendant cleanup and late release veto, replay/live-event interleaving, disposition append crash matrix and all artifact/session bundle retention.
- Server flush receipt proof, exact native restore fidelity, artifact pruning, every Git writer and all save versus capture races.
- Preview handoff/session renewal and ongoing stream revocation, all terminal resize/close/env consumers and frontend browser behavior.
- All resource accounting writers and physical disk/CPU/RAM observations; no system-wide path denominator.

## Direct inspection ledger

Ranges below were directly read, not inferred from search hits. Full bodies were read before selected metadata; truncated interiors and nonexistent-path attempts are excluded. Historical reads are separately pinned in the JSON. Formal report/README association is not a fresh model run.

| File | Direct ranges |
|---|---|
| `.g5/TLA_verification/ben-coverage/runtime/README.md` | 1–41 |
| `.g5/TLA_verification/ben-coverage/runtime/manifest.json` | 1–25 |
| `.g5/ontology/concepts/security.g5.md` | 1–108 |
| `.g5/ontology/features/terminal.g5.md` | 1–70 |
| `.g5/ontology/software_architecture/git.g5.md` | 265–304 |
| `.g5/ontology/software_architecture/host_control_plane.g5.md` | 1–91 |
| `.g5/ontology/software_architecture/hosts.g5.md` | 1–230 |
| `.g5/ontology/software_architecture/workspace_vm.g5.md` | 1–280 |
| `.g5/uplift/ben-ontology/reviews/runtime-full.md` | 22–40 |
| `backend/g5-api-core/src/cloud/bringup.rs` | 65–135 |
| `backend/g5-api-core/src/services/agents/task_lifecycle.rs` | 555–705 |
| `backend/g5-api-core/src/services/hosts/host_capacity.rs` | 73–130, 206–397, 461–638, 678–742 |
| `backend/g5-api-core/src/services/hosts/host_control_state.rs` | 170–253, 270–291, 302–345, 350–636, 672–730, 870–898 |
| `backend/g5-api-core/src/services/hosts/worker_host_service.rs` | 284–456 |
| `backend/g5-api-core/src/services/workspace/workspace_runtime.rs` | 45–70, 323–390, 1708–1742 |
| `backend/g5-api-core/src/services/workspace/workspace_vm_service.rs` | 235–282, 746–940, 1930–1990, 2100–2128, 2180–2300, 2570–2625, 2628–2647, 2790–2840 |
| `backend/g5-api-core/src/ws/host_ctl.rs` | 45–92, 115–185, 350–383 |
| `backend/g5-api-routes/src/routes/mod.rs` | 530–630 |
| `backend/g5-api-routes/src/routes/preview.rs` | 425–515, 1052–1096 |
| `backend/g5-contracts/src/types/worker_host.rs` | 350–456 |
| `backend/g5d-dispatch/src/files.rs` | 385–416, 575–650 |
| `backend/g5d-dispatch/src/journal.rs` | 130–180, 200–295, 510–607 |
| `backend/g5d-dispatch/src/lease.rs` | 93–175 |
| `backend/g5d-dispatch/src/modules/mod.rs` | 440–515, 520–562 |
| `backend/g5d-dispatch/src/server.rs` | 255–300, 1580–1648 |
| `backend/g5d-dispatch/src/watcher.rs` | 270–325 |
| `backend/g5d-dispatch/src/workspace_flush.rs` | 74–195, 360–396, 562–602 |
| `backend/g5d-foundation/src/oci/cache.rs` | 147–195, 291–410 |
| `backend/g5d-foundation/src/oci/extract.rs` | 113–245, 295–337, 638–682, 747–829 |
| `backend/g5d-foundation/src/oci/registry.rs` | 833–949 |
| `backend/g5d-foundation/src/terminal/mod.rs` | 395–470, 506–552 |
| `backend/g5d-foundation/src/terminal/session.rs` | 155–260, 995–1065 |
| `backend/g5d-foundation/src/workspace/restore.rs` | 65–155 |
| `backend/g5d-host/src/cache_door/sccache.rs` | 50–145 |
| `backend/g5d-host/src/cache_door/store.rs` | 170–272, 402–463, 519–560 |
| `backend/g5d-host/src/ctl.rs` | 45–385, 592–705, 750–795 |
| `backend/g5d-host/src/workspace_vm.rs` | 247–350, 650–705, 735–787, 1150–1200 |
| `backend/g5d-isolation/src/isolation/guest_egress.rs` | 45–180 |
| `backend/g5d-isolation/src/isolation/passt.rs` | 288–315 |
| `backend/g5d/src/cli/preview_serve.rs` | 508–625 |
| `docs/dispatch-runtime.md` | 141–141 |
| `docs/worker-host-api-v9.md` | 100–128 |
