# Workspace VM

<a id="g5.vm"></a>
[I · g5.vm]

A Workspace VM runs one workspace's repositories, dispatcher and workloads under its owner's authority while the workspace is running.

<a id="g5.vm.durability"></a>
[I · g5.vm.durability]

The externally important Workspace VM state is its git repositories.  The VM is expected to commit and push to the central repository state that it wants to persist.  

<a id="g5.vm.setup"></a>
[I · g5.vm.setup]

vm setup includes checking out all the workspace repository branches under appropriate paths  

<a id="g5.vm.flush"></a>
[I · g5.vm.flush]

the workspace VM supports a "flush" mechanism that captures dirty checkout state, persisting it to git in a way that doesn't affect the workspace branch directly and allows a subsequent VM restart to restore that dirty state  

<a id="g5.vm.branch-alignment"></a>
[I · g5.vm.branch-alignment]

if the VM checkout paths change to no longer point at the correct branches, the UI interactions and flush mechanism should produce appropriate errors.  THe VM offers an api call to restore the appropriate branch state.  More complicated misalignment is the responsibility of the user / agent to correct.  

<a id="g5.vm.uniqueness"></a>
[I · g5.vm.uniqueness]

At most one workspace VM holds authoritative custody of a workspace at a time, under [logical uniqueness](#g5.vm.physical-uniqueness-question). A failed VM cannot block launching a replacement once its custody is released.

<a id="g5.vm.physical-uniqueness-question"></a>
[D · g5.vm.physical-uniqueness-question]

[VM uniqueness](#g5.vm.uniqueness) is exclusive authoritative custody, not independently proven physical termination.

- Lease expiry releases an incarnation, and the old VM keeps no authority: its [renewals](hosts.g5.md#g5.runtime.lease.renewal) are refused, its late results are fenced by [incarnation identity](#g5.data.identity.incarnation), and its dispatcher admits no new work once its own lease lapses.
- The host has no lease kill timer, so a partitioned but healthy host never kills VMs by the clock; it stops a VM missing from the control plane's roster when it next reconciles, at reconnect or in a heartbeat reply.
- This holds only while a stop that cannot prove exit never stalls the host's other cleanup or its control session ([cleanup progress](hosts.g5.md#g5.runtime.cleanup.progress)); otherwise released VMs would keep running.

<a id="g5.vm.dispatch-api"></a>
[I · g5.vm.dispatch-api]

The central dispatch process in the VM exposes an API for filesystem access, module invocation and inspection, runtime information and diagnostics.  

<a id="g5.vm.process-isolation"></a>
[I · g5.vm.process-isolation]

the dispatcher forks new processes to service API requests  

<a id="g5.vm.filesystem"></a>
[I · g5.vm.filesystem]

The filesystem API supports list, read, state, write, delete, rename and watch options.  These operations only work on files in a git checkout and do not work on gitignored files.  

<a id="g5.vm.modules"></a>
[I · g5.vm.modules]

Modules are an abstraction for operations on the VM.  The VM explicitly does not support an "execute this command" operation, forcing implementors to build a module (or align with an existing one).  

<a id="g5.vm.module-inventory"></a>
[I · g5.vm.module-inventory]

Modules supported include workspace.bootstrap (start workspace), agent.turn (run an agent turn), workspace.preview (run preview, long-running), workspace.changes (reports repository state and performs structured git operations), and workspace.diagnostics (capture diagnostic bundles and runs fixed read-only probes).  

<a id="g5.vm.catalog"></a>
[I · g5.vm.catalog]

Each VM exposes a versioned catalog of supported modules. Each module declares its typed operations, outcomes, resource limits and requested capabilities in the immutable catalog.  

<a id="g5.security.owner"></a>
[D · g5.security.owner]

Only the current owner with [active organization membership](../concepts/security.g5.md#g5.security.active-membership) and no [execution hold](#g5.lifecycle.stop.by-user) may mutate or use VM files, logs, execution, terminal or preview; administrator status, creation, invitation and past ownership confer no bypass. Transfer is the explicit authority change.

<a id="g5.security.owner.inspection"></a>
[D · g5.security.owner.inspection]

A method-scoped support grant lets an operator read a running VM's bounded, redacted diagnostics — a runtime summary, module logs, events and history, and diagnostic capture and probes — but never invoke modules, touch files or ports, or open a terminal or preview.

<a id="g5.vm.transfer"></a>
[I · g5.vm.transfer]

The platform does not support an ownership transfer of a running workspace VM  

<a id="g5.data.identity"></a>
[D · g5.data.identity]

A VM incarnation is one boot of a workspace VM; its identity fences one instance from its replacement.

<a id="g5.data.identity.incarnation"></a>
[D · g5.data.identity.incarnation]

A command or result for one VM instance must never be applied to its replacement. Late results must not overwrite the replacement's current editor, proposal or operation state; they may still be kept as history.

<a id="g5.security.credentials.publication"></a>
[D · g5.security.credentials.publication]

Git-provider credentials remain server-side rather than entering the guest environment.

<a id="g5.security.credentials.reserved-env"></a>
[D · g5.security.credentials.reserved-env]

User settings and secrets must not override reserved platform identity, runtime or provider-home environment keys.

This protects platform-selected launch values; the [guest authority model](../concepts/security.g5.md#g5.security.guest-authority) and [Runtime Config loader decision](../concepts/security.g5.md#g5.security.runtime-config-loader-env) define its security scope.

<a id="g5.documents.read.live-cache"></a>
[D · g5.documents.read.live-cache]

Bind live caches and streams to the owner and VM incarnation, invalidating them before rendering read-only content after Stop, transfer or revocation.

- Read and Save completions follow the [VM instance invariant](#g5.data.identity.incarnation), and late results are discarded after Stop, ownership transfer or access revocation.
- [Durable reader authority](../features/workspaces.g5.md#g5.workspace.durable-read) forbids a live fallback.

<a id="g5.documents.save"></a>
[D · g5.documents.save]

File mutation preconditions include the editor's observed VM instance, under the [VM instance invariant](#g5.data.identity.incarnation).

<a id="g5.documents.save.acknowledgment"></a>
[D · g5.documents.save.acknowledgment]

A successful Save acknowledges only the submitted content, preserving later edits and leaving them unsaved when they differ.

<a id="g5.vm.preview-exclusive"></a>
[I · g5.vm.preview-exclusive]

A workspace runs one preview at a time: while a preview is queued, running or closing, a start is refused and Close is the only offered action. A new preview starts only after the previous one settled or its VM was lost, without automatic restart or stale-event effects. The preview shares the owner's VM with chat and terminal work, so a replacement that silently killed the old process left its side effects (containers, daemons) running beside the new one.  

<a id="g5.preview.service"></a>
[D · g5.preview.service]

Preview is a visible cancellable service under live-access authority, exempt from bounded-work slots and wall-clock limits.

<a id="g5.preview.start-and-preview"></a>
[D · g5.preview.start-and-preview]

While the owner's VM is stopped, failed or starting, the preview stays usable: **Start workspace and preview** is one owner request that records [Start consent](#g5.lifecycle.start) and the preview task bound to it, the way a [Start and send](../features/chat.g5.md#g5.chat.start-and-send) input is bound.

- The bound task waits for its own Start: it is dispatched only once that Start brings the VM to ready, never by a VM another intent brought up.
- If the consent is cancelled, superseded, expires or fails before then, the task is cancelled in the same transition with its reason shown (a failed Start keeps the existing did-not-start ending).
- Nothing here authorizes waking: only the labelled action records consent. A request refused before any Start is recorded (a missing or refused committed configuration, an active preview) records none, and one refused after recording a Start cancels that Start unless [another action has joined it](#g5.preview.start-and-preview.joining), in which case the consent stays for that action's work.

<a id="g5.preview.start-and-preview.joining"></a>
[D · g5.preview.start-and-preview.joining]

While the owner's own Start is already under way, the preview binds to that Start instead of recording another (Preview once running), and so does any other compound action of theirs, so a second Start and send or Start workspace and preview never supersedes the first and cancels the work bound to it. A joinable Start is one the owner recorded, still waiting or starting, and within its bounded lifetime; a failed request leaves a consent that another action has joined, which the join marks on the consent itself. A request whose recorded Start the VM no longer carries live is refused rather than bound to whatever Start replaced it; while compute starts under a Start that is not joinable (the platform's continuation, or the owner's own past its lifetime) the preview is refused with that reason until the VM runs; and a request that recorded a Start and then finds the VM running records its preview only if that very consent brought the VM up.

<a id="g5.preview.start-and-preview.committed-configuration"></a>
[D · g5.preview.start-and-preview.committed-configuration]

While the VM is not running, the applications offered and the one recorded come from the configuration committed on the branch, and a committed configuration the launch would refuse is reported as that refusal; the guest still launches from its saved working tree and reports the port it listens on, which the relay and the dialog follow. A retry of the same request replays the preview it recorded, whatever its Start did since.

<a id="g5.preview.start-and-preview.close"></a>
[D · g5.preview.start-and-preview.close]

[Exclusivity](#g5.vm.preview-exclusive), replay and [admission](#g5.runtime.preview.admission) are unchanged, a pending preview counting as the one preview; Close cancels the preview and not the Start, as withdrawing a queued message does not stop one.

<a id="g5.vm.preview-stop"></a>
[I · g5.vm.preview-stop]

Closing a preview is a bounded graceful stop owned by the preview task: the guest stops the application, runs the profile's optional cleanup\_command with the same shell, working directory and environment as command, and only then exits. Cleanup also runs when the application exits on its own  

<a id="g5.provider"></a>
[D · g5.provider]

A provider is a coding-agent subscription, such as a native login, whose credentials live in the owner's VM home.

<a id="g5.provider.credential-files"></a>
[D · g5.provider.credential-files]

Native subscription credentials remain in the owner VM home with restrictive permissions. Transfer purge owns release before an ownership change.

<a id="g5.provider.credential-files.import"></a>
[D · g5.provider.credential-files.import]

Reject and scrub retired CODEX\_AUTH\_JSON instead of materializing uploaded or copied subscription credentials.

<a id="g5.provider.credential-files.cache"></a>
[D · g5.provider.credential-files.cache]

The native login cache may be read by tools running as that owner and may disappear with VM replacement; it is not durable platform credential authority. This developer-desktop trust model provides neither a credential broker nor isolation from other tools running as the owner.

<a id="g5.vm.boot-checkout"></a>
[I · g5.vm.boot-checkout]

Each workspace repository is materialized from its mapped internal Git branch at the commit selected for boot.  

<a id="g5.vm.resume"></a>
[I · g5.vm.resume]

Resume restores saved local commits, checkout state and uncommitted files when a recovery snapshot exists.  

<a id="g5.vm.origin"></a>
[I · g5.vm.origin]

origin refers to G5’s internal repository, not directly to the external provider (if configured).  

<a id="g5.vm.workspace-refs"></a>
[I · g5.vm.workspace-refs]

The workspace branch is available locally as refs/heads/<branch> and its G5 tracking ref as refs/remotes/origin/<branch>.

<a id="g5.vm.parent-ref"></a>
[I · g5.vm.parent-ref]

The parent branch is fetched into refs/remotes/origin/<parent> when available.  

<a id="g5.vm.provider-refs"></a>
[I · g5.vm.provider-refs]

Observed provider branches use refs/remotes/provider/\*, separate from G5 branch tracking.  

<a id="g5.vm.reconciliation-refs"></a>
[I · g5.vm.reconciliation-refs]

Pinned reconciliation inputs use refs/g5/provider-inputs/\*.  

<a id="g5.vm.recovery-refs"></a>
[I · g5.vm.recovery-refs]

Recovery uses private refs/g5/restore/\* refs populated from immutable flush records.  

<a id="g5.vm.refresh-fetch"></a>
[I · g5.vm.refresh-fetch]

Normal refresh fetches the mapped branch and workspace-specific provider refs; it does not fetch every branch or tags.  

<a id="g5.vm.fetch-frame"></a>
[I · g5.vm.fetch-frame]

Fetching tracking and provider refs leaves the current checkout, index and files unchanged.  

<a id="g5.vm.ref-authority"></a>
[I · g5.vm.ref-authority]

Ref visibility does not grant write authority; pushes remain subject to G5’s workspace and namespace policies.


## Preview admission

<a id="g5.runtime.preview"></a>
[D · g5.runtime.preview]

Runtime rules admit and bound a workspace's preview.

<a id="g5.runtime.preview.admission"></a>
[D · g5.runtime.preview.admission]

Enforce [preview exclusivity](#g5.vm.preview-exclusive) by recording the task and start intent under the workspace authority lock.

- A replay must match its original request.
- Admit browser traffic only with [current incarnation authority](#g5.data.identity.incarnation) and listener-ready evidence.
- Close revokes admission immediately; settlement clears intent only for the same task.
- Bind every stream to its task and incarnation, and close it on Stop or ownership transfer.

## File mutation and observation

<a id="g5.runtime.files"></a>
[D · g5.runtime.files]

The dispatcher serializes file mutations against publication and treats watches as bounded change hints.

<a id="g5.runtime.files.mutation"></a>
[D · g5.runtime.files.mutation]

Serialize filesystem RPC precondition checks with publication.

- Stage existing-file updates beside the file, then recheck the observed hash before replacement.
- Creates and renames must not replace an existing destination.
- Caller cancellation does not cancel admitted mutations or their bounded replay within the incarnation.
- Native owner tools use the same tree outside this RPC lock.

<a id="g5.runtime.files.observation-recovery"></a>
[D · g5.runtime.files.observation-recovery]

Treat filesystem watches as bounded change hints.

- Request a rescan after lost events, uncertain paths or registration gaps.
- Coalesce refreshes within a fixed window, and retry degraded coverage without waiting for another event.
- Report a concrete event path only while its held parent remains the current confined parent.

## Execution and browser authority

<a id="g5.workspace.preview-origin-security"></a>
[D · g5.workspace.preview-origin-security]

Serve interactive previews only from an isolated per-preview origin, admitting a short-lived, target-bound browser handoff against the current owner and runtime, or a workspace agent's [preview credential](#g5.preview.agent-access).

- A browser handoff and the preview ticket it becomes, including each renewal, are a person's [browser ticket](../concepts/security.g5.md#g5.security.credential-classes): the explicit mint, the platform-path redirect and relay renewal share one issuance boundary that refuses an organization API key of any scope before anything is minted, so no browser credential outlives a key's revocation.
- A key may still relay requests on the preview origin with its own authority. Tickets issued before that boundary carry no key provenance and cannot be revoked by it; each lapses thirty minutes after its last relayed traffic.
- Strip platform credentials before relay, reject upstream cookie and security-header contamination, and require the preview origin for WebSocket upgrades.
- The preview command learns its own origin as `G5D_PREVIEW_ORIGIN`, so an application that admits browser origins can admit the one it is served from. The API derives that origin from the preview task and the suffix when it creates the task, and without a suffix the variable is absent.
- Legacy platform-origin paths must redirect or refuse, never relay. Without a configured preview suffix, report `PREVIEW_ORIGIN_UNCONFIGURED` while keeping the API available.
- A separate registrable domain remains deferred; subdomain isolation does not provide separate-site isolation.
- The relay cannot intercept JavaScript parent-domain cookie writes, so platform refresh and GitHub App attempt cookies require [host protection](../concepts/security.g5.md#g5.security.browser-cookie-origin) in addition to relay filtering.

<a id="g5.preview.agent-control"></a>
[D · g5.preview.agent-control]

A workspace agent can discover the preview applications, start, inspect and close the managed preview, and list and save preview profiles of its own workspace or another workspace within its scope, through the owner's preview operations and [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope); exclusivity, replay and Close are unchanged. It receives no preview browser handoff, which is a person's [browser ticket](../concepts/security.g5.md#g5.security.credential-classes). It reaches its own preview on guest loopback, and its own or another workspace's preview through the relay with a [preview credential](#g5.preview.agent-access).

<a id="g5.preview.agent-access"></a>
[D · g5.preview.agent-access]

A workspace agent reaches the managed preview of its own or another in-scope workspace with a preview credential its VM credential mints under [agent scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) and the [other-workspaces setting](../concepts/security.g5.md#g5.security.agent-other-workspaces) for the target's current ready preview task; minting is an audited agent operation.

- The credential names that task, the target and the caller incarnation, lasts ten minutes and is never renewed.
- It is a [credential class](../concepts/security.g5.md#g5.security.credential-classes) of its own: accepted only from its own request header on that task's preview origin, from a client sending no browser fetch metadata, and stripped before relay; never a cookie, URL parameter, browser handoff or person session, and never upgraded to a WebSocket.
- Each request rechecks the caller incarnation, the target's agent scope (its current owner included), the setting and that the task is still the ready current preview, so Close, either VM's Stop, transfer or turning the setting off refuses the next request.
- Relayed requests renew the target's activity like other [preview traffic](#g5.lifecycle.idle).
- The agent's command uses the credential itself and never prints it, because agent transcripts keep tool output unredacted for readers of the workspace.

<a id="g5.workspace.module-execution"></a>
[D · g5.workspace.module-execution]

Each invocation identifies its catalog operation and VM incarnation, uses authorized capabilities, and respects declared concurrency and bounded capacity.

- Never invoke a cancelled or settled execution identity anew. Journal start intent before launch and cancellation before signaling.
- Retain the exact redacted outcome separately from prunable events, and expose persistence failures without claiming durability.
- Before serving after restart, durably settle executions whose supervision was lost without relaunching them; refuse startup if that persistence fails.
- Prune only durably acknowledged event prefixes, keep sequence watermarks, and keep unacknowledged events even beyond the journal's soft target.
- Replay serves bounded pages from a sequence-to-offset index rather than rescanning the file, and reports whether more follows (a caller's event and byte limits, under the 4 MiB reply line; one event alone may be up to 1 MiB).
- An acknowledgement always makes its watermark durable but compacts the file only when the acknowledged prefix is large (at least 1 MiB and a quarter of the retained file), when nothing unacknowledged remains, or when the disk budget needs it, so a consumer acknowledging in [batches](../features/chat.g5.md#g5.chat.history-batches) does not rewrite the same suffix repeatedly. An append during compaction is never lost, and output overflow or drain failure cannot count as clean success.
- A command or preview attempt that times out, exceeds a limit or has its supervision interrupted is recorded `timed_out` and fails its task.
- Such tasks cannot be resumed, so neither outcome holds them for Continue, and a `suspended` outcome is refused with 409. Preview keeps its [service lifetime](#g5.preview.service).

## Lifecycle consent and stopping

<a id="g5.lifecycle"></a>
[D · g5.lifecycle]

A workspace's lifecycle is its consented compute: Start and Resume, Stop, ownership transfer and idle reclamation.

<a id="g5.lifecycle.start"></a>
[D · g5.lifecycle.start]

Attaching compute requires durable, explicit Start or Resume consent for the current owner and lifecycle generation; clearly labeled compound actions may provide it.

- Capacity waits and API restarts keep consent only for its bounded lifetime, and retries cannot revive cancelled or superseded intent.
- If the lease expires while the VM is still starting, including a Start whose outcome stays unknown under [start settlement](hosts.g5.md#g5.runtime.start.settlement), the Start fails with `lease_expired` and its consent ends instead of returning to the queue; the owner may press Start again.
- Reads, Save, queued work, Stop and transfer do not themselves authorize waking.
- [Automatic continuation](#g5.lifecycle.start.automatic-continuation) after a platform-caused interruption is the one scoped exception.

<a id="g5.lifecycle.start.held-work"></a>
[D · g5.lifecycle.start.held-work]

When an owner's Start, or an [automatic continuation](#g5.lifecycle.start.automatic-continuation) start, brings the workspace back to ready, agent and command work held by a stop, a transfer, a needed restart, a lost VM or an attempt suspended with no question pending (`owner_stop`, `idle_stop`, `transfer`, `start_required`, `vm_lost`, `response_timeout`) returns to the queue by itself, so a chat message queued before a workspace Stop goes out without another message. Work held on the owner's own decision stays held: an input of unknown delivery waits for [Redeliver](../features/chat.g5.md#g5.chat.uncertain-delivery), and a turn with an unanswered clarification waits for its answer or explicit cancellation under [native question recovery](../features/chat.g5.md#g5.chat.native-question-recovery). Reason: after an explicit Start the owner expects their work to proceed.

<a id="g5.lifecycle.start.automatic-continuation"></a>
[D · g5.lifecycle.start.automatic-continuation]

When the platform, not the owner, ends a workspace's compute while a chat turn is active (a host drain or rollout stop, or a lost VM), G5 starts the workspace again for its current owner so the interrupted turn continues without their action.

- This automatic start is its own scoped consent: it exists only for the turns the interruption left queued or held `vm_lost`, is admitted under the same owner, membership, execution-authority and transfer conditions as an owner's Start, and uses ordinary placement.
- It never follows an owner, idle, transfer, deactivation, credential, capacity or Delete stop, and never starts a workspace whose remaining work all waits on an owner decision (unknown delivery, an unanswered clarification) or whose lost attempt has not settled.
- No input is replayed: the turn continues through [chat recovery](../features/chat.g5.md#g5.chat.recovery) on the restored save, and after a lost VM the continuation warns that changes after the last save may be lost, so the agent re-verifies rather than assumes.
- An organization administrator may turn automatic continuation off under [organization settings](../features/organizations.g5.md#g5.data.configuration.org-admin-settings); it is on by default, and a workspace interrupted while it is off waits for its owner's Start.

<a id="g5.lifecycle.start.automatic-continuation.decision"></a>
[D · g5.lifecycle.start.automatic-continuation.decision]

Expands [automatic continuation](#g5.lifecycle.start.automatic-continuation): each ended incarnation is decided once, and the decision is recorded with the workspace VM, with the consent when the decision is to start.

- Every stop other than a drain or rollout is that decision when it is admitted or announced, whether the VM is running, in a deadline stop's warning window or already stopped, so a workspace is never started again behind its owner's or an administrator's back even if the VM is lost before the stop settles its turns.
- Loss evidence belongs to the incarnation it ended: a Start that fails before Ready, the owner's or an automatic one, ends its own consent and interrupts nothing, so a hold left from an earlier loss does not start it again.
- Lost VMs, including one lost while a drain was still settling its turns, count toward a cap of two automatic continuations per workspace between owner Starts; completed drains do not count, because host operations already bound them.
- Incarnations that ended before this rule existed are decided as the old rule left them: the owner starts them again.

<a id="g5.lifecycle.start.automatic-continuation.consent"></a>
[D · g5.lifecycle.start.automatic-continuation.consent]

Expands [automatic continuation](#g5.lifecycle.start.automatic-continuation): the consent waits up to one hour for capacity, after which the work waits for the owner as before. A consent that ends without its VM ever running, expired, cancelled or failed to boot, ends the continuation, so placements the host refused meanwhile do not renew the hour; once the owner has taken the interrupted work over with a Start of their own, their consent governs it even if it lapses without running. The consent stays scoped to the interrupted turns: if the owner ends the last of them while it waits, by Cancel, chat Stop, a new message or a withdrawal, the consent is cancelled rather than starting an empty workspace, and a turn whose clarification is still unanswered does not qualify whether it is held or queued.

<a id="g5.lifecycle.start.automatic-continuation.visibility"></a>
[D · g5.lifecycle.start.automatic-continuation.visibility]

Expands [automatic continuation](#g5.lifecycle.start.automatic-continuation): the owner learns what happened in the interrupted chat.

- A platform notice names the interruption and says the workspace was started again, or why it was not and that Start continues it; the same text reaches the owner as a live workspace notification, and the Activity row keeps its interruption reason.
- Decisions the owner made themselves get no notice: their Stop, a Start of theirs that lapsed, and an automatic start that finds they have ended its last turn meanwhile (which records nothing of its own and is decided as none).
- The restarted workspace keeps its ordinary idle timeout after the continued work settles.

<a id="g5.lifecycle.stop"></a>
[D · g5.lifecycle.stop]

Stop excludes new work and invalidates older Start consent before settling attempts and saving.

- A save failure never returns the workspace to running.
- After the [save window](#g5.lifecycle.stop.window), terminate through the host and keep any unsaved-loss record; no new loss display is required.
- The next explicit Start restores the last complete receipt.
- An unreachable host stays pending under existing custody rules; logical release does not prove [physical exit](#g5.vm.physical-uniqueness-question).

<a id="g5.lifecycle.stop.window"></a>
[D · g5.lifecycle.stop.window]

Owner, idle, transfer, member-deactivation, stop-by-user and ordinary Delete stops allow two minutes to save.

- Retry transient capture failures within that window.
- Deterministic failure or expiry durably ends capture and records the unsaved-work loss before killing, so reconciliation cannot reopen the window; the release keeps a loss already recorded, with its first time.
- Deadline stops keep their separate announced deadline and save window; force Delete skips saving.
- Never record a completed receipt for that same stop as lost.

<a id="g5.lifecycle.member-deactivation"></a>
[I · g5.lifecycle.member-deactivation]

[Deactivating a member](../features/organizations.g5.md#g5.organization.deactivation) stops their running workspace execution in that organization and excludes queued work from starting. Access remains disabled throughout failed shutdown and retries. Reactivation does not restore old Start consent or resume work; [explicit Start](#g5.lifecycle.start) remains necessary.

<a id="g5.lifecycle.member-deactivation.recovery"></a>
[D · g5.lifecycle.member-deactivation.recovery]

Database guards serialize ownership acquisition and transitions into starting or running with current membership activity.

- Lifecycle command admission holds that authority only through bounded enqueue; an ambiguous start is observed before any further action. Logical lease expiry and forced custody release cannot complete pending member shutdown without termination evidence.
- Commit the membership's [execution hold](#g5.lifecycle.stop.by-user) with deactivation, joining a pending one. It blocks that membership's new workspace execution submissions and runtime admission even if the member is reactivated before cleanup finishes.
- The existing lifecycle owner discovers the hold durably, cancels old Start intent, holds queued work and drives bounded batches through the existing Stop operations.
- Cleanup may save and settle only the affected incarnation under trusted lifecycle authority; it grants no owner commands or new execution.
- A pending stop is joined without extending its save deadline. Retries require the same cleanup identity and current ownership, so they cannot stop a replacement or a recipient's VM after transfer.
- Admission failures keep a durable retry schedule outside the failed transaction. Unattempted obligations precede due retries, and repeated failures move behind earlier due retries, so a failing batch cannot monopolize bounded discovery after worker restart.
- Clear the barrier only after all affected execution and old consent are settled, with competing admission excluded. Membership removal cannot erase unfinished cleanup.
- Host loss, deleted metadata or logical lease release alone does not establish physical shutdown; keep unconfirmed cleanup pending under [Stop custody](#g5.lifecycle.stop).
- Runtime denial does not depend on host availability, worker progress or delivery of a revocation event. No fan-out limit or physical-shutdown deadline is promised.

<a id="g5.lifecycle.stop.by-user"></a>
[D · g5.lifecycle.stop.by-user]

Stop by user commits a constant-size execution hold on one organization membership without changing access; a repeated request joins the pending hold. While it is set, Start, runtime admission and new execution submissions are refused for every workspace that user owns there, including workspaces a fork creates during the pass, and the [cleanup](#g5.lifecycle.member-deactivation.recovery) reconciler cancels old Start consent and drives ordinary [Stop](#g5.lifecycle.stop) for each, reporting per-workspace blockers. Caller loss does not end it, the caller's own workspace is not exempt, and neither is an organization Admin.

<a id="g5.lifecycle.stop.by-user.release"></a>
[I · g5.lifecycle.stop.by-user.release]

The hold clears when no workspace the user owns is starting, running or stopping, holds a VM or active Start intent, or has queued or running tasks. There is no separate unfreeze; the next explicit Start works as usual. Nothing restarts automatically and cancelled work is not replayed.

<a id="g5.lifecycle.stop.claim"></a>
[D · g5.lifecycle.stop.claim]

Once admitted, a stop runs to its end whether or not the request that asked for it is still connected: a reload, a reset connection or a dropped handler leaves the execution running.

- The executing replica holds a claim on the operation that it renews while it works; an execution that dies (API crash, restart) loses the claim within half a minute, and the lifecycle reconciler on any replica resumes the same operation and incarnation.
- A repeated request for the same operation joins the live execution and reports its progress instead of running it again; a different operation is refused while one is in progress.
- Every stop reports its stages.

<a id="g5.lifecycle.transfer"></a>
[D · g5.lifecycle.transfer]

Transferring ownership hands a workspace to a new owner; [a running VM is not transferred](#g5.vm.transfer).

<a id="g5.lifecycle.transfer.admission"></a>
[D · g5.lifecycle.transfer.admission]

At most one ownership transfer is pending per workspace; a second request is refused rather than queued behind it. Start admission re-reads the pending-transfer predicate after taking the workspace lock, so a transfer admitted while the Start waited is honored.

<a id="g5.lifecycle.idle"></a>
[D · g5.lifecycle.idle]

Reclaim idle compute according to configured inactivity, rechecking activity and work when admitting Stop.

- Running bounded work and a requested preview that has not reported ready prevent idle Stop. Task completion alone does not release the workspace.
- G5 UI activity requires actual authorized owner interaction: automatic refresh, refetch, polling, event-WebSocket reconnect or resubscribe and their downstream requests and retries never renew it, while recovery and authorization continue independently.
- Terminal activity requires nonempty valid keyboard or paste input, never opening, reconnecting, quiet presence, ping/pong, resize or control, output or invalid frames.
- Preview traffic is an intentional exception: authorized proxied HTTP requests and new preview WebSocket connections may renew activity under existing throttling, including automatic traffic.
- Do not inject preview scripts, rewrite CSP for activity, or expose preview interaction-reporting endpoints.

<a id="g5.lifecycle.idle.observability"></a>
[I · g5.lifecycle.idle.observability]

Log each committed activity-timestamp change with workspace ID, VM ID and epoch, activity reason, request or correlation ID, and previous and new timestamps.

- Distinguish initialization, lifecycle clearing and preview traffic from user activity; preview resets do not establish human interaction.
- Never log terminal input or document contents.
- Log idle-stop decisions and the exemptions that prevent shutdown.
- Keep candidate selection bounded in SQL, and sample exemption diagnostics in bounded pages at a limited cadence so observability does not expand every reconciliation sweep to all expired workspaces.

<a id="g5.preview.service.stop-reason"></a>
[D · g5.preview.service.stop-reason]

When workspace Stop or loss ends a preview, retain the lifecycle reason in its task outcome and show it to the owner, even if cancellation precedes confirmed launch. Bare module cancellation does not explain idle Stop, transfer, host drain or VM loss.

## Execution ownership

<a id="g5.architecture.agent_execution_tree"></a>
[D · g5.architecture.agent_execution_tree]

The execution owner keeps dispatcher privilege while its supervisor runs as the module identity, so cleanup can reap descendants that change identity.

- When the provider ends its turn normally, including a clean native-question deferral, an explicit release may free execution capacity while leaving tools under the agent and VM lifecycle.
- Stop, forced End, limits, failure or supervisor interruption veto that release: clean up descendants before freeing capacity. Classify the outcome before harvesting; a later interruption may still veto release.
- This provides lifecycle containment within the [guest authority model](../concepts/security.g5.md#g5.security.guest-authority); it does not isolate the dispatcher from a guest-root owner.

## Preview diagnostics

<a id="g5.preview.service.exit-diagnostics"></a>
[D · g5.preview.service.exit-diagnostics]

On preview application failure, keep its exit status or signal and a masked stderr tail of at most the last 4 KiB in the task outcome; that outcome is the only place the tail exists, since nothing of the application reaches the guest diagnostic log.

- If cleanup fails after a clean application exit, report a distinct cleanup message.
- Keep customer stdout and stderr outside module protocol framing, so they cannot forge an outcome and descendants cannot hold the outcome channel open.
- The application's streams go to its [hosted terminal session](#g5.preview.output.live-tail), never to events, because a per-line output event would be a durable task event the shared API reconciler applies one at a time, and one verbose build once delayed every workspace's queued work.
- Neither stream reaches the guest's bounded log, which no owner surface shows. Without a hosted terminal (an older dispatcher), the application's stdout is discarded and its stderr read only for the tail.

<a id="g5.preview.output"></a>
[D · g5.preview.output]

A preview application's live output is VM-local evidence the owner can watch, never a durable lifecycle fact.

<a id="g5.preview.output.live-tail"></a>
[D · g5.preview.output.live-tail]

A preview application's output is VM-local evidence, not a lifecycle fact: it lives in dispatcher memory as the scrollback of a hosted terminal session named `preview:<task id>`.

- The application runs on that session's PTY as its stdin and stdout (stderr teed onto it) but not as its controlling terminal, so a leader's exit never hangs up the application tree and Close keeps its SIGTERM semantics; the profile's cleanup command and dim stage markers go to the same terminal.
- The session is bounded (4 MiB for preview), kept after the application exits until the next preview start, workspace Stop or ownership transfer, and never persisted; only the [exit diagnostics](#g5.preview.service.exit-diagnostics) outlive the VM.
- The application's bytes count toward no module output budget. Viewing follows [the hosted-session attachment rules](#g5.preview.output.live-tail.viewer).

<a id="g5.preview.output.live-tail.viewer"></a>
[D · g5.preview.output.live-tail.viewer]

The owner reads a [hosted session](#g5.preview.output.live-tail) through the unchanged [terminal transport](../features/terminal.g5.md#g5.terminal.runtime) and [attachment rules](../features/terminal.g5.md#g5.runtime.terminal.attachment) with [offset reattachment](../features/terminal.g5.md#g5.terminal.reattach), keyed on the session kind rather than the client.

- Input is dropped, resize applies, close detaches without ending the session, and an ended session answers an attach with its retained tail followed by a `closed` control line (`session_ended`).
- One viewer at a time: a newer attach supersedes the current one, whose stream ends with a `closed` control line (`superseded`), so the browser knows it was replaced rather than disconnected — it stops reconnecting, keeps the output on screen, says it is viewing in another window and offers Take over, which reattaches at its own offset.
- Control changes hands only on that Take over click, never by reconnect policy, so two windows on one log do not alternate.
- Hosted sessions can only be attached, never created, from the terminal protocol; they count toward no shell limit and are never enumerated to the agent.
- Watching follows the same [activity rule](#g5.lifecycle.idle) as an open terminal: only nonempty input renews owner activity, and a hosted session accepts none, so a quiet viewer never keeps the VM running by itself.

## Lifecycle defaults

<a id="g5.lifecycle.idle.settings"></a>
[D · g5.lifecycle.idle.settings]

Give every new workspace, including any fork, the organization's idle timeout at creation; the organization default is initially eight hours.

- A [personal workspace](../features/personal_workspace.g5.md#g5.personal.capacity) uses the separate personal default instead.
- The workspace owner may override its value later.
- Changing the organization default never rewrites existing workspaces, and other forked settings keep their own inheritance rules.
- [Organization administrators](../features/organizations.g5.md#g5.security.org-admin-settings) edit this default.

## Open questions


<a id="g5.vm.request-isolation-question"></a>
[Q · g5.vm.request-isolation-question]

Must every dispatcher API operation execute in a separate process, or is [process isolation](#g5.vm.process-isolation) intended for external modules only? Provisionally preserve the stated per-request requirement. Threaded filesystem execution is a code gap under that reading; allowing trusted built-ins in process is a plausible explicit alternative, not an inferred correction.

## Metadata
### g5.vm

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule; it names what the `g5.vm.*` blocks describe.

### g5.vm.durability

- human-provided
- Source: [Ben, line 78](../../../docs/ben_new_ontology.md#L78).

### g5.vm.setup

- human-provided
- Source: [Ben, line 79](../../../docs/ben_new_ontology.md#L79).

### g5.vm.flush

- human-provided
- Source: [Ben, line 80](../../../docs/ben_new_ontology.md#L80).

### g5.vm.branch-alignment

- human-provided
- Source: [Ben, line 81](../../../docs/ben_new_ontology.md#L81).

### g5.vm.uniqueness

- human-provided
- Source: [Ben, line 82](../../../docs/ben_new_ontology.md#L82).
- Clarification: [physical custody](#g5.vm.physical-uniqueness-question). HostAdmission checks logical placement/custody only, not physical singleton under a partition.

### g5.vm.physical-uniqueness-question

- human-provided
- Source and implications: [runtime audit](../../uplift/ben-ontology/investigations/runtime.md), worker-host API v9 defers host expiry/kill; RUNTIME-Q-PHYSICAL is resolved by the supplied logical-custody decision, subject to its cleanup-progress condition.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 53 (L1A-Q1).

### g5.vm.dispatch-api

- human-provided
- Source: [Ben, line 83](../../../docs/ben_new_ontology.md#L83).

### g5.vm.process-isolation

- human-provided
- Source: [Ben, line 84](../../../docs/ben_new_ontology.md#L84).
- Clarification: [request isolation](#g5.vm.request-isolation-question). Dispatcher file operations run on threads; no process-isolation proof is claimed.

### g5.vm.filesystem

- human-provided
- Source: [Ben, line 85](../../../docs/ben_new_ontology.md#L85).
- Code gap: [RUNTIME-001](../../../docs/code_gaps.md): direct live operations do not enforce gitignore. FileVisibility checks the intended and observed distinction, not all filesystem semantics.

### g5.vm.modules

- human-provided
- Source: [Ben, line 86](../../../docs/ben_new_ontology.md#L86).

### g5.vm.module-inventory

- human-provided
- Source: [Ben, line 87](../../../docs/ben_new_ontology.md#L87).

### g5.vm.catalog

- human-provided
- Source: [Ben, line 88](../../../docs/ben_new_ontology.md#L88).

### g5.security.owner

- human-provided
- Source: [Ben, line 89](../../../docs/ben_new_ontology.md#L89).
- [Member-deactivation request](../../chat_intent/member-removal/request.md#human-instructions) makes membership activity explicit; the [cleanup barrier](../../pr_ontology_diff_design/member-removal/ontology-plan.md#state-and-ownership) is an agent decision preventing old execution from returning during early reactivation. Runtime checks current membership and cleanup alongside ownership/incarnation; [implementation evidence](../../pr_ontology_diff_design/member-removal/ontology-plan.md#implementation-evidence) covers retained handles and early reactivation.

### g5.security.owner.inspection

- agent
- Expands: [owner authority](#g5.security.owner).
- Inferred from code for the writing-rules [coverage review](https://github.com/G5Labs/G5-Lean/issues/1119); the owner should confirm the intent and whether this authority belongs with the security ontology.
- Implementation: `backend/g5d-dispatch/src/protocol.rs` (`inspection_grant_id`; the grant admits only `runtime.inspect.summary`, `modules.logs`/`events`/`history`, `diagnostics.capture`/`probe`).

### g5.vm.transfer

- human-provided
- Source: [Ben, line 90](../../../docs/ben_new_ontology.md#L90).

### g5.data.identity

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [incarnation](#g5.data.identity.incarnation).

### g5.data.identity.incarnation

- human-provided
- Expands: [VM identity](#g5.data.identity).
- Source: [Ben, line 91](../../../docs/ben_new_ontology.md#L91).
- Code gap: [RUNTIME-002](../../../docs/code_gaps.md): old stop settlement can withdraw replacement notices despite a fenced VM update. StaleRelease checks the reachable abstract schedule.

### g5.security.credentials.publication

- human-provided
- Source: [Ben, line 92](../../../docs/ben_new_ontology.md#L92).

### g5.security.credentials.reserved-env

- human-provided
- Source: [Ben, line 93](../../../docs/ben_new_ontology.md#L93).
- Security scope clarified by the user's 2026-10-16 [guest authority decision](../concepts/security.g5.md#g5.security.guest-authority); retained platform-key restrictions do not require the [#576 remediation](../concepts/security.g5.md#g5.security.runtime-config-loader-env).

### g5.documents.read.live-cache

- human-provided
- Source: [Ben, line 94](../../../docs/ben_new_ontology.md#L94).

### g5.documents.save

- human-provided
- Source: [Ben, line 95](../../../docs/ben_new_ontology.md#L95).

### g5.documents.save.acknowledgment

- human-provided
- Source: [Ben, line 96](../../../docs/ben_new_ontology.md#L96).

### g5.vm.preview-exclusive

- human-provided
- Source: [Ben, line 97](../../../docs/ben_new_ontology.md#L97).

### g5.preview.service

- human-provided
- Source: [Ben, line 98](../../../docs/ben_new_ontology.md#L98).

### g5.preview.start-and-preview

- human-verified
- user-facing-behavior
- test-coverage
- Reason: owner request (2026-11-05) that clicking Preview and picking a profile on a workspace without a VM attach the VM and start the preview, and the owner's choice of the durable server-side record over a browser-held one.
- Source: owner request in workspace chat (2026-11-05): "when there is no vm attached to a workspace, and user clicked preview button and pick the profile, the preview will trigger a vm attach and start the preview", then "I want option B like other chat start and send" ([intent](../../chat_intent/preview-wovm/request.md), [design](../../pr_ontology_diff_design/preview-wovm/ontology-plan.md), question set `preview-without-vm` in the docs repository, `docs/27786cca-1e71-45a7-b528-de1cdb9c677f/`).
- Evidence: [backend integration tests](../../../backend/g5-api/tests/api_preview_start_and_preview.rs), [dialog tests](../../../frontend/src/components/preview/PreviewControl.test.tsx), [e2e flow](../../../frontend/e2e/flows/workspace-preview.spec.ts); the design records the implementation and its review rounds.
- Agents (`g5d workspace preview-start`) keep the running-VM requirement; the service supports the compound path for a later flag.

### g5.preview.start-and-preview.joining

- Expands: [parent](#g5.preview.start-and-preview)
- human-verified
- architecture-decision
- test-coverage
- Evidence: [backend integration tests](../../../backend/g5-api/tests/api_preview_start_and_preview.rs) (joining Start and send and Resume, a cancelled or replaced recorded Start, a boot under a Start the owner cannot join, a compound Start on running compute); review rounds 2 to 14 in the [design](../../pr_ontology_diff_design/preview-wovm/ontology-plan.md).

### g5.preview.start-and-preview.committed-configuration

- Expands: [parent](#g5.preview.start-and-preview)
- human-verified
- user-facing-behavior
- test-coverage
- Evidence: [backend integration tests](../../../backend/g5-api/tests/api_preview_start_and_preview.rs) (committed listing, refused committed configuration, replay of the earlier digest), [dialog tests](../../../frontend/src/components/preview/PreviewControl.test.tsx).

### g5.preview.start-and-preview.close

- Expands: [parent](#g5.preview.start-and-preview)
- human-verified
- user-facing-behavior
- test-coverage
- Evidence: [backend integration tests](../../../backend/g5-api/tests/api_preview_start_and_preview.rs) (Close cancels the preview only; one preview at a time), [dialog tests](../../../frontend/src/components/preview/PreviewControl.test.tsx).

### g5.vm.preview-stop

- human-provided
- Source: [Ben, line 99](../../../docs/ben_new_ontology.md#L99).
- [PreviewLifecycle](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/runtime/PreviewLifecycle.tla) exposed cleanup and replacement after unconfirmed stop (RUNTIME-004); the intended reap gate preserved their ordering. Whole-VM shutdown and actual process-group death remain outside the model; see [runtime evidence](../../uplift/ben-ontology/investigations/runtime.md).

### g5.provider

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [credential files](#g5.provider.credential-files).

### g5.provider.credential-files

- human-provided
- Expands: [provider](#g5.provider).
- Source: [Ben, line 100](../../../docs/ben_new_ontology.md#L100).

### g5.provider.credential-files.import

- human-provided
- Source: [Ben, line 101](../../../docs/ben_new_ontology.md#L101).

### g5.provider.credential-files.cache

- human-provided
- Source: [Ben, line 102](../../../docs/ben_new_ontology.md#L102).

### g5.vm.boot-checkout

- human-provided
- Source: [Ben, line 103](../../../docs/ben_new_ontology.md#L103).

### g5.vm.resume

- human-provided
- Source: [Ben, line 104](../../../docs/ben_new_ontology.md#L104).

### g5.vm.origin

- human-provided
- Source: [Ben, line 105](../../../docs/ben_new_ontology.md#L105).

### g5.vm.workspace-refs

- human-provided
- Source: [Ben, line 106](../../../docs/ben_new_ontology.md#L106).

### g5.vm.parent-ref

- human-provided
- Source: [Ben, line 107](../../../docs/ben_new_ontology.md#L107).

### g5.vm.provider-refs

- human-provided
- Source: [Ben, line 108](../../../docs/ben_new_ontology.md#L108).

### g5.vm.reconciliation-refs

- human-provided
- Source: [Ben, line 109](../../../docs/ben_new_ontology.md#L109).

### g5.vm.recovery-refs

- human-provided
- Source: [Ben, line 110](../../../docs/ben_new_ontology.md#L110).

### g5.vm.refresh-fetch

- human-provided
- Source: [Ben, line 111](../../../docs/ben_new_ontology.md#L111).

### g5.vm.fetch-frame

- human-provided
- Source: [Ben, line 112](../../../docs/ben_new_ontology.md#L112).

### g5.vm.ref-authority

- human-provided
- Source: [Ben, line 113](../../../docs/ben_new_ontology.md#L113).

### g5.runtime.preview

- agent
- Expands: [runtime](hosts.g5.md#g5.runtime).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [admission](#g5.runtime.preview.admission).

### g5.runtime.preview.admission

- agent
- Expands: [runtime preview](#g5.runtime.preview).
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md) and [preview transport contract](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/preview_terminal.g5.md); Close evidence names `full-stack.yaml`.
- PreviewLifecycle checked task exclusivity, current-task effects and ready-owner streams; digest matching was inspected separately. RUNTIME-004 still lacks verified stop-before-cleanup, and the profile's write restriction is not an enforced read-only mount: writes remain visible and flushable.

### g5.runtime.files

- agent
- Expands: [runtime](hosts.g5.md#g5.runtime).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [mutation](#g5.runtime.files.mutation), [observation recovery](#g5.runtime.files.observation-recovery).

### g5.runtime.files.mutation

- agent
- Expands: [runtime files](#g5.runtime.files).
- Source: [filesystem supplement](../../uplift/ben-ontology/investigations/runtime.md).
- FileMutationCAS checked serialized RPC updates and rejection of native edits seen before the final hash check. Later native edits can defeat stronger all-writer atomicity; replay retention and crash recovery were not modeled.

### g5.runtime.files.observation-recovery

- agent
- Source: [watcher supplement](../../uplift/ben-ontology/investigations/runtime.md).
- WatcherRecovery checked retained rescan obligations, fixed coalescing and recovery assuming eventual native, timer and consumer progress. It does not prove descriptor confinement, a lossless event stream or the client's rescan.

### g5.workspace.preview-origin-security

- agent
- Source: [preview audit](../../uplift/ben-ontology/investigations/chat-identity.md) and [removal of legacy relay](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-488-remove-legacy-preview/request.md); production, CI, Helm and self-hosted delivery use isolated preview hosts.
- PreviewBoundary checked target-bound handoff, owner/origin admission and credential/header filtering; single use holds within one API process. Shared-site JavaScript Domain-cookie risk and legacy browser-state cleanup remain, so this is not total site isolation.
- [Issue #917 design and verification](../../pr_ontology_diff_design/issue-917/ontology-plan.md) address credential transplantation through the implemented host-cookie contract. PreviewBoundary did not establish this browser-enforced guarantee or eliminate other shared-site risks; browser and API evidence are recorded separately.
- Person-session-only issuance: the owner's instruction on [G5Labs/G5-Lean#661](https://github.com/G5Labs/G5-Lean/issues/661) (2026-10-31) that every browser-credential producer accept a person session only, through the existing session guard, and that already-issued tickets be left to expire. Evidence: `api_keys_cannot_mint_preview_browser_handoffs` (`backend/g5-api/tests/api_task_lifecycle.rs`) with a narrow and a broad owner-attributed key against the mint, every platform-path GET and HEAD form, preview-host relay and revocation. Not exercised: a renewal-window relay by a key, which cookie-first admission on the preview host never produces.
- `G5D_PREVIEW_ORIGIN`: owner choice "Platform passes origin" on 2026-10-26 ([chat intent](../../chat_intent/issue-776/request.md#preview-origin-2026-10-26)), after the full-stack preview's API refused its own origin's cookie refresh. The origin travels in the preview module input (`origin`). The API sends it only to a VM whose module catalog is `2026.10.26-workspace-preview-origin` or later, because older guests refuse unknown preview input; elsewhere the variable is unset and the preview starts as before. [Design](../../pr_ontology_diff_design/issue-776/ontology-plan.md#preview-origin-for-full-stack-previews).

### g5.preview.agent-control

- agent
- Source: the owner's request to "implement preview parity" (2026-10-22) under [G5-Lean#710](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request); refusing the browser handoff follows `g5.security.credential-classes` and `g5.security.agent-human-exceptions`. [Design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-3-managed-preview).
- Implemented: `routes/workspace_agent.rs` preview routes sharing `cancel_preview_with` and `create_preview_profile_with` with the UI; `g5d workspace preview|preview-apps|preview-start|preview-close|preview-profiles|preview-profile-save`. Evidence: `an_agent_runs_the_managed_preview_of_its_workspace_and_children` (`backend/g5-api/tests/workspace_agent_parity.rs`).

### g5.preview.agent-access

- human-verified
- Source: the owner's answer "Add a task-bound, non-browser preview credential" (2026-10-26) to `agent-child-preview-access` in the question set `issue-710-owner-decisions`, which resolved the former `g5.preview.agent-child-access-question` ([request](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace)). The lifetime, header-only presentation, fetch-metadata refusal, WebSocket exclusion, reach under agent scope and never printing the credential are agent choices in the [design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-7-agent-preview-credential).
- Implemented: `g5_auth::PreviewAgentClaims` (`JwtMinter::mint_preview_agent`, `JwtValidator::validate_preview_agent`), the `x-g5-preview-credential` admission in `auth_middleware.rs`, the recheck `resolve_agent_preview_target` and header stripping in `routes/preview.rs`, the mint route `POST …/workspaces/:t/preview/credential` in `routes/workspace_agent.rs`, and `g5d workspace preview-fetch`. Evidence: `an_agent_fetches_its_childs_preview_with_a_task_bound_credential` (`backend/g5-api/tests/workspace_agent_parity.rs`) through the real relay to a scripted child guest; unit tests in `g5-auth` (`minter.rs`), `auth_middleware.rs`, `audit_log.rs` and `g5d` (`cli/workspace_agent.rs`). Not exercised: a deployed preview suffix reached from a real guest's network.

### g5.workspace.module-execution

- Amendment: [#966 design](../../pr_ontology_diff_design/issue-966/ontology-plan.md#guest-journal): paged replay from an offset index and amortized compaction (`g5d-dispatch/src/journal.rs`, covered by its journal tests).

- agent
- Source: [module audit](../../uplift/ben-ontology/investigations/chat-identity.md), [dispatcher journal design](../../../docs/dispatch-runtime.md) and recovered `67ad4b5d` supervision/pruning decisions.
- [ModuleLifecycle and ExecutionJournal](../../uplift/ben-ontology/quantitative-refinement/runtime.md) checked selected admission, cancellation, outcome retention, restart settlement and acknowledged-only pruning. An already reserved runner may race cancellation; filesystem persistence is a premise, and neither memory fallback nor restart proves physical child termination.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 54 (Q8).

### g5.lifecycle

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule; it names the consented-compute transitions the `g5.lifecycle.*` blocks describe.

### g5.lifecycle.start

- agent
- Source: [Start API contract](../../../docs/workspace-vm-api-v9.md) and [consent audit](../../uplift/ben-ontology/reviews/coverage-runtime.md#r-int-5).
- [Consent](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked owner/generation authority, bounded consent and refusal of unauthorized waking. This is logical admission, not physical VM uniqueness.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 55 (Q4).
- Amendment ([G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984)): the scoped exception is now the named [automatic continuation](#g5.lifecycle.start.automatic-continuation) consent; see its [design](../../pr_ontology_diff_design/issue-984/ontology-plan.md).

### g5.lifecycle.start.held-work

- human-provided
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 56 (TK-cell-47).
- Amendment ([G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984)): an automatic continuation start releases the same held work; the release already runs on any Ready (`release_tasks_after_resume`), so no new release path is added.

### g5.lifecycle.start.automatic-continuation

- agent
- Reason: owners leave agents working unattended, so a turn stalled by host maintenance silently loses hours, while compute must not run without a reason the owner gave.
- Decisions: the owner decided the Project Doc question set `chat-auto-resume` on 2026-10-31 ([G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984), [design](../../pr_ontology_diff_design/issue-984/ontology-plan.md)): scope, no replay, immediate start with an hour's wait, the chat notice with a live notification, the loss cap of two, the ordinary idle timeout afterward, and an organization setting on by default.
- Source: [G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984) asks whether to build chat auto resume when a host is rotated; the design recommends yes within this scope and records the alternatives.
- Reuses the drain interruption policy (`drain`/`rollout` interrupts re-queue the turn with their reason), the `vm_lost` hold, continuation attempts with native session resume ([g5.chat.recovery](../features/chat.g5.md#g5.chat.recovery)), the start-intent queue and the held-work release on Ready. New: the `automatic_continuation` start kind, the lifecycle reconciler pass that decides each ended incarnation (`workspace_vms.automatic_continuation`, migration 140), Agents' verdict on what the interruption left (`interrupted_work`), the restore sentence in the continuation instruction and the `automatic_continuation_enabled` organization key (migration 141).
- Platform-caused interruptions today: forced drains at the host credential cliff (rotation of an idle host retires only an empty host), operator and unreachable-host drains, forced host removal, lease expiry and missing inventory; a `rollout` stop reason exists but no production path issues it.

### g5.lifecycle.start.automatic-continuation.decision

- agent
- Mechanics: a start writes its decision in the same transaction as its consent, under the VM row lock, fenced on the incarnation being undecided and without live consent; other decisions use the same fence. A queued turn is stamped `vm_lost` with its incarnation's epoch at a loss release only when the released VM had reached Ready and ended `failed` or `lease_expired_during_stop`, so a clean stop's exit and a failed initial boot stamp nothing; the verdict reads holds and attempts only for the ended incarnation. Migration 140 backfills a decision for incarnations already ended.
- Evidence: `backend/g5-api/tests/api_automatic_continuation.rs` (lost VM starts once, cap refusal, drain not counting, owner Stop in its three forms, failed initial boot, incarnation binding, successive drains, a lease lost during a stop).

### g5.lifecycle.start.automatic-continuation.consent

- agent
- Mechanics: every path that ends a queued or held agent turn calls one release helper that locks the VM row while an automatic consent waits and cancels it when no qualifying turn remains; recording or replaying the consent makes the same check, and a fresh start whose turns are gone records nothing.
- Evidence: `api_automatic_continuation.rs` (lapsed consent, lapsed owner Start, replays, Cancel, chat Stop and withdrawal ending the last turn, clarification and unknown-delivery exclusions).

### g5.lifecycle.start.automatic-continuation.visibility

- agent
- Mechanics: the reconciler appends the notice through the working-agents notice path, stores a `workspace_lifecycle` notification and pushes it to the owner's user channel and their workspace-channel connections through the WebSocket hub.
- Evidence: `api_automatic_continuation.rs` (notice and notification after a start and after the cap).

### g5.lifecycle.stop

- agent
- Source: [nuclear Stop request and interpretation](../../chat_intent/nuclear-stop/request.md), commit `527886a5`.
- [StopWindow](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked bounded saving followed by termination. Progress assumes host reachability and an eventual exit receipt; RUNTIME-002 settlement fencing remains separate; [logical uniqueness](#g5.vm.physical-uniqueness-question) defines the custody guarantee without promising physical exit.

### g5.lifecycle.stop.window

- agent
- [Deactivation design](../../pr_ontology_diff_design/member-removal/ontology-plan.md#shutdown-and-concurrency) reuses the two-minute save window after immediate admission closure; `MemberDeactivated` uses the existing Stop executor, claims, settlement and receipts. The archived check below does not establish the membership barrier.
- Source: [human two-minute correction and accepted Stop design](../../chat_intent/nuclear-stop/request.md); the duration is explicit intent, with separate deadline and force-delete modes.
- [StopWindow](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked transient retry, deterministic failure, expiry and completed-receipt protection. Host reachability and eventual exit remain progress assumptions.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 57 (Q3).

### g5.lifecycle.member-deactivation

- human-provided
- Source: running-execution, queued-work, retry and reactivation requirements in the [request](../../chat_intent/member-removal/request.md#human-instructions).
- Implemented with a durable membership cleanup identifier and bounded lifecycle discovery. Runtime admission and database VM guards close access before the first Stop is staged; cleanup survives reactivation. [Evidence](../../pr_ontology_diff_design/member-removal/ontology-plan.md#implementation-evidence).

### g5.lifecycle.member-deactivation.recovery

- agent
- [Design and simpler alternatives](../../pr_ontology_diff_design/member-removal/ontology-plan.md#why-a-durable-cleanup-marker-is-needed). A separate cleanup identifier preserves pending work across reactivation without atomically locking an unbounded workspace inventory.
- The cleanup identifier is the [execution hold](#g5.lifecycle.stop.by-user) that stop by user also sets ([design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#state-and-ownership)).
- [Concurrency and physical completion limits](../../pr_ontology_diff_design/member-removal/ontology-plan.md#shutdown-and-concurrency). Implemented with owner/cleanup-identifier fencing and confirmed-termination requirements. PostgreSQL/HTTP/lifecycle tests cover early reactivation, queue paging, repeated admission failures across worker recreation, stale retry generations and unreachable-host retries. Archived Consent/StopWindow checks do not cover the membership barrier or its races.

### g5.lifecycle.stop.by-user

- agent
- Source: [stop by user](../../chat_intent/issue-710-workspace-agent-parity/request.md#interpretation); generalizes [`g5.lifecycle.member-deactivation.recovery`](#g5.lifecycle.member-deactivation.recovery) and replaces the earlier subtree stop and fork freeze. [Design and alternatives](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#stop-workspaces-by-user).
- Implemented: `OrgService::stop_member_workspaces` sets the membership's `deactivation_cleanup_id` (the hold) without `deactivated_at`; migration `117_execution_hold.sql` lets the owner be held and adds the `execution_hold` stop reason, which `admit_stop` records under the membership lock. Callers: the person route, the agent's `stop-owner-workspaces` route, the Members page and Home's Stop my workspaces (`HomeStopMyWorkspaces`, the caller's own hold in the Running compute tile's header, where their running VMs are listed; while the roster reports it pending, the control opens progress instead of asking again; covered in `HomePage.test.tsx`).
- Evidence: `an_agent_stops_its_owners_workspaces_and_the_hold_clears_once_settled`, `people_stop_their_own_or_as_admin_anyones_workspaces_including_the_owner` (`backend/g5-api/tests/workspace_agent_parity.rs`). The first also forks during the pass and shows the fork stays stopped with its chat refused. Not covered there: an unreachable host, which relies on the unchanged deactivation cleanup and its tests.

### g5.lifecycle.stop.by-user.release

- human-verified
- Source: the earlier "starting again implicitly re-enables" instruction, [carried over](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-instructions-recorded-by-earlier-sessions); clearing on settlement is agent design.

### g5.lifecycle.stop.claim

- agent
- From the incident in [G5-Lean#566](https://github.com/G5Labs/G5-Lean/issues/566): a QA Stop whose flush completed in about seven seconds reached `stopped` 5m21s later, matching the former fixed 320-second recovery timer set at admission; the Stop request had no completion audit, consistent with its handler future being dropped with the owner's connection. Replaces that timer, which had to outlast any legitimate execution because nothing distinguished a live executor from a dead one.
- Implemented · `workspace_vm_service.rs::stop_with_authority` runs the admitted stop on its own task (`execute_admitted_stop`) and only awaits it, so a dropped request no longer cancels it. Admission (`admit_stop`, `claim_stop`) sets `stop_retry_at` `STOP_CLAIM_LEASE_SECS` (30 s) ahead under the row lock and returns that value; `StopClaim` renews it every `STOP_CLAIM_RENEW_SECS` (10 s) through `renew_stop_claim`, a compare-and-set that replaces exactly the value this executor wrote last, so whichever order a renewal and a yield commit in, the renewal can neither postpone the yield nor extend another executor's claim (found by the local Codex review). A renewal that finds another value, or that has not succeeded by the time the executor's own measure of the lease ends (counted from before it asked the database, so never later than the stamped lease; the database failed or never answered; found by the second and third Codex rounds), marks the claim lost and `execute_admitted_stop` stops the execution at once, reporting the row as it stands; the claim holder's replay of the keyed guest and host steps lands on the same results. `reconcile_stopping` still selects only due rows and logs how long the claim had lapsed; a re-entered admission for an operation that is not yet due returns `StopAdmission::Claimed`. The host-command stage relies on the claim instead of its own budget; the host answers a resend under the same key with `Busy` or its recorded result. `log_stop_phase` reports every stop's stages (formerly transfer only). `backend/g5-api/tests/api_workspace_vms.rs`: `owner_stop_outlives_the_connection_that_requested_it` (the HTTP request is aborted while the guest holds the flush; the claim is renewed past the renewal interval, the stop finishes without the reconciler, keeps its receipt and releases the VM once), `abandoned_stop_is_resumed_once_its_claim_lapses` (an admitted, abandoned stop is left alone while claimed and resumed by the reconciler after the claim lapses), `repeated_stop_requests_join_the_live_execution_and_release_once`, `an_executor_that_loses_its_claim_stops_and_the_claim_holder_finishes` (the claim is taken while the guest holds the flush; the executor stops without a host command and the reconciler completes the same operation with one release), `a_renewal_that_never_answers_loses_the_claim_when_the_lease_ends` (another transaction holds the row lock so renewals wait; the executor stops at lease end and the reconciler finishes), and `forced_delete_stop_is_claimed_on_the_database_clock_while_it_executes` (a force Delete is claimed on the database clock, with the trigger-visible immediate supersession kept). Both the wait for a renewal and the renewal itself end at the executor's lease deadline, so a lapsed claim ends its execution no later than the database lease; another holder can only claim a due row, so the two never work at once beyond the skew between the executor's clock and the database's, bounded by the keyed steps and incarnation fencing. Reviewed locally with Codex over five rounds (2026-10-15); approved on the fifth.

### g5.lifecycle.transfer

- agent
- Expands: [lifecycle](#g5.lifecycle).
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule.
- Children: [admission](#g5.lifecycle.transfer.admission).

### g5.lifecycle.transfer.admission

- agent
- Expands: [transfer](#g5.lifecycle.transfer).
- Source: [G5-Lean#626](https://github.com/G5Labs/G5-Lean/issues/626) findings D7 (competing pending transfers) and S14 (stale predicate read while waiting for the workspace lock); [design](../../pr_ontology_diff_design/issue-626/ontology-plan.md).
- Implemented in `workspace_vm_service.rs::transfer` and `record_start_intent`; covered by `api_workspace_vms` transfer tests.
- Refines: [transfer](#g5.vm.transfer), [Start consent](#g5.lifecycle.start).

### g5.lifecycle.idle

- human-provided amendments: [owner inactivity request and explicit preview-traffic exception](../../chat_intent/optimize-vm-idle-detection/request.md), [design](../../pr_ontology_diff_design/optimize-vm-idle-detection/ontology-plan.md).
- Retains [preview idle Stop request](../../chat_intent/preview-idle-stop/request.md), commit `75b12ed0`, work exemptions and configured timeouts. Supersedes open-terminal presence as activity.
- Archived [IdlePreview and TerminalIdle](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) retain activity revalidation evidence; the former connection-lifetime terminal policy is no longer governing.

### g5.lifecycle.idle.observability

- human-provided: [owner inactivity request](../../chat_intent/optimize-vm-idle-detection/request.md).
- Human follow-up on PR #789 requires SQL-bounded selection and rate-limited exemption diagnostics; implementation remains in the [same design](../../pr_ontology_diff_design/optimize-vm-idle-detection/ontology-plan.md).

### g5.preview.service.stop-reason

- agent
- Source: [preview idle Stop request](../../chat_intent/preview-idle-stop/request.md).
- [IdlePreview](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md) checked lifecycle-reason propagation into outcomes, not browser rendering. This rule does not add optional preview cleanup during workspace Stop.

### g5.architecture.agent_execution_tree

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/agent-turn-release-leftovers/request.md).
- Preserves [cancellation](../features/chat.g5.md#g5.chat.cancellation). Source evidence retains an unproven post-reap provider-group-ID reuse window; the rewrite does not certify that race or the full live-provider exit matrix.
- The user's 2026-10-16 [guest authority decision](../concepts/security.g5.md#g5.security.guest-authority) confirms that this lifecycle mechanism is not a security boundary against the workspace owner.


### g5.preview.service.exit-diagnostics

- agent. The owner’s 2026-10-20 request that stderr show while a build fails (“std error should show when preview build failed too”) was delivered as per-line task events, withdrawn the same day after the [owner’s finding](../../chat_intent/preview-status-observation/request.md) that they blocked the shared reconciler, and is met by the [hosted terminal session](#g5.preview.output.live-tail) (owner decision in chat, 2026-10-20; [design](../../pr_ontology_diff_design/preview-status-observation/ontology-plan.md)).
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/bake-permission/request.md).
- Upstream process/API tests cover persisted diagnostics and framing. [Stop reasons](#g5.preview.service.stop-reason) remain distinct from application-exit evidence. The tail stays outcome-only: the guest-log mirror dropped in the 2026-10-20 revert is not restored.
- Implementation: `backend/g5d/src/cli/preview_serve.rs` (`forward_stderr` tees the pipe onto the hosted terminal and fills `StderrTail`; without `G5_APP_TTY_FD` the app’s stdout is opened on null). Tests: `preview_serve::tests::tty_mode_keeps_the_failure_tail_and_marks_the_exit`, `without_the_tty_env_the_app_keeps_todays_streams`; `dispatch_modules::workspace_preview_app_output_cannot_preempt_or_hold_the_outcome_channel` asserts that the journal holds no `module.output` event and that neither stream reached the guest log.

### g5.preview.output

- agent
- Concept added for the [writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) ID-level rule; the runtime group owns the `g5.preview.output` level.
- Children: [live tail](#g5.preview.output.live-tail), [viewer](#g5.preview.output.live-tail.viewer).

### g5.preview.output.live-tail

- human-verified
- Expands: [preview output](#g5.preview.output).
- Source: owner decision in chat, 2026-10-20 (“reuse the same terminal mechanisms”; the PTY-hosted design chosen over a no-PTY output session), recorded with the rejected alternative in the [design](../../pr_ontology_diff_design/preview-status-observation/ontology-plan.md) for the [finding](../../chat_intent/preview-status-observation/request.md).
- Settled with the owner: watching is governed by the same activity guard as an open terminal; a failed run’s output is viewable only while its VM runs; the application runs on a real TTY; one viewer at a time; deploy order degrades to the previous behavior, never to a lost preview. Amended 2026-10-21 (agent): the PTY is not made the controlling terminal because the leader’s exit would SIGHUP the application before its SIGTERM handler ran (observed in CI as lost shutdown output); recorded in the design. Amended 2026-10-27 (agent): the settled wording “watching counts as activity like an open terminal” rested on the design’s assumption that the adapter’s heartbeat renews activity; it does not (`terminal_ws_adapter` records activity only for nonempty input, as [g5.lifecycle.idle](#g5.lifecycle.idle) requires), so the viewer rule now states the shared guard rather than a renewal; whether watching should keep a VM running is an idle-rule decision for the owner, not taken here; recorded in the design.
- Implementation: `backend/g5d-foundation/src/terminal/hosted.rs` and `TerminalManager::{host_session, attach_hosted, detach_hosted, resize_hosted, discard_hosted_where}`; `backend/g5d-dispatch/src/modules/runner.rs` (hosts `preview:<task id>` for a module requesting `preview.service`, passes the slave as fd 3 with `G5_APP_TTY_FD`); `backend/g5d-dispatch/src/server.rs` (`open_hosted_pty`, `relay_hosted_pty`); `backend/g5d/src/cli/preview_serve.rs` (`AppTty`, `Placement::TtySession`, stage markers); `backend/g5-contracts/src/terminal.rs` (`TerminalServerMessage::Closed`, `HOSTED_TERMINAL_PREFIX`). Transport, API routes and the host relay are unchanged.
- Tests: `g5d-foundation` `terminal::tests::hosted_sessions::*`; `preview_serve::tests::tty_mode_*`; `dispatch_modules::workspace_preview_app_output_streams_on_a_hosted_terminal_not_the_journal` (journal lifecycle-only, live attach, stale-offset reset, input dropped, close detaches, Close output in the session, late attach ends with `session_ended`, replacement by the next preview, no creation from an interactive `open`); `api_workspace_vms::owner_terminal_relays_a_hosted_preview_identity_unchanged`; Playwright `workspace-preview.spec.ts` run-log case.
- Gap: the retained output is dispatcher memory; a dispatcher restart within the incarnation loses it (the preview is then settled as interrupted). Capacity is `G5D_PREVIEW_OUTPUT_RETAIN_BYTES` (64 KiB to 64 MiB, default 4 MiB).

### g5.preview.output.live-tail.viewer

- human-verified
- Source: the same owner decision as [g5.preview.output.live-tail](#g5.preview.output.live-tail) (“reuse the same terminal mechanisms”), split out because the viewer rules can change without moving where the output lives.
- Amended 2026-10-27 (agent) after the owner’s QA finding ([request](../../chat_intent/preview-ux-post-improvements/request.md), [design](../../pr_ontology_diff_design/preview-ux-post-improvements/ontology-plan.md)): two browser tabs on one preview log superseded each other every 1–6 s because the replaced viewer’s stream closed with no reason and the browser’s reconnect policy reattached it. The replaced viewer is now told (`closed`/`superseded`); a shell stream is unchanged. Rejected alternative: several simultaneous viewers with per-viewer cursors; the ring already serves by offset, but the single attachment generation also decides whose resize applies, and the owner’s “one viewer at a time” decision stands until re-decided.
- Implementation: `backend/g5d-dispatch/src/server.rs` (`open_hosted_pty`, `relay_hosted_pty`, hosted branch of `finish_pty`: `closed`/`superseded` on `PtyExit::Superseded`); `TerminalManager::{attach_hosted, detach_hosted, resize_hosted}`; `backend/g5-contracts/src/terminal.rs` (`TerminalServerMessage::Closed`, `TerminalClose::{SessionEnded, Superseded}`, `HOSTED_TERMINAL_PREFIX`); `frontend/src/components/execution/useWorkspaceTerminalSocket.ts` (`superseded`, `takeOver` at the kept offset), `WorkspaceTerminalPanel.tsx` (Take over), `terminalViewer.ts`; the activity guard is the adapter’s unchanged heartbeat (`backend/g5-api-routes/src/routes/mod.rs` `terminal_ws_adapter`). Tests: `dispatch_modules::workspace_preview_app_output_streams_on_a_hosted_terminal_not_the_journal` (the superseded stream ends with `closed`/`superseded`, the new viewer keeps streaming), `terminal::tests::hosted_sessions::{hosted_session_drops_input_and_detach_does_not_end_it, hosted_sessions_are_outside_the_shell_limits_and_listing}`, `api_workspace_vms::owner_terminal_relays_a_hosted_preview_identity_unchanged`, `useWorkspaceTerminalSocket.test.tsx` and `WorkspaceTerminalPanel.viewer.test.tsx` superseded cases, Playwright `workspace-preview.spec.ts` two-tab case.


### g5.lifecycle.idle.settings

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/admin-page/request.md).
- The later owner decision supersedes Base-only inheritance by copying. Admission and activity still follow [idle reclamation](#g5.lifecycle.idle).
- [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624) (human-verified) adds the personal-workspace idle default exception; [design](../../pr_ontology_diff_design/issue-624-personal-workspace/ontology-plan.md). Implemented; locations and tests are in the [personal workspace metadata](../features/personal_workspace.g5.md#metadata).

### g5.vm.request-isolation-question

- agent; uncertain
- Source and implications: [runtime audit](../../uplift/ben-ontology/investigations/runtime.md), RUNTIME-003 and dispatcher filesystem dispatch.
