# Host

<a id="g5.host"></a>
[I · g5.host]

A Host is a machine in one organization that runs that organization's workspace VMs.

<a id="g5.host.tenancy"></a>
[I · g5.host.tenancy]

A Host belongs to one organization and is never shared between organizations.

<a id="g5.host.lifecycle"></a>
[I · g5.host.lifecycle]

Each Host has and exposes lifecycle and health information  

<a id="g5.host.kind"></a>
[I · g5.host.kind]

Hosts have a kind, which determines which settings are relevant  

<a id="g5.host.cloud"></a>
[I · g5.host.cloud]

Cloud Hosts have instance\_type, instance\_id, and region fields.  While initially supporting GCP, the platform should generalize to run on any major cloud provider.  

<a id="g5.host.macos"></a>
[I · g5.host.macos]

macOS Hosts are for development and are not a production environment  

<a id="g5.host.drain"></a>
[I · g5.host.drain]

A host can be drained which gracefully shuts down all contained workspace VMs while preventing new VMs from starting on the host.  Once no VMs are running on the host it is fully drained and the host is de-provisioned  

<a id="g5.host.telemetry"></a>
[I · g5.host.telemetry]

A Host reports telemetry useful for system observability and debugging (eg active VMs, disk usage, cache state, error information, retry counts)  

<a id="g5.host.slots"></a>
[I · g5.host.slots]

A host has a configured capacity in slots and it advertises the resources (cpu, memory, disk) provided to each slot on the host  

<a id="g5.host.vpc"></a>
[I · g5.host.vpc]

A host may have a configured VPC  

<a id="g5.host.start"></a>
[I · g5.host.start]

A Host can start a VM.  This boots a workspace VM with its image, resources, repositories, and incarnation information  

<a id="g5.host.stop"></a>
[I · g5.host.stop]

A Host can stop a VM either gracefully or with a kill operation  

<a id="g5.host.status"></a>
[I · g5.host.status]

A Host reports on VM status as requested  

<a id="g5.host.heartbeat"></a>
[I · g5.host.heartbeat]

A host reports heartbeat information on host liveness.  Lack of Host liveness is insufficient to show that contained VMs are dead  

<a id="g5.host.image"></a>
[I · g5.host.image]

A Host has a versioned named image which it runs  

<a id="g5.host.cache-door"></a>
[I · g5.host.cache-door]

Each Host runs one cache door that its VMs reach by a fixed alias. It serves verified artifacts and trusted compiled objects from a bounded, least-recently-used store on the host disk, filling misses from the organization's fleet tier or the public upstream; the store dies with the host.  

<a id="g5.host.cache-trust"></a>
[I · g5.host.cache-trust]

A cache changes build time, never results. Verified artifacts are checked by their consumer, so a cache entry can only fail, never substitute; compiled objects run unverified, so they come only from trusted producers, and a guest's own objects never leave its VM.  

<a id="g5.host.cache-fleet"></a>
[I · g5.host.cache-fleet]

Across hosts, warmth comes from a per-organization fleet tier of base image tiers and compiled objects, written only by the deployment's producer and read with a credential scoped to that organization.  

<a id="g5.host.image-tiers"></a>
[I · g5.host.image-tiers]

Base image tiers are prebuilt images keyed on their recipe inputs (Dockerfiles, lockfiles, build configuration, toolchain), never on source files; a VM pulls a published key through the door and builds an unpublished one locally.

<a id="g5.host.isolation"></a>
[I · g5.host.isolation]

Workspace execution uses the isolated [Cloud Hypervisor microVM](#g5.architecture.microvm_backend); backend failure never falls back to an unisolated process.

<a id="g5.host.guest-image"></a>
[I · g5.host.guest-image]

The customer OCI image supplies the guest filesystem and development environment.  

<a id="g5.host.image-selection"></a>
[I · g5.host.image-selection]

Image selection uses committed code-repository pins, falling back to the organization default when no pin exists.  

<a id="g5.host.immutable-image"></a>
[I · g5.host.immutable-image]

Image pins must identify immutable digests; malformed, mutable or conflicting pins are rejected.  

<a id="g5.host.boot-spec"></a>
[I · g5.host.boot-spec]

Each VM incarnation records its selected image and immutable boot specification.  

<a id="g5.host.storage"></a>
[I · g5.host.storage]

Each VM receives its own writable filesystem and workspace storage.  

<a id="g5.host.runtime-bundle"></a>
[I · g5.host.runtime-bundle]

G5 supplies a separate read-only runtime bundle containing its dispatcher, provider CLIs, libraries, pinned module catalog and, when the building host has one, the pinned platform (VMM, kernel, initramfs, kernel configuration, passt) an [inner host](#g5.architecture.microvm_backend.nested) runs.

<a id="g5.host.compile-cache"></a>
[I · g5.host.compile-cache]

The guest image ships a baked compile cache for the dependency graph; cargo builds in the VM read further objects through the host's cache door, hitting the organization's own crates only at commits a trusted producer has built.

<a id="g5.host.readiness"></a>
[I · g5.host.readiness]

Missing or incompatible runtime components prevent readiness.

<a id="g5.host.tool-path"></a>
[I · g5.host.tool-path]

Bundled tools supplement the image’s command path; image-provided tools take precedence.  

<a id="g5.host.environment"></a>
[I · g5.host.environment]

Image environment variables reach agents, terminals and previews, subject to platform-owned key restrictions.  

<a id="g5.host.checkout"></a>
[I · g5.host.checkout]

Repository checkout and recovery follow the exact versions selected in the boot specification.  

## Runtime

<a id="g5.runtime"></a>
[I · g5.runtime]

The runtime is the host-side and control-plane machinery that places, boots, leases, reclaims and caches workspace VMs.

<a id="g5.runtime.placement.sizing"></a>
[D · g5.runtime.placement.sizing]

At startup the host divides its measured CPU and RAM budget equally among the configured maximum VM count, using integer division.

- Reserve RAM for the host: at least 1536 MiB or ten percent of measured RAM, capped at half of RAM.
- Reject unsupported or impossible sizes before registration rather than oversubscribe or substitute fixed defaults.
- Explicit CPU/RAM overrides stay available within the same aggregate budget.

<a id="g5.runtime.control"></a>
[D · g5.runtime.control]

Host control commands negotiate their versions and are deduplicated by identity.

<a id="g5.runtime.control.versions"></a>
[D · g5.runtime.control.versions]

Negotiate host protocol and VM specification independently, and reject unsupported versions.

<a id="g5.runtime.control.replay"></a>
[D · g5.runtime.control.replay]

Identify each host command by its method, idempotency key and original payload; reusing that identity with a changed payload conflicts.

- An expired admission budget prevents new work; a caller timeout leaves the outcome unknown, so retries keep the same identity.
- Retain completed results across tunnel reconnection for the advertised window. This cache is process-local and need not survive host restart.
- Deduplicate active identities before consuming execution capacity: an identical active retry may return a transient `Busy`.
- Bound distinct executions across control-session reconnects; retries must not fill the budget with waiters.

<a id="g5.runtime.control.tunnel"></a>
[D · g5.runtime.control.tunnel]

Each host reaches the control plane over a single outbound WebSocket tunnel it dials and keeps open, so a host needs no inbound ingress or listening port.

- The tunnel authenticates with a per-host bearer token read fresh on every reconnect; a worker-registration token is refused.
- A keepalive ping and an inbound-liveness timeout detect a half-open carrier, then tear it down and reconnect with bounded backoff.
- On a protocol-version rejection the host self-drains, keeping existing VMs alive and refusing new Starts, while it keeps retrying to recover.

## Custody and host inventory

<a id="g5.runtime.cleanup"></a>
[D · g5.runtime.cleanup]

Host cleanup reaps a released or stale VM incarnation while the host's other work keeps progressing.

<a id="g5.runtime.cleanup.progress"></a>
[D · g5.runtime.cleanup.progress]

The host admits stale-incarnation cleanup in roster-generation order, independently for each held VM.

- A stop that cannot prove exit keeps custody and its cleanup owner, while unrelated reconciliation, reconnect, commands, heartbeats and event replay keep progressing.
- Cancelling the caller cannot abandon admitted cleanup.
- A late completion releases only its original reservation; host restart keeps the existing recovery obligations.

<a id="g5.runtime.lease.renewal"></a>
[D · g5.runtime.lease.renewal]

Only the current live VM incarnation may renew custody.

- On both private host-control and control-plane relay paths, the request's workspace, VM and epoch must match the trusted connection; reject a mismatch with `STALE_INCARNATION` before relaying, issuing a token or changing sequence state.
- Accept replies only for outstanding, increasing requests on the same VM; obsolete, unsolicited or expired replies cannot extend the lease, and repeating a sequence must not extend server expiry.
- Bound guest expiry by the request's send time and credential validity, and publish the credential before advancing custody.
- A renewal judges expiry at the start of its database transaction, before it holds the VM row, and extends the lease from that time; Ready instead locks the VM row first and judges expiry when it promotes the VM.
- A renewal that waited past expiry is harmless: the expiry release re-checks expiry under the same row lock, so whichever commits first wins, and a renewal that finds its incarnation released is answered stale.

<a id="g5.runtime.host.inventory"></a>
[D · g5.runtime.host.inventory]

Reconcile missing current incarnations only from authenticated, complete inventory supplied by the current connection under current host authority.

- A partial heartbeat preserves unreported custody and unknown inventory.
- Remove a reported live incarnation only on matching exit evidence.
- Missing heartbeats block placement but prove neither release nor physical termination.

<a id="g5.runtime.start"></a>
[D · g5.runtime.start]

A Start boots a placed incarnation on a host; settlement decides when it leaves custody.

<a id="g5.runtime.start.settlement"></a>
[D · g5.runtime.start.settlement]

A placed incarnation stays in custody until its Start is settled: the host answered it definitely, or no Start for it can still reach the host. Host absence (`NotFound` from status or Stop, or omission from a complete hello) releases only an incarnation whose every sent Start the host necessarily observed before answering, or whose Start was never sent and is fenced from being sent; otherwise custody waits for the [lease rule](#g5.runtime.lease).

<a id="g5.runtime.start.settlement.formal"></a>
[D · g5.runtime.start.settlement.formal]

The refinements below pin down start settlement's send, host, reply and retraction rules precisely enough to check.

<a id="g5.runtime.start.settlement.formal.send"></a>
[D · g5.runtime.start.settlement.formal.send]

Expands [start settlement](#g5.runtime.start.settlement): the control plane records the send under the incarnation's row lock before enqueuing it, only on a connection whose hello reconciliation completed, and never sends once that fence fails.

<a id="g5.runtime.start.settlement.formal.host"></a>
[D · g5.runtime.start.settlement.formal.host]

Expands [start settlement](#g5.runtime.start.settlement): the host registers each received Start execution before evaluating any later command.

- Until reservation, it reports the incarnation as booting in complete inventory and status while any execution stays pending, including retries whose earlier execution expired or was cancelled.
- A Stop cancels every pending execution before answering absent, and that cancellation holds until the last execution ends.
- Settling an older execution cannot release a later registration, and no new timeout decides absence.

<a id="g5.runtime.start.settlement.formal.replies"></a>
[D · g5.runtime.start.settlement.formal.replies]

Expands [start settlement](#g5.runtime.start.settlement): a delayed definite reply releases only its exact incarnation, requeues only a Start intent still in `starting`, and settles dependent Agents work only when that fenced release matched.

<a id="g5.runtime.start.settlement.formal.retraction"></a>
[D · g5.runtime.start.settlement.formal.retraction]

Expands [start settlement](#g5.runtime.start.settlement): a settled non-admission (refusal, definite boot failure, or absence once every sent Start was observed) retracts the incarnation from the host's reported inventory, since no VM existed to emit exit evidence; a partial heartbeat never retracts.

<a id="g5.runtime.events"></a>
[D · g5.runtime.events]

Journal host VM facts durably across host restart until acknowledged, and keep sequence watermarks after pruning.

- Commit deduplicated facts and their applicable effects on the current incarnation together, then acknowledge only a contiguous stored prefix. Reject conflicting reuse of a sequence.
- Keep stale facts as history under [incarnation fencing](workspace_vm.g5.md#g5.data.identity.incarnation), and preserve acknowledgment progress when cleaning up facts the host may still replay.
- Keep durable event identity stable across supported API and host versions.
- Failed-boot diagnostics do not prove process exit: when cleanup proves exit, the host journals the legacy `BootFailed` then a separately sequenced `Exited`, together in one durable append. This reuses evidence older peers preserve without weakening payload-conflict checks.
- A completed Stop keeps its existing exit-only outcome; unproven cleanup emits no exit fact.
- A temporary host-disk failure keeps its phase and available/required bytes in replies and durable facts, and preserves Start intent through the capacity retry flow only after exit-proven cleanup, even when events precede replies or arrive out of order.
- Image, authorization and source-integrity failures stay distinguishable.

<a id="g5.runtime.module-event-hints"></a>
[D · g5.runtime.module-event-hints]

The host's control connection to a guest dispatcher receives every `module.event`; the host forwards only a content-free, coalesced hint naming the VM and the executions with new events, so the API's [live delivery](../features/chat.g5.md#g5.chat.live-delivery) fetches promptly instead of polling. The hint is neither durable nor acknowledged: the guest journal remains the single source and ordering authority, so a lost or merged hint only delays a fetch to the next poll. A host offers the hint through a hello capability and sends it only after the control plane enabled it in its accept, so a peer from before this change on either side exchanges nothing new.

## Cache lifetime

<a id="g5.runtime.cache"></a>
[D · g5.runtime.cache]

A host's on-disk cache is bounded, accounted, and described to guests by a projected profile.

<a id="g5.runtime.cache.accounting"></a>
[D · g5.runtime.cache.accounting]

Reserve space for temporary cache bytes before writing, and refuse the write if space cannot be made.

- Publication and eviction keep their accounting through caller cancellation.
- Protect each entry from eviction until its reader finishes.
- On host startup, recover temporary leftovers and reconstruct disk usage.

<a id="g5.runtime.cache.profile"></a>
[D · g5.runtime.cache.profile]

A host's cache profile is the non-secret description of its [door](#g5.host.cache-door) and the organization's [fleet tier](#g5.host.cache-fleet), authored by host bring-up.

- Shared images carry none of it: the [runtime bundle](#g5.host.runtime-bundle) ships only an empty placeholder at the guest path.
- Every VM boot, including restart, projects the host's current profile onto that path read-only before hooks, workload or any consumer runs, in both isolation modes, so guest tools select the door without caller injection; an explicit caller setting still wins.
- A host without a profile leaves the placeholder, which consumers read as no door and fall back as before.
- A present profile the runtime cannot receive fails the boot under [runtime closure rejection](#g5.runtime.bundle), never a guest that silently loses its cache.

## Workspace roots and published capacity

<a id="g5.runtime.materialization"></a>
[D · g5.runtime.materialization]

Every Start uses a fresh private workspace root and restores the exact admitted source and [complete Git receipt](git.g5.md#g5.git.flush-proof) before workload readiness.

- Every production microVM restores source directly into its private disk through trusted guest SCM fetch, proof and restore before customer hooks, without a duplicate host checkout or archive.
- An eligible [pristine repository seed](#g5.architecture.workload_repo_seed) may supply initial files and objects; process-mode development uses the host bare cache. Seeds hold members only and never substitute for authoritative ref proof.
- Recoverable local capacity failure anywhere in optional seed preparation, including its authorized repository refresh, falls back to ordinary trusted guest restoration after the owning processes finish cleanup.
- Only local typed I/O or a local write probe establishes host pressure; Git, image and guest text cannot. An opaque optional-cache refresh failure may discard the accelerator without claiming pressure, and the ordinary guest restore still rejects invalid source and proves its authority.
- Explicit authorization and invalid-input refusals stay terminal.
- The host owns root names and paths and never runs Git inside guest-written content. Preserve the host bare cache; no earlier guest-written root or Docker disk becomes a boot or recovery source.
- Reference checkouts — a personal workspace's [checkouts of Base](../features/personal_workspace.g5.md#g5.workspace.identity.reference) and the owner's [personal mount](../features/personal_workspace.g5.md#g5.personal.mount) in any other workspace — materialize on their branch at the boot specification's pinned heads with no receipt, fetched by trusted guest bootstrap over the VM's own token for microVMs and from the host cache in process mode.
- Origin tracks the branch, with `origin/<branch>` fetched at Start, and the dispatcher relays it with the VM's credential like a member's, so ordinary pulls work; the control plane admits what it may read or push.

<a id="g5.runtime.purge"></a>
[D · g5.runtime.purge]

Reclaim roots after lifecycle release, failed boot and host restart, keeping paths held by active or unreaped runtime custody.

- A fresh materialization may retire an obsolete hold on a replaced host checkout path; that does not release a VM slot, port or private disk, which the [VM process owner](#g5.architecture.microvm_backend) keeps until exit is proven.
- Claim an unheld root against a concurrent Start before deleting it asynchronously, so delayed cleanup cannot delete a replacement root.
- Retry failed reclamation under disk pressure and during later cleanup. Deletion need not provide secure erasure.

<a id="g5.runtime.placement.sizing-publication"></a>
[D · g5.runtime.placement.sizing-publication]

Publish resolved slot resources before accepting placement, and use them in VM boot specifications. Keep those values stable while the host holds VM custody. A compatible older host may continue custody and control service without complete sizing information, but must publish it before receiving new placement.

## Placement and image assembly

<a id="g5.runtime.placement"></a>
[D · g5.runtime.placement]

Place a VM within its organization and requested region on an eligible, ready host with a free slot.

- Prefer hosts with running VMs, then greater starting/running/stopping occupancy, then age and stable host identity.
- Exclude hosts refused for that workspace; a transient admission or control refusal imposes a 30-second cooldown.
- The host makes the final admission decision, subject to [published capacity](#g5.runtime.placement.sizing-publication) and [quota serialization](host_control_plane.g5.md#g5.runtime.placement.capacity).
- Do not lend capacity across organizations or introduce a host task scheduler.

<a id="g5.runtime.boot"></a>
[D · g5.runtime.boot]

Booting a VM assembles its root image and brings up its execution backend.

<a id="g5.runtime.boot.concurrency"></a>
[D · g5.runtime.boot.concurrency]

A host runs at most two workspace VM boots at once and refuses a further Start with `BootCapacity`, even when slots are free. A boot that has abandoned its VM and waits only for proof of that VM's exit no longer counts: the VM keeps its slot, port, workspace exclusion and inventory under [custody](#g5.runtime.purge) until exit is proven, but other Starts may boot.

<a id="g5.runtime.boot.image"></a>
[D · g5.runtime.boot.image]

A boot image is assembled from recorded OCI layers.

<a id="g5.runtime.boot.image.rootfs"></a>
[D · g5.runtime.boot.image.rootfs]

Apply recorded layer content and explicit directory metadata in layer order, including layers that change only a directory's mode.

- An implied parent directory supplies no new mode, so preserve its lower-layer mode.
- Do not preserve ownership, and strip setid bits when assembling under a non-root daemon.
- Advance the template format when assembly semantics change, rebuilding old roots from cached layers. This keeps the development image's warm registry and compile cache writable by uid 1000 when a workspace needs newer dependencies than the baked lockfile.

## Development preview

<a id="g5.runtime.boot.backend"></a>
[D · g5.runtime.boot.backend]

The execution backend runs a VM as a production microVM or, for local development, as a host process.

<a id="g5.runtime.boot.backend.process-local"></a>
[D · g5.runtime.boot.backend.process-local]

Under the provisional [development-only process-mode scope](#g5.runtime.local-mode-scope), a full-stack preview builds its own host first using available cores, separate from the developer's stack.

- Start the stack without its edge, confirm ready host registration through the API, then publish the edge from the same stack phase without recreating the API; remove any prior edge while replacing the stack.
- Give the preview a Compose identity and host of its own, and stop that host on Close.
- The `full-stack` profile runs that host as a microVM inside a nested-enabled Linux guest ([inner hosts](#g5.architecture.microvm_backend.nested)): it proves KVM, the runtime disk's platform and the guest image's identities, mounts a loop-backed XFS root outside the archived workspace tree as the host directory, and stages the inner guest runtime once per VM through the canonical `worker-host-setup`.
- That profile fails with the reason and the name of its `full-stack-process` sibling rather than falling back. Only the host process and the signals that stop or reap it cross the root boundary; builds, Docker, the staged runtime and every file under the checkout stay the agent user's.
- The `full-stack-process` profile is the explicit process-mode stack for macOS, ARM and hosts with the nested switch off. `full-stack-no-worker` omits the host.
- The previewed API trusts the [preview origin](workspace_vm.g5.md#g5.workspace.preview-origin-security) as a browser origin and names its refresh cookie outside the `g5_` names the preview relay withholds, so [cookie refresh](../features/users.g5.md#g5.security.login.refresh-exchange) and organization switching work from the preview.
- `frontend-only` runs the real client against its in-browser mock API with no API stack; the mock follows the dev stack's default features and layout and answers the organization Home with demo data, so a preview shows the shipped UI rather than empty or failed tiles. Unit tests keep the fixture's own defaults.
- A process-mode host may copy a trusted bare cache only when reflink reports unsupported; other errors stay failures, and microVM hosts require reflink.
- Every mode follows [fresh-root materialization](#g5.runtime.materialization); none permits fallback after production microVM isolation fails.

## Open questions

<a id="g5.runtime.local-mode-scope"></a>
[Q · g5.runtime.local-mode-scope]

Should the microVM isolation rule admit an explicitly selected and acknowledged local development process mode? It is implemented without automatic fallback, but selection alone does not restrict the host to localhost. Provisionally keep production execution microVM-only and treat process mode as development without production isolation assurance.

## VM execution and storage

<a id="g5.architecture.microvm_backend"></a>
[D · g5.architecture.microvm_backend]

Linux workspace VMs use Cloud Hypervisor with block-backed workload storage and virtio-net through a supervised passt vhost-user sidecar.

- Preserve the existing dispatcher, ingress, host-door and egress boundaries. Guest control-plane access uses passt DNS and the allowlisted host door for local endpoints; no TSI relay remains.
- The NIC backend is not selectable: no host, deployment or control-plane setting chooses a guest network mode, and the `VmSpec` network value is the constant `passt`.
- The supervisor owns both processes, and the host keeps [custody](workspace_vm.g5.md#g5.vm.physical-uniqueness-question) until their exit is proven. A backend failure cannot select another backend.
- Local macOS development uses the [explicit process mode](#g5.runtime.boot.backend.process-local).
- Remove the replaced libkrun, TSI and virtio-fs workload mechanisms rather than maintain two VM architectures.
- The platform supplies a pinned kernel and minimal trusted init; replacing the VMM does not relax Docker, filesystem, network or recovery requirements.
- Neither deprivileged child inherits host, provider or workspace credentials through the supervisor environment. Loopback host aliases use the allowlisted host door.

<a id="g5.architecture.microvm_backend.git_transport"></a>
[D · g5.architecture.microvm_backend.git_transport]

Refining the [microVM backend](#g5.architecture.microvm_backend): a guest's Git transport reaches a loopback layout origin through the host door only as the relay's upstream, and that upstream view is what the exact-route check admits.

- The persisted layout and each checkout's origin keep the admitted URL, which native Git routing keys on, so ordinary `git push` and `git fetch` in the checkout reach the relay.
- A process-mode host and a non-loopback origin have no door view: their upstream is the admitted URL itself.
- Self-hosted Linux uses the same systemd supervision and scoped cgroup delegation as managed hosts; its immutable platform worker OCI supplies a read-only service root and library closure, while customer environments stay separate VM inputs.
- Preserve passt confinement and the restricted supervisor capabilities without granting `SYS_ADMIN` or disabling AppArmor to accommodate a container wrapper.

<a id="g5.architecture.microvm_backend.nested"></a>
[D · g5.architecture.microvm_backend.nested]

Refining the [microVM backend](#g5.architecture.microvm_backend): a Linux host exposes hardware virtualization to its guests only when its deployment switch is on (`G5D_NESTED_VIRTUALIZATION`, from the API's `G5_WORKER_NESTED_VIRTUALIZATION`; default off). The switch is a host property, not part of the VM specification.

- With the switch on, the VMM launches with `nested=on`, the platform kernel has KVM built in, and trusted init makes `/dev/kvm` world-usable (`0666`) so the workload user and `--device` containers can open it and boot their own KVM guests. With it off, the guest shows no VMX flag and no `/dev/kvm`.
- The host publishes `nested-virtualization:v1` on hello when on and records the state in each boot summary. A host with the switch on refuses to start unless its platform kernel declares KVM, so the kernel ships before the switch is enabled.
- Exposure changes nothing else: in-guest backend selection still requires the platform artifacts, and passt, Landlock, runtime and bootstrap boundaries are unchanged. The security reason for the default is in [g5.security.nested-virtualization](../concepts/security.g5.md#g5.security.nested-virtualization).
- Inner hosts: the [runtime bundle](#g5.host.runtime-bundle) carries the host's own pinned VMM, kernel, initramfs, kernel configuration and passt under `/opt/g5/platform`, staged from the same files the host runs, so a deploy updates both together and no second pin exists.
- An in-guest `g5d host` in a nested-enabled guest resolves its launch artifacts there and runs the unmodified launch: Landlock, the `g5-vmm` and `g5-passt` identities, the nftables egress policy, the read-only runtime and one-shot bootstrap.
- For that the guest kernel declares Landlock as its LSM, the `inet` nf_tables family with the `ct` and `fib` expressions the policy uses, and loop devices with XFS, because a microVM host requires reflinks while the guest's ext4 workload disk cannot; the inner host's directory is a loop-mounted XFS image.
- A guest image that hosts an inner host supplies the two identities, the `kvm` group, `nft` and `mkfs.xfs`, and builds no kernel and no passt.
- Inner hosts are a development path (the full-stack preview); the control plane never places workspaces on them, and a guest without KVM or without the platform fails that preview explicitly rather than falling back to process mode.

<a id="g5.architecture.build_scratch"></a>
[D · g5.architecture.build_scratch]

For Linux microVM boots, place the complete workload on one private, ephemeral ext4 block filesystem within the published scratch capacity, preserving guest paths, creation permissions, isolation and the independent read-only [runtime bundle](#g5.host.runtime-bundle).

- Complete workspace ownership handoff before admitting repository writers; a denied entry on the private filesystem fails startup rather than inherit the retired macOS shared-filesystem permission exception.
- Prepare a pristine image before guest ownership, and finish fresh workspace import and per-VM configuration during bootstrap before hooks, workload execution or readiness.
- Bootstrap is one-shot per private disk: a guest reboot must fail and exit rather than re-import source or replay receipt restoration over live workspace changes.
- Trusted recovery runs under a platform-owned temporary root with the independent read-only runtime, so image loaders and system configuration cannot execute before recovery; only then may children enter the customer workload root.
- The workload root is the guest mount-namespace root (the init pivots the guest disk onto it and stacks the workload view there); children enter it by joining the init's mount namespace, never by chroot alone, so `docker exec`, Compose health checks and `nsenter -m` resolve to the workload and its containers rather than the empty initramfs.
- Only pristine environment templates and [authorized repository seeds](#g5.architecture.workload_repo_seed) may be shared; each guest writes a private sparse reflink clone whose physical allocation grows only with its own writes, the host template owner bounds cached variants, and the host never mounts a guest-written filesystem.
- For an admitted workspace selecting another environment or capacity, prepare and verify the matching pristine template under the same exclusive cache lock; existing private clones stay independent.
- Start admission is the [VM disk rule](host_control_plane.g5.md#g5.runtime.placement.vm-disk): cold preparation, archive staging and bootstrap add no further free-space margin, and cold preparation is pinned in a fresh owned root under bounded, exit-proven custody before the dispatcher timer starts.
- Existing [fresh materialization](#g5.runtime.materialization), [complete Git receipts](git.g5.md#g5.git.flush-proof) and [exit-proven disposal](#g5.runtime.purge) stay authoritative; no guest image is retained for reuse or recovery, and unsupported boot modes or preparation failures are rejected rather than silently falling back.
- Preserve guest flush semantics; Git recovery does not justify ignoring a live guest's flushes. Qualify startup, workflow/recovery, full-slot capacity and block flush behavior before widening rollout; disk sizing and custody keep their existing owners.

<a id="g5.architecture.workload_repo_seed"></a>
[D · g5.architecture.workload_repo_seed]

A repository seed is an optional, rebuildable accelerator, never a saved workspace. The [generic delivery path](../features/operations.g5.md#g5.scope.deployment.workload_templates) must work without customer repository access at deploy time or any repository seed.

- The authorized host may begin pristine seed preparation as soon as scoped source access and the selected environment are available, independently of platform deploy and without waiting for VM Ready.
- For a known organization repository selection, the control plane supplies a bounded seed manifest and host-bound read-only source authorization during host bootstrap, before any workspace Start. Derive the selection from existing authorized membership, including docs; org ownership alone neither fixes every workspace's member set nor grants Git access.
- Preserve host-control credential scope and keep provider credentials out of provisioning metadata. Missing or invalid source assignments leave generic startup available.
- Keep preparation bounded and subordinate to foreground startup; cancel or defer it on authorization loss or resource pressure, and never make a first Start wait for an optional seed.
- A seed hit restores inside trusted guest bootstrap, avoiding a full host checkout, import and rsync comparison; a miss keeps the generic path, as does a seed the host finds unusable before attaching it (inconsistent identity, or a missing or incomplete disk pair). Errors the generic path shares still fail the Start.
- Bind the seed to the resolved environment, capacity, layout and exact authorized organization and source membership, with every seeded member exactly matching an authorized repository ID, root and Git endpoint; additional workspace members are freshly imported. Never attach its disk to a nonmatching workspace, because deleting files does not erase raw blocks.
- Build only from pinned committed sources, without credentials or guest-written state. Before attachment, require each baseline commit in the independently authorized repository object store refreshed under current read authority; an absent baseline is a generic miss, and workspace lifecycle counters do not version repository source.
- Keep [fresh materialization](#g5.runtime.materialization) and complete receipt authority unchanged: trusted bootstrap uses shared typed SCM restoration to fetch required authoritative refs, reuse cached objects and restore the exact admitted branch, HEAD and snapshot before hooks or readiness, including deletions and Git metadata.
- A temporary bootstrap negotiation ref may expose the verified seed HEAD to Git transport; remove it before successful restoration returns. It grants no source authority and never changes customer branches or the immutable seed.
- Never merge or rebase a newer remote tip automatically. Publish the immutable disk and its committed baseline manifest together and refresh only from authorized pristine inputs, without blocking an eligible Start or promoting a workspace disk. Bound the owner to one generic and one repository variant with staging and full-slot capacity accounted for; a mismatch keeps the generic path, a failed import cannot become Ready, and seeds enable for rollout only after a demonstrated startup benefit and qualification of the affected workspace flows.

<a id="g5.runtime.bundle"></a>
[D · g5.runtime.bundle]

Refining the [independent read-only runtime](#g5.host.runtime-bundle), preserve executable
identity and self-reexec for platform CLI, Git and module supervision. Reject missing or
incompatible runtime/catalog closure before readiness; no guest-copy fallback.

<a id="g5.runtime.boot.net-resume"></a>
[D · g5.runtime.boot.net-resume]

The selected virtio-net backend must resume receiving after the guest refills an exhausted ring, including signalling a delivered parked frame when no later traffic arrives.

- CPU-heavy builds and bounded queues must not permanently stall a live VM's receive path.
- Validate the selected VMM and network implementation; larger buffers or watchdogs do not replace correct notification.
- Vhost-user replaces the old libkrun socketpair sizing mechanism.

<a id="g5.runtime.lease"></a>
[D · g5.runtime.lease]

Current-row lease, owner and incarnation checks fence authority; inherited guest credentials do not renew an expired server lease.

- A delayed Ready event cannot promote an expired lease.
- The first dispatcher connection requests renewal promptly after its required Ready frame; only an accepted renewal extends authority.
- [Renewal sequencing](#g5.runtime.lease.renewal) governs retries and credential publication.

<a id="g5.runtime.guest-call-admission"></a>
[D · g5.runtime.guest-call-admission]

A guest call is admitted by the current-row lease, owner and incarnation checks — and, for owner calls, current membership activity — in a transaction that commits before the guest is asked, so the call holds no database connection while the guest answers and a guest that stops answering pins nothing but its own stream.

- The API counts each in-flight call per workspace, in process, from the moment its admission is decided under the workspace row's share lock.
- A stop or ownership transfer waits for that count to drain before it takes the workspace row and again under that row, where every admitted call is counted and a call still waiting for the row is not, so no owner call interleaves with its stop admission or the move of ownership and the wait never includes a call the row itself blocks.
- The wait is bounded by the calls' own budgets: a call still counted past its deadline has outlived its budget, and the stop or transfer refuses rather than proceed beside it.
- Methods the guest serializes behind its lifecycle gate (admission open and close, flush) are not counted, since their admission never held the fence; preview writers, which take the incarnation row only for lock order, do not wait.
- The count is process-local, which the one-replica API deployment allows.

## Metadata
### g5.host

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule; it names what the `g5.host.*` blocks below describe.

### g5.host.tenancy

- human-provided
- Source: [Ben, line 117](../../../docs/ben_new_ontology.md#L117).

### g5.host.lifecycle

- human-provided
- Source: [Ben, line 118](../../../docs/ben_new_ontology.md#L118).

### g5.host.kind

- human-provided
- Source: [Ben, line 119](../../../docs/ben_new_ontology.md#L119).

### g5.host.cloud

- human-provided
- Source: [Ben, line 120](../../../docs/ben_new_ontology.md#L120).

### g5.host.macos

- human-provided
- Source: [Ben, line 121](../../../docs/ben_new_ontology.md#L121).

### g5.host.drain

- human-provided
- Source: [Ben, line 122](../../../docs/ben_new_ontology.md#L122).

### g5.host.telemetry

- human-provided
- Source: [Ben, line 123](../../../docs/ben_new_ontology.md#L123).

### g5.host.slots

- human-provided
- Source: [Ben, line 124](../../../docs/ben_new_ontology.md#L124).

### g5.host.vpc

- human-provided
- Source: [Ben, line 125](../../../docs/ben_new_ontology.md#L125).

### g5.host.start

- human-provided
- Source: [Ben, line 126](../../../docs/ben_new_ontology.md#L126).

### g5.host.stop

- human-provided
- Source: [Ben, line 127](../../../docs/ben_new_ontology.md#L127).

### g5.host.status

- human-provided
- Source: [Ben, line 128](../../../docs/ben_new_ontology.md#L128).

### g5.host.heartbeat

- human-provided
- Source: [Ben, line 129](../../../docs/ben_new_ontology.md#L129).

### g5.host.image

- human-provided
- Source: [Ben, line 130](../../../docs/ben_new_ontology.md#L130).

### g5.host.cache-door

- human-provided
- Source: [Ben, line 131](../../../docs/ben_new_ontology.md#L131).
- [CacheAccounting](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/CacheAccounting.tla) checked bounded space accounting, reader leases and publication through cancellation. Startup recovery and real byte arithmetic were inspected but not modeled; see [runtime evidence](../../uplift/ben-ontology/investigations/runtime.md).

### g5.host.cache-trust

- human-provided
- Source: [Ben, line 132](../../../docs/ben_new_ontology.md#L132).
- [CacheTrust](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/CacheTrust.tla) checked refusal of guest exports and use of verified artifacts or trusted compiled objects. Artifact verification and producer identity are premises, not a supply-chain proof; see [runtime evidence](../../uplift/ben-ontology/investigations/runtime.md).

### g5.host.cache-fleet

- human-provided
- Source: [Ben, line 133](../../../docs/ben_new_ontology.md#L133).
- [CacheCredentialScope](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/CacheCredentialScope.tla) distinguished scoped facade paths from broader QA credential grants, exposing RUNTIME-006. It did not demonstrate token exfiltration or test deployed IAM; see [runtime evidence](../../uplift/ben-ontology/investigations/runtime.md).

### g5.host.image-tiers

- human-provided
- Source: [Ben, line 134](../../../docs/ben_new_ontology.md#L134).

### g5.host.isolation

- human-provided; human-verified amendment
- Source: [Ben, line 135](../../../docs/ben_new_ontology.md#L135). The user's subsequent explicit Cloud Hypervisor replacement request supersedes the named libkrun mechanism, preserving VM isolation and no fallback; see the [confirmed design](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#final-design-for-confirmation--2026-10-14).

### g5.host.guest-image

- human-provided
- Source: [Ben, line 136](../../../docs/ben_new_ontology.md#L136).

### g5.host.image-selection

- human-provided
- Source: [Ben, line 137](../../../docs/ben_new_ontology.md#L137).
- [BootContract](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/BootContract.tla) checked committed-pin precedence and default selection only when pins are absent. It did not prove running-guest readiness; see [runtime evidence](../../uplift/ben-ontology/investigations/runtime.md).

### g5.host.immutable-image

- human-provided
- Source: [Ben, line 138](../../../docs/ben_new_ontology.md#L138).
- [BootContract](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/BootContract.tla) checked rejection of invalid pins and incompatible contracts. Artifact verification is an implementation premise; see [runtime evidence](../../uplift/ben-ontology/investigations/runtime.md).

### g5.host.boot-spec

- human-provided
- Source: [Ben, line 139](../../../docs/ben_new_ontology.md#L139).

### g5.host.storage

- human-provided
- Source: [Ben, line 140](../../../docs/ben_new_ontology.md#L140).

### g5.host.runtime-bundle

- human-provided
- Source: [Ben, line 141](../../../docs/ben_new_ontology.md#L141).

### g5.host.compile-cache

- human-provided
- Source: [Ben, line 142](../../../docs/ben_new_ontology.md#L142).

### g5.host.readiness

- human-provided
- Source: [Ben, line 143](../../../docs/ben_new_ontology.md#L143).

### g5.host.tool-path

- human-provided
- Source: [Ben, line 144](../../../docs/ben_new_ontology.md#L144).

### g5.host.environment

- human-provided
- Source: [Ben, line 145](../../../docs/ben_new_ontology.md#L145).

### g5.host.checkout

- human-provided
- Source: [Ben, line 146](../../../docs/ben_new_ontology.md#L146).

### g5.runtime

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule; it names the runtime machinery the `g5.runtime.*` blocks refine.

### g5.runtime.placement.sizing

- human-provided
- Source: [Ben, line 147](../../../docs/ben_new_ontology.md#L147).

### g5.runtime.control

- agent
- Expands: [runtime](#g5.runtime).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [versions](#g5.runtime.control.versions), [replay](#g5.runtime.control.replay), [tunnel](#g5.runtime.control.tunnel).

### g5.runtime.control.versions

- human-provided
- Expands: [host control](#g5.runtime.control).
- Source: [Ben, line 148](../../../docs/ben_new_ontology.md#L148).

### g5.runtime.control.replay

- Active-identity admission and the process-wide execution bound follow [issue #626](https://github.com/G5Labs/G5-Lean/issues/626) and its [continued design](../../pr_ontology_diff_design/issue-626/ontology-plan.md#host-progress-follow-up-issue-626-october-22).

- agent
- Source: [RT-P01](../../uplift/ben-ontology/quantitative/runtime.md#rt-p01--g5runtimecontrolreplay-high), recovered `67ad4b5d` contract and [host protocol §3.2](../../../docs/worker-host-api-v9.md#32-commands-control-plane--host).
- [CommandReplay](../../uplift/ben-ontology/quantitative-refinement/runtime.md) checked per-key exclusion, payload binding, admission budgets and replay after timeout/disconnect. Its retention window is bounded; restart deliberately loses the process-local cache.

### g5.runtime.control.tunnel

- agent
- Expands: [host control](#g5.runtime.control).
- Inferred from code for the writing-rules [coverage review](https://github.com/G5Labs/G5-Lean/issues/1119); the owner should confirm the intent.
- Implementation: `backend/g5d-host/src/tunnel.rs` (outbound dial, per-reconnect bearer token, keepalive/liveness, backoff, protocol-reject self-drain); `backend/g5d/src/cli/host.rs` (host-token requirement, worker-registration token refused).

### g5.runtime.cleanup

- agent
- Expands: [runtime](#g5.runtime).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [progress](#g5.runtime.cleanup.progress).

### g5.runtime.cleanup.progress

- agent
- Expands: [host cleanup](#g5.runtime.cleanup).
- Source: [issue #626](https://github.com/G5Labs/G5-Lean/issues/626) and [continued design](../../pr_ontology_diff_design/issue-626/ontology-plan.md#host-progress-follow-up-issue-626-october-22).
- Refines existing exit-proof custody; runtime tasks are bounded by held VM slots.
- Implemented in `workspace_vm.rs`, `runtime.rs` and `ctl.rs`; new mux/runtime regressions cover unproven stops, replay, caller cancellation, generation retry and late completion. [Local evidence and environment limits](../../pr_ontology_diff_design/issue-626/ontology-plan.md#local-verification-of-the-follow-up).

### g5.runtime.lease.renewal

- agent
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md) and [connection-binding contract, PR #548](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/software_architecture/runtime.g5.md).
- [LeaseRenewal](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/LeaseRenewal.tla) checked sequence rollback, duplicate expiry, identity, send-time bounds and credential publication. Lookup and credential validation are premises; these checks establish neither physical termination nor protection after host-credential compromise.
- Refines: [VM uniqueness](workspace_vm.g5.md#g5.vm.uniqueness), [incarnation identity](workspace_vm.g5.md#g5.data.identity.incarnation), [host heartbeat](#g5.host.heartbeat).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 50 (L1A-Q2).

### g5.runtime.host.inventory

- agent
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md), complete Hello, partial heartbeat and fenced exit/release paths.
- [HostInventory](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/HostInventory.tla) checked inventory completeness and current-connection authority. It establishes neither physical exit nor deployed transport behavior.
- Refines: [heartbeat](#g5.host.heartbeat), [drain](#g5.host.drain), [VM uniqueness](workspace_vm.g5.md#g5.vm.uniqueness), [incarnation identity](workspace_vm.g5.md#g5.data.identity.incarnation).

### g5.runtime.start

- agent
- Expands: [runtime](#g5.runtime).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [settlement](#g5.runtime.start.settlement).

### g5.runtime.start.settlement

- agent
- Expands: [Start](#g5.runtime.start).
- Source: [G5-Lean#626](https://github.com/G5Labs/G5-Lean/issues/626) findings D2/H1 (release on absence while a Start could still land), D5 (refusal revived cancelled intent), S26 (settlement after a missed fence); [design](../../pr_ontology_diff_design/issue-626/ontology-plan.md).
- Implemented: `workspace_vms.start_command_sent_at` (migration 097) is stamped by `resume_with` under the row lock before `HostCommandClient::dispatch` enqueues the Start; `workspace_vm_release.rs` fences every unadmitted-start release on that stamp and retracts the id from `worker_hosts.reported_live_vm_ids` on a settled non-admission (host row locked first, in placement's and hello's order); the tunnel registry publishes a command client after `accept_hello`; `g5d-host` keeps `pending_starts` from request dequeue to execution end and answers status, complete-hello inventory and stop from it. Verified by API tests against the scripted host and host unit tests, including the barrier-controlled concurrent-Stop `Busy` interleaving; no Linux/KVM run for this change.
- Refines: [host inventory](#g5.runtime.host.inventory), [command replay](#g5.runtime.control.replay), [lease](#g5.runtime.lease), [Start consent](workspace_vm.g5.md#g5.lifecycle.start), [placement capacity](host_control_plane.g5.md#g5.runtime.placement.capacity).

### g5.runtime.start.settlement.formal

- agent
- Expands: [start settlement](#g5.runtime.start.settlement).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule; it groups the formal send, host, reply and retraction refinements.
- Children: [send](#g5.runtime.start.settlement.formal.send), [host](#g5.runtime.start.settlement.formal.host), [replies](#g5.runtime.start.settlement.formal.replies), [retraction](#g5.runtime.start.settlement.formal.retraction).

### g5.runtime.start.settlement.formal.send

- agent
- Source: [G5-Lean#626](https://github.com/G5Labs/G5-Lean/issues/626) D2/H1; [design](../../pr_ontology_diff_design/issue-626/ontology-plan.md).
- Implemented in `workspace_vm_service.rs::resume_with` (stamp and `HostCommandClient::dispatch` in one row-locked transaction) and `preview_tunnel.rs` (client published after `accept_hello`, under the publication guard).
- Expands: [g5.runtime.start.settlement](#g5.runtime.start.settlement) (formal).

### g5.runtime.start.settlement.formal.host

- agent
- Source: [G5-Lean#626](https://github.com/G5Labs/G5-Lean/issues/626) D2/H1 and S11; [design](../../pr_ontology_diff_design/issue-626/ontology-plan.md).
- Retry lifetime and cancellation: [G5-Lean#999](https://github.com/G5Labs/G5-Lean/issues/999); [lifecycle regressions](../../../backend/g5d-host/src/qualification/lifecycle.rs).
- Implemented in `g5d-host` (`pending_starts` registered at request dequeue in `ctl.rs`, answered by `status`, hello `inventory` and `stop` in `workspace_vm.rs`); the partial heartbeat lists only active records.
- Expands: [g5.runtime.start.settlement](#g5.runtime.start.settlement) (formal).

### g5.runtime.start.settlement.formal.replies

- agent
- Source: [G5-Lean#626](https://github.com/G5Labs/G5-Lean/issues/626) D5 and S26; [design](../../pr_ontology_diff_design/issue-626/ontology-plan.md).
- Implemented by the fenced releases in `workspace_vm_release.rs`; `a_late_refusal_does_not_revive_a_cancelled_start_intent` covers the intent and fence rules.
- Expands: [g5.runtime.start.settlement](#g5.runtime.start.settlement) (formal).

### g5.runtime.start.settlement.formal.retraction

- agent
- Source: independent review of [PR #634](https://github.com/G5Labs/G5-Lean/pull/634) (phantom reported occupancy after a settled pending Start); [design](../../pr_ontology_diff_design/issue-626/ontology-plan.md).
- Implemented by `retract_unadmitted_inventory` in `workspace_vm_release.rs`, run after a matched settled release with the host row locked first; covered by the two retraction tests in `api_worker_hosts_stale`.
- Expands: [g5.runtime.start.settlement](#g5.runtime.start.settlement) (formal).

### g5.runtime.events

- agent
- Source: [RT-P02](../../uplift/ben-ontology/quantitative/runtime.md#rt-p02--g5runtimeevents-high), recovered `67ad4b5d` contract and [host protocol §3.3](../../../docs/worker-host-api-v9.md#33-messages-host--control-plane).
- Mixed-version failed-boot evidence: [PR #637 blocker](https://github.com/G5Labs/G5-Lean/pull/637#issuecomment-6280438603) and [recovery design](../../pr_ontology_diff_design/issue-627/ontology-plan.md). Reuse of the existing exit event is an agent design decision; diagnostics and proven inventory release remain distinct.
- [HostEvents, EventConflict and TerminalFact](../../uplift/ben-ontology/quantitative-refinement/runtime.md) checked durable replay, atomic ingestion, sequence conflicts and selected recovery paths, exposing RUNTIME-EVENT-DELETE and RUNTIME-EXIT-FACT. The repair comparisons are unimplemented, and recovery of a lost terminal receipt remains unresolved.

### g5.runtime.module-event-hints

- agent
- Source: [#966 design](../../pr_ontology_diff_design/issue-966/ontology-plan.md#module-event-hint-host-and-guest-control-stream); `vm.module_events` in `g5-internal-api/src/host_ctl.rs`, forwarded by `g5d-host/src/ctl.rs`, consumed by the attempt follower.

### g5.runtime.cache

- agent
- Expands: [runtime](#g5.runtime).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [accounting](#g5.runtime.cache.accounting), [profile](#g5.runtime.cache.profile).

### g5.runtime.cache.accounting

- agent
- Expands: [host cache](#g5.runtime.cache).
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md).
- CacheAccounting checked bounded accounting, reader protection and publication through cancellation. Startup reconstruction was inspected but not modeled; real byte arithmetic and same-key replacement remain outside the check.

### g5.runtime.cache.profile

- agent
- Source: [issue #918](https://github.com/G5Labs/G5-Lean/issues/918) (QA, 2026-10-28: guests on a bootstrapped host had no `/opt/g5/cache-profile.env`; the runtime disk is frozen at image build and attached read-only, so the host's later write never reached guests). Design in [issue-918-cache-profile-projection](../../pr_ontology_diff_design/issue-918-cache-profile-projection/ontology-plan.md), correcting the [cache sharing design](../../pr_ontology_diff_design/cross-vm-cache-sharing/ontology-plan.md), which assumed guests mount the runtime directory.
- Implemented: `g5d vm-exec` stages the host's profile onto the per-VM bootstrap disk, `scripts/build-runtime-disk.sh` ships the empty placeholder, and `infra/guest/init.c` binds the profile over it before building the guest views. Regressions: `scripts/test-guest-init-mount-roots.py` with and without a profile, `scripts/test-cloud-hypervisor-ci.sh` and `scripts/test-cloud-hypervisor-guest.sh` on real guests (profile byte-for-byte with the fixture runtime; the placeholder with a baked runtime), launcher oracles for the empty placeholder. Fresh-host acceptance (both launchers selecting the door, private OCI reuse, a seeded same-organization fleet hit, stop/restart, disabled cache) is pending QA on a deployed worker image.

### g5.runtime.materialization

- agent
- [Issue #1049](https://github.com/G5Labs/G5-Lean/issues/1049) extends the [incremental-seed design](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#incremental-repository-seed-amendment--2026-10-14) to every production microVM: trusted guest SCM restores source directly into the private disk before customer hooks. Seeds remain optional; process-mode development uses the host bare cache. Linux/KVM seed hit/miss and exact clean/dirty receipt evidence and remaining qualification gaps are recorded in the [disk-policy evidence](../../pr_ontology_diff_design/issue-1049/evidence.md). Prior RootLifetime evidence does not qualify this execution path.
- Source: [retained-roots request](../../chat_intent/retained-roots/request.md), [accepted design](../../pr_ontology_diff_design/retained-roots/ontology-plan.md), commit `3e0b771e`.
- [RootLifetime](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked fresh-root and cleanup ownership. Exact Git restoration and the later [process-mode copy exception](#g5.runtime.boot.backend.process-local) are outside that model's source baseline.
- [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624) (human-verified) adds the personal-workspace reference checkouts; [design](../../pr_ontology_diff_design/issue-624-personal-workspace/ontology-plan.md). The personal mount (2026-10-19, [chat intent](../../chat_intent/issue-624-personal-workspace/request.md)) gives nearly every Start a reference. Trusted guest restoration fetches references after members; the current [Linux/KVM qualification](../../pr_ontology_diff_design/issue-1049/evidence.md) verifies reference repositories and personal mounts with and without a seed.

### g5.runtime.purge

- Current repair: `g5d-host/src/workspace_vm.rs` retains active reservation/exclusion on unproven microVM exit; `g5d-isolation/src/isolation/{microvm,vm_scope}.rs` retains the process owner and retries. Portable delayed-proof regressions pass; final Linux/cgroup/API rerun is recorded in the [current review](../../pr_ontology_diff_design/virtiofs-393/review.md).

- agent
- Source: [retained-roots design](../../pr_ontology_diff_design/retained-roots/ontology-plan.md); Start and cleanup share the runtime-state lock before deletion uses a unique trash name.
- [RootLifetime](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked custody holds and protection from delayed deletion. Reclamation progress requires successful later filesystem operations; logical release does not prove VM death.

### g5.runtime.placement.sizing-publication

- agent
- Source: [equal-resource slot request and design](../../chat_intent/host-equal-resource-slots/resource-sizing.md), Hello validation and placement selection.
- [Placement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked serialized admission, custody counting and refusal of incomplete legacy sizing. It does not verify measured hardware resources or the current packing policy.

### g5.runtime.placement

- agent
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/software_architecture/runtime.g5.md).
- Packing supports guarded idle retirement. The old least-occupied policy is superseded; this does not assert global fairness beyond eligible queued demand.


### g5.runtime.boot

- agent
- Expands: [runtime](#g5.runtime).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [concurrency](#g5.runtime.boot.concurrency), [image](#g5.runtime.boot.image), [backend](#g5.runtime.boot.backend), [net resume](#g5.runtime.boot.net-resume).

### g5.runtime.boot.concurrency

- human-provided
- Expands: [boot](#g5.runtime.boot).
- Reason: the limit only rations bring-up, while custody protects the user's data; two boots stuck on unprovable kills would otherwise refuse every Start on the host.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 51 (DA1-Q1).

### g5.runtime.boot.image

- agent
- Expands: [boot](#g5.runtime.boot).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [rootfs](#g5.runtime.boot.image.rootfs).

### g5.runtime.boot.image.rootfs

- agent
- Expands: [boot image](#g5.runtime.boot.image).
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/bake-permission/request.md).
- Upstream template version 3 and explicit-directory cache markers enforce the repair. Root reserved cache names remain an unsurveyed limitation; non-root later writes into read-only directories can still fail. A fixed Dockerfile alone does not prove the deployed host assembler is current.


### g5.runtime.boot.backend

- agent
- Expands: [boot](#g5.runtime.boot).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [process-local development](#g5.runtime.boot.backend.process-local).

### g5.runtime.boot.backend.process-local

- agent
- Expands: [boot backend](#g5.runtime.boot.backend).
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/software_architecture/runtime.g5.md).
- The scope Q remains unresolved: explicit selection and acknowledgement do not prove a process host is bound to localhost. These operational deltas preserve the implemented development envelope without extending it to production.
- Preview origin trust: `just dev-stack-up` appends `G5D_PREVIEW_ORIGIN` to `G5_CORS_ALLOWED_ORIGINS`, which the API's trusted-origin list includes, and sets `G5_REFRESH_COOKIE_NAME=preview_refresh`; owner choice of 2026-10-26 ([chat intent](../../chat_intent/issue-776/request.md#preview-origin-2026-10-26)). The rename was found when the restarted preview still failed: the relay drops every `g5_` Set-Cookie, so the stack's `g5_refresh` never reached the browser.
- Microvm full-stack preview (2026-10-28): [nested-virt-followup design](../../pr_ontology_diff_design/nested-virt-followup/ontology-plan.md) §3.5; the sibling profile and the explicit failure are the design's choice where the request delegated it; the root boundary and the reflink root are agent proposals pending the owner's confirmation in the question set `nested-virt-followup`. Regressions: `scripts/test-dev-up-preview-profile.sh`, `scripts/test-preview-microvm-host.sh`, `scripts/test-worker-host-env-process-build.sh`.
- Frontend-only demo Home data: the owner asked on 2026-10-23 to preview this workspace's Home changes as the real UI rather than the Project Docs mocks. The full stacks have no issue data without a GitHub App. Implemented in `frontend/src/mocks/{home-preview,fetch-interceptor}.ts`; `npm run test:e2e:mock-preview` drives the profile's own command through Home tiles, search, filters, Link, Create and the narrow-tile header wrap.


### g5.runtime.local-mode-scope

- agent; uncertain
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md), merged PR #487.
- Explicit mode selection does not prove localhost confinement or production isolation. The available lifecycle models do not resolve this question.

### g5.architecture.microvm_backend

- human-verified
- The user requested Cloud Hypervisor, block disks, removal of obsolete libkrun/TSI and dead code, and startup optimization; concrete ownership and network choices are agent decisions in the [final design](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#final-design-for-confirmation--2026-10-14).
- Implementation authorized and present on PR #516. Linux/KVM component checks pass at the scoped candidate revisions; platform journeys pass at the recorded candidate; restricted-service dual-VM operation and whole-service-loss recovery pass. Final release checks remain incomplete. Prototype connectivity does not establish replacement completeness.
- The retired network-mode switch (`G5D_VM_NET`, `G5_WORKER_VM_NET`, the `vm-exec --net` flag and the `tsi` wire alias) is removed; `backend/g5d-isolation/src/isolation/passt.rs` and `cloud_hypervisor.rs` spawn passt unconditionally.

### g5.architecture.microvm_backend.git_transport

- agent
- Expands: [g5.architecture.microvm_backend](#g5.architecture.microvm_backend).
- Source: [G5Labs/G5-Lean#1020](https://github.com/G5Labs/G5-Lean/issues/1020) (2026-10-31: native `git push` in a loopback-layout guest dialled the guest's own loopback because the `insteadOf` key was the door-rewritten URL); the door mapping itself is from the [virtiofs-393 plan](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md).
- Implemented: `backend/g5d-dispatch/src/config.rs` (`door_url`, `transport_url()`, `door_url_for`, exact-route validation on the transport view), `git.rs` (`configure_native_remotes` keys `insteadOf` on `git_url`; `credential_for_url` matches the upstream), leases in `modules/builtin.rs` and `workspace_flush.rs`.
- Tests: `g5d-dispatch` `config::tests::guest_git_transport_maps_loopback_without_changing_membership_or_route_guards`, `git::tests::native_remotes_key_on_the_persisted_origin_and_relay_through_the_door`, `git_transport::tests::a_guest_checkout_reaches_its_door_mapped_upstream_through_the_relay` (ordinary `git` in a checkout with an unreachable loopback origin reaches the door-mapped upstream through the relay).

### g5.architecture.microvm_backend.nested

- human-verified: the owner confirmed the four decisions (switch shape, `CONFIG_KVM_AMD`, the `/dev/kvm` mode, the CI treatment of the nested boot) as recommended and authorized implementation in the Project Docs question set `nested-virtualization` (workspace `003f1792-2438-4c99-b200-53bbd2841ee0`, 2026-10-27); mechanism and reasons are agent decisions in the [nested-virtualization design](../../pr_ontology_diff_design/nested-virtualization/ontology-plan.md).
- Expands: [g5.architecture.microvm_backend](#g5.architecture.microvm_backend).
- Source: the owner's handover `request_investigation.md` in the same Project Docs.
- Implemented (design §6): `G5_WORKER_NESTED_VIRTUALIZATION` → `startup_script.rs` → `G5D_NESTED_VIRTUALIZATION=on` in host.env; `backend/g5d/src/cli/host.rs` (`resolve_nested_virtualization`, `validate_nested_virtualization`, kernel-config guard); `WorkspaceVmRuntimeConfig`/`MicrovmRunSpec.nested_virtualization`; `vm-exec --nested-virtualization` and `cloud_hypervisor::cpus_argument`; `infra/guest/kernel.config` (built-in KVM, Intel and AMD); `infra/guest/init.c` (`/dev/kvm` 0666); `BootSummary.nested_virtualization`; hello capability `nested-virtualization:v1`; QA Helm value on.
- Relied-on facts verified 2026-10-26/27: `--cpus nested=on` works on the QA hosts (Ubuntu 6.8, `kvm_intel nested=Y`); the pinned 6.12.112 source builds with the four settings (full build in this VM, emitted `kernel.config` checked); `GUEST_KERNEL_SHA256` pins the source tarball, so no pin changes.
- Inner hosts (2026-10-28): agent design in [nested-virt-followup](../../pr_ontology_diff_design/nested-virt-followup/ontology-plan.md) from the owner's Project Doc `request.md` (workspace `3927fd16-a974-437f-8185-a653ea04a22d`, option A decided by the owner there); the reflink root, the inner runtime source and the root boundary are agent proposals under the owner's "continue" in the question set `nested-virt-followup`, pending the owner's confirmation. Verified in a workspace VM: the previous guest kernel had no `inet` nf_tables family and no `ct`/`fib` expressions (the egress policy failed at its first line), no Landlock, no loop device and no XFS; the ext4 guest disk cannot reflink. Implemented: `infra/guest/kernel.config` (Landlock as the LSM, `NF_TABLES_INET`, `NFT_CT`, `NFT_FIB_*`, loop, XFS; config-only build of the pinned 6.12.112 emits every setting), `scripts/stage-runtime-platform.sh` from `build-runtime-disk.sh` (`runtime/platform`, digests in `worker-image.json`), discovery and the root boundary in `scripts/worker-host-env.sh` and the worker-host recipes, `scripts/dev-host-reflink-root.sh`, `scripts/preview-microvm-host.sh`, the `full-stack`/`full-stack-process` profiles, the g5dev identities and packages, and the strict KVM probe `test-guest-nested-kvm.py --vm-exec` (Landlock, passt and egress policy, reflink root hard; the third-level `vm-exec` boot report-only unless strict, hard on a Landlock or passt failure). Script regressions pass here. First real KVM runs (g5-ci-rust `b2fc6ef2`, `782a7179`, 2026-10-28): inside the nested-on fixture guest the Landlock ruleset (ABI 6), the passt egress policy, the loop XFS reflink root and a third-level boot through the in-guest `g5d vm-exec` with the platform from the runtime disk all passed (14.3 s); the QA acceptance from a workspace on a nested-enabled host has not run yet.
- Evidence: unit tests for the launch argument, `vm-exec` argv, host env resolution and guard, startup-script render and boot summary pass in this VM (no `/dev/kvm`); clippy clean; the host-crate reflink failures seen here reproduce on the base commit and are environmental (design §6–§7). Strict KVM CI (`test-cloud-hypervisor-ci.sh` with `scripts/test-guest-nested-kvm.py`) hard-checks VMX, a world-usable `/dev/kvm` and Docker device access, keeps a switch-off negative control, and reports the three-deep nested boot (strict behind `G5_NESTED_BOOT_STRICT=1`, because that depth was unreliable on C3 build workers); the E2E lane runs its hosts with the switch on and `host_boot_timings` checks the summary token. First real KVM run (PR #853, g5-ci-rust `77881746`): VMX flag, `/dev/kvm` 0666, unprivileged open with KVM API version 12 and the Docker `--device` container all pass under `g5d`'s exact launch; the report-only nested boot's first attempt failed on a staging permission and was fixed; the rerun (`69ffbe25`) booted the platform kernel as a third-level guest in 6.3 s and passed the switch-off negative control (design §7). Gaps: the AMD path (compiled, not exercised), the E2E lane's switch-on hosts at the final head, and QA acceptance from a workspace VM on a newly provisioned host (design §4.3).

### g5.architecture.build_scratch

- human-verified
- The user requested the complete private workload and later Cloud Hypervisor replacement in the [confirmed design](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#final-design-for-confirmation--2026-10-14).
- Review corrections add one-shot bootstrap, private pre-boot pins and per-Start capacity admission. Local regression checks pass; these corrections still require Linux/KVM qualification.
- [Issue #1049](https://github.com/G5Labs/G5-Lean/issues/1049): private clones stay sparse and Start admission is the [VM disk rule](host_control_plane.g5.md#g5.runtime.placement.vm-disk), without cold-preparation, archive or bootstrap margins, per the [owner's 2026-11-04 correction](../../chat_intent/issue-1049/sparse-disk-correction.md).
- Current implementation: `backend/g5d-isolation/src/isolation/{microvm,cloud_hypervisor,workload_root}.rs`, trusted bootstrap/runtime builders and the existing host materialization/custody owners. Denied ownership repairs fail before repository writers enter the private workload.
- Namespace-root entry (issue #605): `infra/guest/init.c` moves the disk onto the namespace root and binds the workload view over it; `backend/g5d/src/cli/vm_init.rs` joins PID 1's mount namespace with Linux-only calls guarded on other build targets. Strict KVM CI and worker-image gates invoke `scripts/test-guest-docker-exec.sh` inside a candidate guest for actual Docker exec, Compose exec and health checks; full-environment guest acceptance shares those assertions. The mount replay and legacy negative control remain supplementary checks. Real candidate execution remains pending in the [PR #608 follow-up review](../../pr_ontology_diff_design/virtiofs-393/review.md#authorized-repair-follow-up).
- Scoped real KVM trust, read-only runtime and private-write checks pass. Actual platform Files Save, Git, Fork, Stop/Start, Preview and forced-VMM recovery pass on the candidate. Restricted-service launch and whole-service receipt restoration/fencing also pass. Concurrent two-slot byte exhaustion, separate inode exhaustion/recovery and matched host/guest Cargo checks pass at their recorded scope in the [current self-review](../../pr_ontology_diff_design/virtiofs-393/review.md#cloud-hypervisor-implementation-review--2026-10-15). Historical libkrun timings do not qualify this replacement.

### g5.architecture.workload_repo_seed

- human-verified
- The full-history dirty-resume regression exposed a detached seed HEAD omitted from Git negotiation. File-transport checks passed, but real HTTP transport still duplicated history: the negotiation-tip option did not reach the protocol-v0 helper. The scoped amendment uses a temporary bootstrap ref, with HTTP regression and VM requalification required. Exact receipt checks remain unchanged; the rollout qualification requirement still applies.
- The user requested repo seeding and provisioning-time source assignment without customer code at deploy time. The [incremental-seed amendment](../../pr_ontology_diff_design/virtiofs-393/ontology-plan.md#incremental-repository-seed-amendment--2026-10-14) supersedes the historical host-restore/rsync variant (`51b816ee`); its timings do not qualify the replacement.
- Review corrections map partition I/O limits to the whole device, preserve authorization on exact-generation reuse, and prevent unrelated children retaining daemon cache locks. Portable regressions cover the changed predicates and lock lifetime; the managed Linux path remains unexecuted for these fixes.
- Implemented in `g5d-host/src/source_preparation.rs`, the host source grant and `g5d-isolation/src/isolation/workload_seed.rs`. Scoped grants, exact membership, revocation and admitted-boot fallback are covered by API tests. Real KVM clean/dirty multi-member recovery, cross-scope fallback and corrupt-input refusal pass.
- The standalone host publishes a seed before successful workspace Ready, cancels optional preparation for foreground Start, and pins the seed on subsequent Starts. Seeded API/PTY, Files, Stop/Start and whole-service-loss receipt/Preview fencing pass. Interleaved generic/seeded source readiness and small-fixture API startup are measured; two 64-GiB guests pass simultaneous capacity pressure/recovery. Release-profile startup and recovery pass on the small fixture; published-artifact first boot remains a qualification limit in the [current review](../../pr_ontology_diff_design/virtiofs-393/review.md#cloud-hypervisor-implementation-review--2026-10-15).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 52 (L1A-Q4).

### g5.runtime.bundle

- agent
- Refines `g5.host.runtime-bundle` under the confirmed Cloud Hypervisor design. Native ELF interpreter/RPATH pinning preserves self-reexec; real KVM runtime closure, chat/CLI and module reap checks pass. No loader execution from customer image configuration is used for trusted recovery.

### g5.runtime.boot.net-resume

- human-verified; agent mechanism
- Source: issue #407 and the retained [network-liveness design](../../pr_ontology_diff_design/virtio-net-rx-resume/ontology-plan.md). The prior libkrun receive patch and `g5.runtime.boot.net-sndbuf` mechanism are retired by the user's Cloud Hypervisor replacement request; their historical QA results do not qualify vhost-user.
- Cloud Hypervisor packet/isolation gates pass. Three synthetic bursts also resume reception after stopping only the two VMM vCPU threads, without later peer transmissions. This observes guest progress, not internal ring notification correctness or all receive-starvation races; those limits remain in the replacement review.

### g5.runtime.lease

- agent
- Preserves the Cloud Hypervisor slow-boot refinement: Ready checks current lease expiry and the dispatcher queues its initial renewal after Ready. Focused host-control regression and real isolated API/KVM startup/recovery checks pass; production rollout remains unqualified.

### g5.runtime.guest-call-admission

- agent
- architecture-decision
- test-coverage
- Reason: the admission transaction used to stay open through the round trip, so every stuck guest call pinned a pooled connection.
- Source: [follow-up design](../../pr_ontology_diff_design/live-relay-followups/ontology-plan.md#1-a-guest-call-holds-no-database-connection-while-the-guest-answers); `WorkspaceRuntime::call_once` and the in-flight count the stop admission and the transfer drain (`workspace_vm_service.rs`).
- Verified by `api_live_relay::held_guest_calls_pin_no_database_connection` and `a_stop_waits_for_the_incarnations_calls_in_flight`, and by the pull-request incarnation-replacement tests, which now replace the incarnation while a call is held.
