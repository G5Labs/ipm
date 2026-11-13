# Chat

<a id="g5.purpose.agents"></a>
[I · g5.purpose.agents]

Chat's goal is to support people asking coding agents to investigate and change their workspace through conversations.

<a id="g5.chat.workspace"></a>
[I · g5.chat.workspace]

Each chat belongs to a workspace.  

## Launch configuration

<a id="g5.chat.configuration"></a>
[I · g5.chat.configuration]

Model configuration selects the agent, provider account, endpoint, model and reasoning effort.  

<a id="g5.chat.account-credential-shape"></a>
[D · g5.chat.account-credential-shape]

A credential submitted for a provider account is trimmed and must be one token without spaces or line breaks; where the provider publishes a shape (Claude subscription `sk-ant-oat01-` from `claude setup-token` and Anthropic API key `sk-ant-api03-`, each followed by base64url characters; OpenRouter `sk-or-v1-` followed by hex; OpenAI `sk-…`), create and update refuse a value that does not match and the refusal names the account kind the pasted value belongs to. Only what a wrong paste would violate is pinned: the prefix family with its version digits left free (an Anthropic API key is any `sk-ant-<family>-` other than the OAuth token families `oat` and `ort` and the Admin key family `admin`, none of which can call Messages), the character class, and a floor of 20 body characters far below every real length, so a provider rotating its format is not locked out. An API-key kind pointed at a custom base URL carries that gateway's key, whose shape G5 cannot know, so the shape applies only on the provider's own host (same scheme and host name, default port); the compatible-endpoint bearer has no shape. The authentication-failure notice makes the same distinction and names the gateway's base URL instead of Anthropic. A credential already stored is not re-checked at launch, so an older row keeps working until it is edited. When Claude Code reports an authentication failure, the chat notice names the frozen account kind whose credential the provider rejected and how to replace it in Settings → Accounts, instead of Claude's own "run /login" advice, which cannot be followed inside a workspace VM. Reason: on QA (2026-10-27) an Anthropic API key pasted into a Claude subscription account passed validation and failed every turn with "Not logged in · Please run /login".

<a id="g5.chat.native-catalog"></a>
[D · g5.chat.native-catalog]

The shared provider catalog owns offered route-specific models and per-model efforts
for UI and API/backend consumers. Required tests keep their projections synchronized
and exercise every offered model, native mode and effort on each exact runtime pin,
checking effective settings and outbound requests. Known Claude OpenRouter ids carry
native capability mappings so selected efforts cannot silently disappear. Native
mode inventory grants no additional G5 interaction support; account-specific model
access requires separate live-provider validation.
Offer only modes the production dispatcher can execute; the binary gate may exercise
a broader native inventory without making those modes launch choices.
Models without an effort control remain selectable with an explicit read-only
absence state; no native effort argument is sent. New resolutions reject efforts
outside a known model's set, while historical frozen resolutions retain their
reserved validation contract.

The Claude lineup offers one plain id per family, listed newest first by
Anthropic's release day, which the catalog records per family and the composer
picker follows. Every offered family runs its 1M window without Claude Code's
`[1m]` suffix, so no suffixed variant is offered; a saved suffixed selection stays
resolver-known with its label, runs on the direct route as selected and collapses
to the family slug on OpenRouter as its plain id does. The bundled chat default is
the current Opus, Opus 5.5, in its plain form, represented once in the picker. A
persisted choice equal to an earlier bundled default (Opus 4.8's slug or its
suffixed direct id) was never changed by the person, so it follows the current
default; any other saved choice is kept. Reason: owner request (2026-11-09) with
Fable 5.2's release, see the [design](../../pr_ontology_diff_design/fable-5-2/ontology-plan.md).

With a named OpenRouter account, the Claude chat composer offers that route's
catalog families; a saved suffixed selection changes to the family's plain id with
a visible explanation, and account and model are confirmed or saved together.
Returning to a direct account keeps the model shown.

<a id="g5.chat.inheritance"></a>
[I · g5.chat.inheritance]

Settings inherit from organization to workspace to user, with more specific values taking precedence.  

<a id="g5.chat.launch"></a>
[I · g5.chat.launch]

Each launch can override inherited choices or use platform defaults.  

<a id="g5.chat.queued-launch"></a>
[D · g5.chat.queued-launch]

Each queued input retains its accepted launch preferences and any confirmed resolution.
Its later turn uses that input's choices; the preceding task is only a compatibility
fallback for historical inputs whose choices were not stored. Steering into an existing
attempt preserves that attempt's [frozen configuration](#g5.chat.frozen-attempt).

<a id="g5.chat.confirmed-launch"></a>
[D · g5.chat.confirmed-launch]

Preview and Start use the same scoped configuration resolver and require a matching
confirmed resolution. Preserve confirmation while the input waits; first admission
compares only effective provider, model, effort, permission mode and account kind,
scope and ID. Unrelated setting or secret changes do not invalidate those choices.
Admitted retries retain their frozen choices. Unconfirmed launches follow normal
resolution; current-owner credential authority still applies.

<a id="g5.chat.confirmed-launch-recovery"></a>
[D · g5.chat.confirmed-launch-recovery]

When changed confirmed choices refuse first admission, retain the editable, withdrawable
queued input and name the old and current values. Tell the owner that their next Send
will run it with current values and its own preferences, or they can withdraw it.
That Send clears stale confirmation on the failed, never-admitted input and runs it
ahead of the new message, without an automatic refusal loop.

<a id="g5.chat.model-env"></a>
[I · g5.chat.model-env]

Claude reads ANTHROPIC\_MODEL; Codex reads OPENAI\_MODEL.  

<a id="g5.chat.providers"></a>
[I · g5.chat.providers]

Claude Code or Codex are the primary agents the platform integrates with  

## Conversation and execution

<a id="g5.chat.authority"></a>
[I · g5.chat.authority]

Only the workspace owner, or their workspace agent [acting for them](../concepts/security.g5.md#g5.security.agent-workspace-scope), controls live agent interaction; authorized readers can read and search saved conversations without a VM.  

<a id="g5.security.agent-chat-control"></a>
[D · g5.security.agent-chat-control]

Agent-facing platform chat controls target a workspace within the agent's [scope](../concepts/security.g5.md#g5.security.agent-workspace-scope) other than its own. The rule covers chat creation, follow-ups, rename or fork into a chat, queued-input changes and chat-task control, including launchers with those effects. Self reads, native local sub-agents and runtime session persistence are unaffected.

<a id="g5.chat.queue"></a>
[I · g5.chat.queue]

Follow-up inputs queue without interrupting the current attempt and remain editable or withdrawable until reserved, except a [redelivery-authorized revision](#g5.chat.uncertain-delivery), which is withdrawable only. Eligible [native Claude steering](#g5.conversation.delivery.steering) may reserve and deliver an input within the running attempt.

In workbench and Home, submission feedback belongs in the existing queue, not a
speculative transcript bubble. Keep it visible while submission and queue/history
reads are pending; reconcile by the server input identity when known. Pending
feedback grants no edit or withdraw permission. Until the send response supplies
the input ID, temporary duplicates remain possible; a lost response leaves
acceptance undetermined. The [frontend-only scope](../../pr_ontology_diff_design/issue-1069/ontology-plan.md)
does not fully satisfy the original exact-once criterion before that response.

<a id="g5.chat.start-and-send"></a>
[D · g5.chat.start-and-send]

While the owner's VM is stopped or failed, every owner surface that ends in a chat request stays usable instead of greyed, and its clearly labelled compound action records [Start consent](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start) together with the request in one owner request: the composer's Start and send, a [chat action](ui.g5.md#g5.workbench.chat-actions) seeded into it, the explainer dialog's Start and generate, Implement from an issue, Prepare to Ship's [Start and ship with agent](../software_architecture/git.g5.md#g5.git.pr-auto-merge.with-agent), the conflicts dialog's [Start and resolve with agent](../software_architecture/git.g5.md#g5.git.stopped-rebase.resolve-with-agent), and Start and answer on a held agent question or approval. A queued input is bound to that consent: if the consent is cancelled, superseded or expires before compute is ready, the input is withdrawn in the same transition, so nothing stays queued; once compute is ready it is an ordinary [queued input](#g5.chat.queue). An accepted answer re-queues its held turn, which runs once the VM is up as [held work](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start.held-work) does; a Start that ends first leaves that turn queued for the next Start, like a message queued before a Stop. Model, Effort and Account choices need no VM: they are kept with the composer and travel with the next request. Start progress shows inline in the conversation. Controls that still need the running VM itself (terminal, file and document writes) stay visible and disabled, and trying one reveals the reason with Start in reach ([unavailable actions](ui.g5.md#g5.ui.unavailable-actions)); the preview has its own compound action, [Start workspace and preview](../software_architecture/workspace_vm.g5.md#g5.preview.start-and-preview), built on the same consent binding. Reason: owner request (2026-11-03) that a send on a stopped workspace attach the VM and queue the message, and that nothing which ends in the chat be greyed out; the personal composer's Start and send ([g5.personal.start-and-send](personal_workspace.g5.md#g5.personal.start-and-send)) already worked this way and is the one path reused.


<a id="g5.chat.delivery"></a>
[I · g5.chat.delivery]

Input becomes delivered history only after confirmed handoff to the provider.  

<a id="g5.chat.uncertain-delivery"></a>
[I · g5.chat.uncertain-delivery]

Uncertain initial-input delivery remains explicit; possible duplicate redelivery requires the owner’s choice, and that choice authorizes the exact possibly delivered revision: the re-queued input stays withdrawable but is not editable, and different content needs a new message. [Native steering fallback](#g5.conversation.delivery.steering-recovery) is the explicit exception: its ambiguous send may remain queued with accepted duplicate risk.


<a id="g5.conversation.late-facts"></a>
[D · g5.conversation.late-facts]

Keep a positively established input acceptance that arrives late, from an attempt that already ended or from a replaced VM incarnation, as delivery history for its own reservation, deduplicated by execution, reservation and delivery generation. It never delivers a newer reservation, advances another attempt's watermark or restarts work. If that input was left with [unknown delivery](#g5.chat.uncertain-delivery), the late acceptance resolves it as delivered; if the owner has already queued it for redelivery, the acceptance stays history and the owner is told the redelivery may duplicate it. Discarding such evidence would leave a delivered input unknown for good.

<a id="g5.chat.completed-output"></a>
[I · g5.chat.completed-output]

Completed assistant output is saved in order and remains visible after interruption or failure.  

<a id="g5.chat.replay"></a>
[I · g5.chat.replay]

Live streaming and saved history preserve the same message identities without replay duplicates. Both carry each event's execution identity (task, attempt and journal sequence) and the message or block it belongs to, and both reduce it with the same rules, so a live row and its later saved row are one fact: the saved row replaces the live one, a duplicate or reordered live notification changes nothing, and a history refresh never removes live output beyond the prefix it returned. Live output may precede saved history; it is never silently skipped.

<a id="g5.chat.presentation"></a>
[I · g5.chat.presentation]

Show user prompts in right-aligned bubbles and assistant responses and timeline
activity in left-aligned bubbles with a distinct, theme-aware background color,
consistently during streaming and after completion, Stop, failure or history reload.
Each bubble is as wide as its content, growing up to eight inches or the column,
whichever is narrower, and wrapping beyond that.
Equivalent Claude and Codex content uses shared turn
projection and rendering, including sparse saved messages. Tool disclosures, code,
artifacts and actionable request cards may have their own containers; controls reflect
the actual request and runtime capabilities.

<a id="g5.chat.presentation-owner"></a>
[D · g5.chat.presentation-owner]

The shared assistant turn owns block presentation and approval delivery state. Native
and legacy input adapters preserve output provenance and chronological interleaving;
message, tool and request identities preserve disclosure choices and question drafts
when history detail changes. Reuse the [output adapters](#g5.chat.adapters) and
[error notices](#g5.chat.error-notice) rather than parallel provider rendering paths.

<a id="g5.chat.file-citations"></a>
[I · g5.chat.file-citations]

In chat replies, Claude and Codex agents cite workspace files as
[file links](documents/files.g5.md#g5.documents.file-links), so a person opens them with a
click; this overrides a provider's default to write `path:line`, which chat does not make
clickable. A cited line goes in the [line fragment](documents/files.g5.md#g5.documents.file-links.line),
and a code file's repository is named only [where several code repositories need it](#g5.chat.file-citations.repo-qualifier).
The link text is the file name, or a path where the name would be ambiguous. Only chat
replies follow this: an agent outside G5 chat keeps `path:line`, documents keep their
relative links, GitHub text and email keep plain paths, and files a link cannot open are
left to the agent. The [packaged agent instructions](#g5.chat.file-citations.instructions)
carry the rule. A [personal workspace](personal_workspace.g5.md#g5.personal.file-citations)
is the exception.

<a id="g5.chat.file-citations.instructions"></a>
[D · g5.chat.file-citations.instructions]

The packaged agent instructions state the [citation rule](#g5.chat.file-citations) once,
for Claude and Codex alike; personas, workbench prompts and skills do not repeat it. No
chat-only delivery carries it: the wording names G5 chat, so sessions outside chat read
the same rule and rely on the agent to recognise that they are not chat.

<a id="g5.chat.file-citations.repo-qualifier"></a>
[I · g5.chat.file-citations.repo-qualifier]

A code-file [citation](#g5.chat.file-citations) names its repository, by
[id](#g5.chat.file-citations.repo-id), only when the workspace has more than one code
repository; Project Docs links never do. An unqualified code link already in chat history
stops opening once the workspace gains a second code repository, which is accepted.

<a id="g5.chat.file-citations.repo-id"></a>
[D · g5.chat.file-citations.repo-id]

A [citation that names its repository](#g5.chat.file-citations.repo-qualifier) uses the
repository id. The id is the one identifier the link accepts that the agent can always read in the workspace VM: it ends
each checkout's `origin` URL and is listed in `/work/.g5/layout.json`. The G5 repository
name is not shown there, and checkout folder names follow the workspace's root paths,
which the link does not accept. The link text hides the unreadable id.

<a id="g5.chat.view-state"></a>
[D · g5.chat.view-state]

Bounded tab-local storage retains question drafts and disclosure choices across
reload, scoped to message/tool or native request execution identity. Empty defaults
must not displace saved choices. This disposable view state never proves request
delivery; native request records remain authoritative for that state.

<a id="g5.chat.history-window"></a>
[D · g5.chat.history-window]

Saved transcript messages and timeline activity load newest first in independent
bounded windows. A turn shows activity only within the contiguous loaded region;
where its activity begins below that region, the turn shows a not-loaded marker
that loads older activity when activated or scrolled into view, and keeps the
reading position. Saved reply text never fills the unloaded region, so a turn never
looks complete without its tools, including a turn with [no loaded activity](#g5.chat.history-window.unloaded-text).
A failed load leaves the marker with a retry.

<a id="g5.chat.history-window.unloaded-text"></a>
[D · g5.chat.history-window.unloaded-text]

A turn with no loaded activity shows its prompt, the not-loaded marker and its terminal
state (Stopped or an error). Its saved reply text, artifacts and citations appear once its
activity loads, so text and tools always appear together in one chronology.

<a id="g5.chat.history-refresh"></a>
[D · g5.chat.history-refresh]

Refreshing the newest activity window never removes loaded history. A refresh is
one newest-window read merged into the held rows; when it no longer overlaps them,
it reads back until it does, keeping that progress across interrupted reads. Held
rows are dropped only when the server's history ends without them. Loading older
activity never re-reads the pages already held, except that a refresh reads back to
a held native request that is still open, including one the browser holds only as a live
row (which carries no status until its saved copy is read), because its status is read with its row
rather than journaled; a reply or invalidation row, or its attempt's outcome, ends that.

<a id="g5.chat.native-permission-actions"></a>
[D · g5.chat.native-permission-actions]

The current native permission reply route supports allow/deny and optional updated
input. Its shared card omits legacy interrupt and persistent permission-rule actions,
which that route cannot deliver. Preserve the [provider capability boundary](#g5.chat.native-requests).

<a id="g5.chat.native-requests"></a>
[I · g5.chat.native-requests]

Claude supports native questions and permission requests; Codex uses a noninteractive prompt-and-output transport.  

<a id="g5.chat.controls"></a>
[I · g5.chat.controls]

Stop ends the current attempt; Continue starts another attempt of a held logical task; Cancel ends the logical task.

An ordinary chat's turn is its own logical task, so Stop cancels it and it cannot be continued; the next message starts a new turn. Stop instead pauses a platform-started turn, such as a conflict resolution, and holds it for Continue or Cancel, because cancelling it would leave the operation that started it following a cancelled step.

Repeated requests for one named turn converge on one cancellation intent and one queue transition: a running turn keeps its single durable cancel intent, a queued or held turn ends once and later requests report that it already ended, and a Stop naming a turn that already ended never reaches its successor. Stop answers with the runtime's disposition — signalling, already finished, cancelled — or reports that the committed intent is still undelivered, which the follower redelivers; only a request whose intent did not commit fails. The follower re-reads the committed intent when poked, on a fixed cadence and again after a read that failed, and redelivers it beside the live reader one call at a time under the [bound on guest calls](#g5.chat.journal), so a transient failure strands neither the intent nor the stream. The chat shows `Stopping…` as soon as a Stop is sent and sends no second request for that turn while one is in flight, shows that output is being finished when the execution had already ended, preserves the actual outcome rather than overwriting success with cancelled, and offers Retry on failure. The workbench and Home chats behave alike.

<a id="g5.chat.control-locks"></a>
[D · g5.chat.control-locks]

Agent-task controls — human and agent Stop, workflow Pause, task Cancel and Continue, input withdrawal, the cancel of an unknown input and the native-reply claim — retain shared workspace authority from admission through commit and take only the session, task, attempt and request locks their transition needs; none upgrades the workspace row to an exclusive lock, and the operation takes the shared lock itself so a direct service caller gets the same guarantee. Sibling paths that do mutate VM state lock the VM row exclusively with the workspace shared, and every preview writer — start, cancel, and the readiness and outcome projection that settle its intent — takes the VM row before the task row, the order a VM release uses, so none of them can wait on another; transfer, stop, delete and placement still take the workspace exclusively, so a held share excludes them. Reason: a control that held the share and the session advisory lock, then waited to upgrade behind a second control's share, deadlocked both ([#966](https://github.com/G5Labs/G5-Lean/issues/966)).

<a id="g5.chat.cancel-outcomes"></a>
[D · g5.chat.cancel-outcomes]

A cancel request does not decide how its task settles. An attempt that succeeds after the owner asked to cancel settles `succeeded`, and one that fails, times out or exceeds a limit settles `failed`, not `cancelled`; the cancel is still honoured, because nothing continues. The exception is an attempt whose input is of [unknown delivery](#g5.chat.uncertain-delivery): a pending cancel settles it `cancelled` even if it failed, since the failure may be the cancel itself. Reason: the actual outcome is the truer record.

<a id="g5.chat.fork"></a>
[I · g5.chat.fork]

Forking a conversation carries selected delivered history into a fresh session.  

<a id="g5.chat.persistence"></a>
[I · g5.chat.persistence]

Chat sessions, transcript messages and attachment records are durable platform records in the platform database.  

<a id="g5.chat.ownership"></a>
[I · g5.chat.ownership]

ChatService owns conversations; TaskService owns queued inputs, execution attempts and task outcomes.  

<a id="g5.chat.creation"></a>
[I · g5.chat.creation]

Session, task and initial input creation share one transaction.  

<a id="g5.chat.send-acceptance"></a>
[D · g5.chat.send-acceptance]

Committing Send establishes its accepted input identity. Subsequent steering completion
survives the HTTP request and records only evidence for that authorized input under
tenant service authority. Completion failure or changed owner authority cannot turn
acceptance into a rejected Send; return the accepted identity and original outcome
without waiting for steering completion or reading newly inaccessible chat state.

<a id="g5.chat.creation.idempotency"></a>
[D · g5.chat.creation.idempotency]

A request key identifies one chat start or history fork for the submitting person and organization. Identical retries return the original session and any created input or task; reusing the key with a different workspace, target, content, attachment or launch/persona choice conflicts. Resolve mutable persona instructions and history selections once, after claiming the request. Lost responses do not change its identity; while the request remains retryable, refuse to recreate deleted results.

<a id="g5.chat.dispatch"></a>
[I · g5.chat.dispatch]

Dispatch reserves input without network calls, commits the reservation, invokes the runtime, then applies the result.  

<a id="g5.chat.frozen-attempt"></a>
[I · g5.chat.frozen-attempt]

Each attempt freezes its initial input revision, attachments, provider configuration, native transport/module contract, permissions and execution limits. Reconstruction and retry retain the reserved contract, including historical transports. [Native steering](#g5.conversation.delivery.steering) binds additional immutable input revisions individually without replacing that frozen configuration.


<a id="g5.chat.attachment-delivery"></a>
[D · g5.chat.attachment-delivery]

Deliver the [reserved typed input](#g5.chat.frozen-attempt) through the provider's supported encoding, verifying attachment identity, authorization and content integrity first. Missing, corrupt or unsupported content fails explicitly before delivery is claimed; recovery may reference prior attachments as history but must not replay their bytes as a new user action.

<a id="g5.chat.runtime"></a>
[I · g5.chat.runtime]

Chat execution invokes the agent.turn module through the Workspace Runtime facade. One production supervisor and dispatcher lifecycle owns admission, deadlines, output bounds, cancellation, process custody, journaling and session files; provider adapters reuse those owners.

<a id="g5.chat.incarnation"></a>
[I · g5.chat.incarnation]

Runtime requests identify the current workspace, VM incarnation, execution attempt and authorized actor.  

<a id="g5.chat.adapters"></a>
[I · g5.chat.adapters]

Provider adapters translate native Claude and Codex output into shared delivery, messages, tool activity, usage, questions, execution events and outcome records. Native thread and turn identities bind only to their exact task, attempt and VM incarnation; wrong identities and duplicate frames cannot deliver input, duplicate output or end another turn.

<a id="g5.chat.accepted-input"></a>
[I · g5.chat.accepted-input]

Confirmed input acceptance creates the delivered user message and assistant placeholder atomically. For Codex App Server, only the matching successful initial turn/start response with a valid native turn identity confirms acceptance; initialization, thread creation and request writes do not. When the provider may have accepted input but that evidence is missing, follow [uncertain delivery](#g5.chat.uncertain-delivery), without a speculative history claim. Claude retains its native write/flush delivery contract.

<a id="g5.chat.journal"></a>
[I · g5.chat.journal]

Journaled events replay with stable identities so retries cannot duplicate transcript output. Applying authoritative terminal execution evidence is durable and idempotent; native completion notifications do not themselves settle tasks or advance their queue. The guest journal is the durable recovery source for an attempt's output until the platform database holds it: the API persists history in [coarse batches](#g5.chat.history-batches), acknowledges only a committed contiguous prefix, cumulatively, and keeps retrying the acknowledgement for an attempt that already ended while its VM incarnation lives, on its own task beside the reconciler tick. Every guest call fences its incarnation with a pooled connection for the whole round trip, so the follower's calls beside the fetch — acknowledgements (one per attempt, carrying its latest prefix), status probes of an idle attempt and control redeliveries — are bounded together to a quarter of the runtime database pool, fetches to half of it and never more than sixteen, so the two together leave the pool room on its default size as on the deployed one, and a probe that fails or runs out its budget is repeated at a growing interval; a guest that accepts calls but answers slowly therefore holds neither the tick nor other workspaces' turns. The guest prunes only acknowledged prefixes and retains sequence watermarks. Browser receipt never advances the watermark or authorizes pruning. Live delivery is [independent of that persistence](#g5.chat.live-delivery).

<a id="g5.chat.live-delivery"></a>
[D · g5.chat.live-delivery]

Live text and tool activity reach the browser from the guest journal's tail without waiting for the history transaction, its commit or the guest acknowledgement. One process-local follower per live attempt fetches journal pages (woken by the host's content-free [module-event hint](../software_architecture/hosts.g5.md#g5.runtime.module-event-hints), otherwise by a one-to-two-second poll), binds every event to the admitted task, session and VM incarnation it was claimed under rather than to anything in the payload, derives the owning message from the attempt's deterministic segment identities without a database read, publishes on the existing task and chat channels under the existing live-access policy and per-message membership check, and only then buffers the event for history. A failed authority check stops forwarding. The follower retains at most 4,096 events or 8 MiB of unpersisted tail per attempt; past that it drops the buffer and rebuilds it from the journal, so a slow database never blocks the provider and no range is silently lost. A live row of a durable control boundary — an actionable provider request above all — is provisional: the browser shows it but cannot act on it until the batch commits, when the stored row is published again under the same id, carrying the request's status or standing as the error row projection made of it, and replaces the provisional copy. Fan-out stays process-local, which the one-replica API deployment allows; the database remains the authority for task and control transitions. Reason: in QA an agent finished 51 seconds before its last delta was shown because each delta waited on a transaction, a guest acknowledgement and a journal rewrite ([#966](https://github.com/G5Labs/G5-Lean/issues/966)).

<a id="g5.chat.history-batches"></a>
[D · g5.chat.history-batches]

Routine transcript output (`message.*`, `thought.*`, `tool.*` and notices) is persisted in one transaction per batch: a batch waits up to **five seconds** from its first unflushed event, or closes earlier at 256 events or 512 KiB, and a backlog that is already fetched drains in consecutive bounded batches without the wait. Every other event kind is a durable control boundary — confirmed input delivery, an actionable request, provider session evidence, the terminal outcome, preview readiness — and flushes the batch at once together with everything before it, since a boundary cannot overtake earlier events. A batch locks the attempt once, inserts its rows in one statement with each event's receipt time, refreshes each touched assistant segment once, settles on the terminal event and advances the persisted prefix once; event rows keep their original identities and shape. VM drain and retirement persist the retained tail before storage is released. The five-second interval is the owner's explicit choice over a sub-second timer; the capacity limits are safeguards, not a replacement cadence. A batch is background work: it waits a bounded two seconds for the session, VM, task and attempt locks and otherwise yields and retries from the buffer, so a stalled write never holds a Stop or a send behind it. The guest acknowledgement after a commit runs apart from the next batch; a lost one is covered by the next cumulative one or by the [retry scan](#g5.chat.journal).

<a id="g5.chat.live-tail-recovery"></a>
[D · g5.chat.live-tail-recovery]

Refresh, reconnect and a second browser recover a running turn as the saved transcript and timeline plus the attempt's unpersisted tail, read from the follower under the session's read authority. The tail read states whether it is complete from the persisted watermark, catching up (no follower in this process yet, or its buffer is being rebuilt) or absent; a catching-up tail shows the saved prefix with an explicit catch-up state and is retried, never shown as complete. The browser merges by execution identity and sequence under the [shared reduction rules](#g5.chat.replay), re-reads saved history when persistence moved past what it holds, and drops a live row only once its own saved copy is held: a saved row beyond its sequence proves the row is saved, not that the browser read it, so the earlier output of a turn whose refresh met only the newest page stays in view until a read brings its saved copies. Live rows are placed into the held window once per animation frame, so a burst costs one render, not one per row, and each render's cost [follows what changed](#g5.chat.live-tail-recovery.render-cost). A live row whose sequence skips one the browser has not seen is kept, and the gap is closed rather than rendered past: the fan-out drops events for a slow connection, so a gap is a signal to resynchronize, never silently skipped. A gap above the persisted watermark is closed by one tail read; a hole in an attempt's sequence that persistence already covers (a held saved row of that attempt at or beyond it, or the tail's persisted watermark past it) is closed by reading saved history backward from the row after it, by sequence, a bounded number of pages per signal, whenever a completed history read, a tail read or a stored row proving rows below it saved reveals one. A history refresh reads back until it meets a held saved row, since a held live row carries the id its saved row will have, and on to a held provider request that is still open, so a question whose stored republish was missed is not left provisional. The browser reads the tail only after a completed read of saved history, when the turn becomes active over a held window and once per gap, never because of its own cache writes, so recovery cannot feed itself. Permanent loss of guest storage before persistence is not recovered by this path.

<a id="g5.chat.live-tail-recovery.render-cost"></a>
[I · g5.chat.live-tail-recovery.render-cost]

The transcript's cost per delivered frame follows what changed: a turn whose rows did not change is neither re-projected nor re-rendered, and a streaming answer's text renders in stable pieces split only at paragraph breaks outside code fences, so a delta re-renders only its open piece and the completed text renders whole. Turns keep their rendered size: collapsing offscreen turns to placeholders would move the [history window's](#g5.chat.history-window) one-viewport trigger and clip overlays.

<a id="g5.chat.settlement"></a>
[I · g5.chat.settlement]

The platform settles an execution attempt only after provider exit, bounded output draining, the applicable process release/cleanup, bounded native-session harvest and finalization. Native turn ending, process exit and attempt settlement are distinct: text, a last tool result, stdin closure and an RPC acknowledgement are individually insufficient. Success requires successful native completion and successful process exit under the provider-specific outcome rules. Shutdown uses a 15-second exit grace and five-second post-exit output-drain bound, without imposing a default active-work deadline. Queue allocation follows authoritative settlement and the existing allocation policy.

<a id="g5.chat.recovery"></a>
[I · g5.chat.recovery]

When native session recovery is unavailable, a new attempt reconstructs context from eligible delivered history. Harvest is bounded; failed, incomplete or stale saves remain explicit and preserve recovery state. Claim resumable files saved only from the corresponding successfully persisted native evidence.

<a id="g5.chat.cancellation"></a>
[I · g5.chat.cancellation]

The dispatcher owns cancellation and process cleanup, retaining execution capacity until descendant exit is proved. Stop, Cancel and limits remain effective during provider setup, active work, blocked I/O and shutdown/finalization. Normal completion retains the [agent execution tree release exception](../software_architecture/workspace_vm.g5.md#g5.architecture.agent_execution_tree).

## Agent credentials

<a id="g5.chat.authentication"></a>
[I · g5.chat.authentication]

Agent authentication is separate from G5 login and repository authentication.  

<a id="g5.chat.claude-auth"></a>
[I · g5.chat.claude-auth]

Claude supports subscription tokens, Anthropic API keys, compatible bearer tokens and OpenRouter keys.  

<a id="g5.chat.codex-auth"></a>
[I · g5.chat.codex-auth]

Codex supports OpenAI API keys, OpenRouter keys and native ChatGPT subscription login.  

<a id="g5.chat.credentials"></a>
[I · g5.chat.credentials]

Credentials resolve for the current workspace owner, and each admitted turn fixes its authentication route.  

<a id="g5.chat.provider-transport"></a>
[D · g5.chat.provider-transport]

Native provider clients connect directly from the owner's VM to the selected endpoint, using the [resolved credentials](#g5.chat.credentials). G5 runs no LLM relay or credential broker inside that VM: the [owner's guest authority](../concepts/security.g5.md#g5.security.guest-authority) already includes credentials delivered there. Preserve the fixed auth route, native caching, [API-key precedence](#g5.chat.api-key-precedence), [usage collection](#g5.chat.usage) and secret redaction. Keys belong in the private launch environment, never command arguments or generated provider configuration. Codex's local RPC connection follows the [approved configuration](#g5.chat.codex-configuration); its native client retains this endpoint and credential ownership.

<a id="g5.chat.accounts"></a>
[I · g5.chat.accounts]

Named accounts belong to organization, workspace or user scope; invalid selections fail without credential fallback.  

<a id="g5.chat.account-continuity"></a>
[D · g5.chat.account-continuity]

A named account's kind and ownership scope remain stable. Settings may select only a compatible, visible account valid for their scope. An account cannot be deleted while organization, workspace or user settings outside its own scope assign it, including an assignment committing concurrently; [its own scope's assignments](#g5.chat.account-defaults) are cleared with it.

<a id="g5.chat.account-defaults"></a>
[D · g5.chat.account-defaults]

Creating a provider account makes it the default for each agent it can serve whose assignment at the account's own scope is still unset: an organization account fills the organization's assignment, a workspace account the workspace's, a person's account their own. The assignment is written with the account, in one commit, so no launch observes the account without its default; an assignment already set at that scope is never replaced and other scopes are untouched, so a person adding their own account overrides the organization's default only for themselves, as [inheritance](#g5.chat.inheritance) provides. Removing an account clears the assignments at its own scope that name it, in the same commit; assignments at other scopes still refuse the removal. Accounts created before this rule keep their unassigned state; their owners choose in Defaults. "Runtime variables", the choice to run on the raw credential keys of Runtime config, is offered in the Defaults card, the chat composer and the fork dialog only while such a key exists for the agent's family in a scope in context; otherwise an agent without an assigned account reads "No account" and the launch preview says to add one in Settings → Accounts. When no default agent is chosen, Auto prefers the first available agent that has an assigned account or raw keys, else the first available agent. Reason: owner request (2026-11-02): a person who had just added their first account still saw every new chat start on "Runtime variables" and fail for want of a credential, the only remedy was the Default account select two sections away in Defaults, and the onboarding copy had to teach that step; see the [design](../../pr_ontology_diff_design/account-defaults/ontology-plan.md).

<a id="g5.chat.fork-start-account"></a>
[D · g5.chat.fork-start-account]

While named accounts are offered, a fork that starts work chooses its first chat's account beside the agent, model and effort, as a new chat does; without that offer the fork's chat follows the configured assignment as before. The choice offers only accounts compatible with the chosen agent that the new workspace will see, the organization's and the person's own, never the source's workspace accounts, and defaults to the first of the person's, the source workspace's and the organization's assignments for the chosen agent that the new workspace can use, else the unassigned choice ([Runtime variables only while raw keys exist](#g5.chat.account-defaults), otherwise No account); a lower scope's assignment stands in because the fork starts with no workspace settings of its own, so that is what its chat would inherit. The fork waits until the accounts and assignments are known before its chat can start, shows a failed load but still lets the fork proceed with the choices offered (the unassigned choice unless an account was picked), and an unchanged retry keeps the account it attempted. The chosen account travels with the fork's launch choices and is resolved for the new workspace when the chat is queued. Reason: owner request [G5Labs/G5-Lean#987](https://github.com/G5Labs/G5-Lean/issues/987) (2026-10-30); a source workspace's account resolves against the source but the fork's chat would refuse it.

<a id="g5.chat.hints"></a>
[I · g5.chat.hints]

Stored keys and tokens are encrypted; account APIs expose only bounded hints.  

<a id="g5.chat.api-key-precedence"></a>
[I · g5.chat.api-key-precedence]

An API-key turn overrides cached subscription login without replacing that cache.  

<a id="g5.chat.login-before-prompt"></a>
[I · g5.chat.login-before-prompt]

ChatGPT subscription login completes before the original prompt is delivered, and that prompt keeps its place, challenge and deadline while login is pending. Messages sent meanwhile [queue](#g5.chat.queue) behind it without cancelling the login and run in order once the prompt has run; a [failed or interrupted](#g5.chat.failed-login) login ends that prompt's turn alone, and the queued messages start the next turn. A later message never ends a pending login; the owner's Stop, the deadline, a platform interruption of the turn, or loss of the VM or ownership does.

<a id="g5.chat.device-challenge"></a>
[I · g5.chat.device-challenge]

Device authorization challenges are private to the current owner and attempt, expire promptly, and never enter shared chat history.  

<a id="g5.chat.failed-login"></a>
[I · g5.chat.failed-login]

Failed or interrupted login does not mark the prompt delivered or automatically retry it.  

<a id="g5.chat.failed-login.notice"></a>
[I · g5.chat.failed-login.notice]

Apply [chat error notices](#g5.chat.error-notice) to terminal sign-in failures, with retry guidance per task attempt across reload and read-only views. Reconcile the timeline notice, saved reply and terminal sign-in state by attempt identity; omit the prompt’s redundant response-error label while preserving its failed delivery state. Active owner sign-in controls remain usable until that attempt settles.

<a id="g5.chat.error-notice"></a>
[I · g5.chat.error-notice]

Chat failure explanations visibly identify errors with an icon and error styling. Show a platform-authored diagnostic once as a notice, rather than also as assistant prose or a redundant generic response error. Reconcile representations by owning message or attempt, preserve genuine partial assistant output and distinct failures, and retain a generic error when no specific explanation is available. Presentation does not change delivery or retry policy.

<a id="g5.chat.credential-home"></a>
[I · g5.chat.credential-home]

Native subscription credentials remain in the owner VM and are excluded from conversation recovery bundles.  

<a id="g5.chat.credential-replacement"></a>
[I · g5.chat.credential-replacement]

VM replacement may require subscription login again; imported Codex authentication files are rejected.  

## Agent execution

<a id="g5.chat.module"></a>
[I · g5.chat.module]

agent.turn is a declared module that runs one agent execution attempt inside the workspace VM.  

<a id="g5.chat.module-input"></a>
[I · g5.chat.module-input]

The platform supplies agent.turn with the [frozen attempt input](#g5.chat.frozen-attempt): prompt, attachments, provider configuration, permissions and limits.

<a id="g5.chat.supervisor"></a>
[I · g5.chat.supervisor]

A supervisor launches the selected Claude Code or Codex CLI in the workspace environment. Every G5-managed Codex attempt, including [formal review](agent_approvals.g5.md#g5.agent-approval.codex-launch), uses one attempt-scoped App Server process over local stdio, initializes, starts/resumes the intended thread and starts one native turn with frozen input. Keep stdin open until the matching turn/completed terminal status, then close it and continue [shutdown and settlement](#g5.chat.settlement). No provider process remains resident between settled attempts. Already reserved attempts retain their [frozen transport/module contract](#g5.chat.frozen-attempt) within the [exec retirement boundary](#g5.chat.codex-exec-retirement); Claude's specialized launchers retain their declared restricted contracts.

<a id="g5.chat.codex-configuration"></a>
[D · g5.chat.codex-configuration]

Codex launches use exact binary pins; the App Server contract uses its pin's generated non-experimental schemas. Initialization explicitly sets capabilities.experimentalApi=false; only required non-gated methods and fields are admitted. The provider contract disables experimental runtime features, including inherited configuration overrides, and forbids experimental API, transport or feature opt-ins. Validate actual launch and handshake frames and effective defaults at the pin. Record the command's upstream maturity separately; an API opt-out does not change that maturity or establish integration compatibility.

<a id="g5.chat.codex-exec-retirement"></a>
[D · g5.chat.codex-exec-retirement]

Every new Codex attempt, ordinary or formal and including a retry or continuation that creates one, freezes the App Server contract and never falls back to `codex exec`. Reconstruction stays bound to the reserved VM incarnation, so an attempt frozen on exec before retirement only re-reaches the older guest that admitted it; guests from the retirement catalog onward have no exec launcher and refuse Codex on exec-contract modules rather than converting them. Remove the API's remaining acceptance of exec-frozen Codex reconstruction once no running pre-retirement incarnation holds an unsettled exec-frozen Codex attempt. Historical Codex chats continue in a new App Server attempt through native resume or [history reconstruction](#g5.chat.recovery), without replaying delivered input; retained transcripts and review reports stay readable. Manual `codex` use in a workspace shell is unaffected.

<a id="g5.chat.codex-runtime-refusal"></a>
[D · g5.chat.codex-runtime-refusal]

A Codex turn admitted to a VM incarnation whose catalog lacks the required App Server module is refused at admission with a notice to stop and start the workspace, keeping its queued input as a [correctable refusal](#g5.chat.secret-admission.kept-input). A formal review's workspace is [human-read-only](agent_approvals.g5.md#g5.agent-approval.human-read-only), so its notice directs an explicit review retry, which forks a new review workspace. Claude turns on the same VM are unaffected. Waiting silently would hide the cause, and an exec fallback would retain the retired transport.

<a id="g5.chat.translation"></a>
[I · g5.chat.translation]

Native output uses the shared records owned by [provider adapters](#g5.chat.adapters).

<a id="g5.chat.prompt-limit"></a>
[I · g5.chat.prompt-limit]

Chat input texts are limited to 512KB  

<a id="g5.chat.background-work"></a>
[I · g5.chat.background-work]

After a run-ending result, the Claude adapter keeps the attempt and Claude's input open while Claude reports background tasks or background sub-agents that it owns. Input admission and closing share the [steering decision](#g5.conversation.delivery.steering); close only between turns with no remaining reported background work, native request or pending stdin write. A flushed input needs no consumption acknowledgement or extra hold; exit, harvest and settlement then proceed normally. After reported background work, retain the three-second settling window for Claude to start its report turn. Drain and account for native turns produced by already-buffered input after EOF. The agent, not the platform, decides whether its background work continues or stops.


<a id="g5.chat.background-stop"></a>
[I · g5.chat.background-stop]

The owner can end the wait without cancelling the logical task. The adapter stops Claude's background work through Claude's native controls where the pinned CLI supports them, otherwise through the bounded stop path. A visible notice records that the work was stopped  

<a id="g5.chat.usage"></a>
[I · g5.chat.usage]

agent.turn collects token usage and the platform persists it in a manner visible through the chat API. Repeated cumulative provider snapshots do not double-count or charge a prior native session turn to the current attempt.

<a id="g5.chat.subscription-usage"></a>
[I · g5.chat.subscription-usage]

For Claude and Codex subscription-authenticated work, show provider-reported five-hour and weekly allowance percentages when those windows are available. Use the attempt's actual frozen authentication route, respecting API-key precedence, rather than the existence of cached login or a claimed paid plan. Read usage passively during ordinary work without additional provider checks. G5 keeps these display observations in memory, scoped to the current chat and account; they do not enter execution history, billing or persistent quota storage, and never gate or delay work. Missing, malformed, differently scoped or reset-expired evidence is unknown, not unused allowance. A reload can wait for the next observation. Meter delivery verifies the current session, task attempt and VM and uses the chat's existing reader authority.

Claude's richer usage observations rely on internal fields in the pinned CLI; verify that compatibility on CLI upgrades. The live chat subscription is available wherever workspace chat is available, including self-hosted deployments, independently of cloud workflow controls. Subscribe only after confirming that the session belongs to the displayed workspace.

Place the remaining-percentage button at the left of the configuration row above the chat input. Activating it expands only the available five-hour and seven-day details above the button, including usage bars, the selected window, available reset information and observation age. Omit windows without valid, unexpired usage; when the preferred window is unavailable, use the available window for the button and selected detail, and hide the meter when neither is available. A second activation, Escape, clicking outside or moving keyboard focus away dismisses the details. The ring, remaining percentage and selected-window label stay visible when collapsed.

<a id="g5.chat.dispatch-secrets"></a>
[I · g5.chat.dispatch-secrets]

The dispatcher clears inherited environment variables and injects validated dispatch secrets into the agent.turn process.  

<a id="g5.chat.secret-admission"></a>
[I · g5.chat.secret-admission]

Runtime Config secrets follow one exportable-environment policy shared by configuration writes, agent and command dispatch and [terminal admission](terminal.g5.md#g5.terminal.secret-admission): an uppercase environment name that is not platform-reserved, a bounded value without NUL, and a bounded entry count and serialized size that fit every consumer's frame. A write refuses a violating key or an overfull scope with the reason. Dispatch validates the effective secrets before it reserves a turn: a stored invalid configuration settles that task as failed with the reason, records the reason in the chat, and the pass goes on to the next candidate, so other sessions and organizations keep dispatching. Scopes merge, so count and size limits are final at admission.

<a id="g5.chat.secret-admission.kept-input"></a>
[D · g5.chat.secret-admission.kept-input]

A turn refused at admission for something the owner must correct keeps its queued input attached to the failed task; the owner's next message, once the configuration is corrected, runs that input ahead of itself. An automatic follow-up would copy the same configuration and fail every pass.

<a id="g5.chat.secret-admission.reconcile"></a>
[D · g5.chat.secret-admission.reconcile]

A reserved attempt whose configuration became invalid before its launch was confirmed is re-invoked with the offending entries withheld and a preflight error, and the guest decides: one already running it confirms, one that never saw it fails the turn before the provider runs, and the reason reaches the chat on that fresh start. Reservation makes no network calls, and a lost invoke response does not mean nothing runs, so the API neither probes nor cancels.

<a id="g5.story.reuse-native-memory"></a>
[I · g5.story.reuse-native-memory]

Reuse native memory across the owner's sessions and workspaces through their [personal repository](personal_workspace.g5.md#g5.personal.memory), privately: it is never shared with teammates through a workspace repository. The agent controls reading, saving, consolidation and pruning; G5 adds no cross-provider reconciliation, semantic conflict resolution or extra pruning.


## Native recovery and forked history

<a id="g5.chat.native-question-recovery"></a>
[D · g5.chat.native-question-recovery]

An unanswered native clarification remains pending through its saved provider session when compute detaches. First acceptance requires the current owner and ready compute, or a [Start consent recorded with the answer](#g5.chat.start-and-send) that is still under way; the accepted answer is frozen, and identical retries may confirm it after detachment. Restore only for that owner’s matching native session and request. If ownership transfers before an accepted answer is restored, that answer is not restored and the question returns to pending for the new owner, so the transfer does not fail the logical task. Confirm delivery only from matching transport-write evidence, settling the restored input and answer together. Explicit Stop or Cancel may end the work.

A question is deferred to the saved session only while the process holds nothing beyond the current turn: no [admitted steering](#g5.conversation.delivery.steering), pending native request or background work. Deferring closes steering admission for the rest of that process; after any admitted steering, this and every later question of the process stay live requests of the running attempt, which keeps its provider process while the owner answers. Expose a deferred question as resumable only while the saved continuation still awaits it: if the process runs another native turn after the deferral, whatever then ends the attempt, or a clean exit does not capture its session, the question is invalidated and the turn holds for the owner, not for an answer. An attempt that ends otherwise after an unadvanced deferral (a provider error, Stop or limit) keeps the question, as before, and restoration decides. Restoration is bounded after Claude acknowledges initialization; a saved session that does not reissue the question, whether it idles or exits, fails that attempt visibly, keeps the accepted answer with the request, reports an identical retry as undeliverable rather than queued, and offers sending the question and answer as an ordinary new message. Never deliver an accepted answer to a different question, truncate real history or replay delivered input. Reason: the pinned CLI consumes already-buffered input after a deferral and saves that later turn, so the deferred question is no longer offered on resume.

<a id="g5.chat.native-question-delivery-state"></a>
[I · g5.chat.native-question-delivery-state]

A question that [native question recovery](#g5.chat.native-question-recovery) withdrew, or whose saved answer could not be delivered, shows as stopped like any other ended request: no new label and no explanation of the cause, which only the turn's notice states. When an answer was saved, the card keeps it visible and offers sending the question and answer as a new message. The internal continuation that carries a saved answer is shown as that delivery, never as a blank queued message. Reason: the owner does not want people to learn that an answer can be saved but undelivered or that a question can be withdrawn; the card should stay familiar and the recovery one action.

<a id="g5.chat.native-request-boundary"></a>
[D · g5.chat.native-request-boundary]

Native request identities are validated at the producer against the full persistence contract: unique across the attempt, bounded and trimmed, with no characters storage sanitization would re-map onto another id. A reused or invalid identity, like an unsupported control request, is rejected to the provider through its native rejection mechanism rather than left unanswered, and the turn settles with that failure evidence. Distinct identities are never merged.

The API stays the final boundary: a deterministic projection or state conflict in one journaled provider-request fact becomes bounded, redacted error evidence that consumes its sequence, so later facts, terminal settlement and workspace capture still proceed; transient storage failures are retried without consumption. After owner authorization, an identical accepted answer replays from its stored key, body and digest even after the request settles or the execution ends — the request is never reopened and a changed answer is rejected. When an owner's pause or stop invalidated a [deferred question](#g5.chat.native-question-recovery) before its journaled suspension applies, the task holds for the owner, not for an answer.

<a id="g5.chat.session-fork"></a>
[D · g5.chat.session-fork]

The [forked history](#g5.chat.fork) preserves the selected transcript order with new message identities and destination-bound attachments retaining their content. Unfinished assistant messages become stopped; queued inputs and native provider continuation are not inherited.


## Dispatch eligibility

<a id="g5.chat.dispatch-eligibility"></a>
[D · g5.chat.dispatch-eligibility]

Agent and command dispatch share workspace execution capacity and require completed dependencies and current runtime authority. A waiting input preserves order within its session without blocking eligible independent sessions.

<a id="g5.chat.dispatch-reachability"></a>
[D · g5.chat.dispatch-reachability]

Before reserving agent, command or preview work, renewing an unconfirmed dispatch retry, or claiming a live attempt for reconciliation, the executing API must have a route to its host. Derive eligibility from the runtime's live connections so a disconnected replica cannot postpone a capable one through shared claim state. Connection loss preserves the durable input and execution identity for reconnect or another capable replica. Database-only settlement of an absent, expired or replaced incarnation requires no host connection; disconnection alone does not establish VM loss. Runtime admission still checks current authority after selection.

<a id="g5.chat.execution-defaults"></a>
[D · g5.chat.execution-defaults]

A workspace defaults to 16 concurrent agent and command turns unless its owner chooses another supported limit. Slots bound concurrency; simultaneous turns share the VM's CPU, memory and disk.

<a id="g5.chat.execution-headroom"></a>
[D · g5.chat.execution-headroom]

API admission and the VM catalog must support the [default turn capacity](#g5.chat.execution-defaults) while leaving room for Changes and diagnostics. A VM on an earlier four-slot catalog admits at most two agent or command turns until replaced; higher owner limits remain subject to the VM's module and total limits.

<a id="g5.chat.spend-thresholds"></a>
[I · g5.chat.spend-thresholds]

A workspace spend or token threshold that the owner can set (`maxSpendUsd`, `maxInputTokens`, `maxOutputTokens`) is enforced when dispatch admits work; until it is, the workspace settings do not offer it. An owner may rely on a limit they were allowed to set, so a threshold with no effect is a defect, not a deferral.

<a id="g5.chat.spend-scope-question"></a>
[Q · g5.chat.spend-scope-question]

Are workspace token/dollar admission thresholds and period-wide accounting still in scope, deferred or retired? Provisionally retain period-wide accounting and defer threshold enforcement pending a scope decision; while enforcement is deferred, the settings do not offer the [thresholds](#g5.chat.spend-thresholds). Retained enforcement must cover ordinary chats and failed, cancelled or jobless attempts; retirement requires explicitly replacing or removing the settings' promise. A product decision and a traced admission owner would settle this question.

## Recording and native steering

<a id="g5.agents.output-bounds"></a>
[D · g5.agents.output-bounds]

An agent turn's output is bounded per line and per event, never in total. No amount of output ends a turn, so a runaway turn ends only by Stop or a VM stop.

- Bound each provider stdout line before parsing: discard an overlong line through its newline with a visible notice, without failing the turn for that line.
- Mask secrets before bounding recorded content, keep the full content's digest and a truncation flag, and bound each serialized event.
- Hold the replies to a provider's control requests within a small fixed bound. A provider that overruns it ends the turn as a provider error.
- Provider stderr keeps its own bounded log, and passing that bound still ends the attempt as limited.

<a id="g5.conversation.delivery.steering"></a>
[D · g5.conversation.delivery.steering]

Every organization enables native steering for eligible running Claude attempts: send ordinary owner text unchanged, binding each immutable revision and delivery generation under the session lock without overtaking older queued, reserved or uncertain input. Require accepted initial input and the attempt’s frozen steering capability, with no hold, cancellation, pause, native request or background wait. Attachments, Codex and ineligible/older processes keep the queue path; never restart Claude to deliver input. A question the process [defers](#g5.chat.native-question-recovery) closes admission for the rest of that process, decided before the hook reply is written; input that arrives after it keeps the queue path with its revision and generation.

A complete write and flush to Claude stdin is delivery, without replay or consumption confirmation. Journal that fact and atomically project its user row and new assistant segment within the same task/attempt. Native text/tool units already started remain in their original segment through completion; new units follow the new input. Same-key retries never write that generation twice. Input admission and stdin closing share one worker decision; no separate input hold or attempt-settlement lifecycle is added.

<a id="g5.conversation.delivery.steering-recovery"></a>
[D · g5.conversation.delivery.steering-recovery]

Steering makes one bounded send RPC. Definite rejection or no definite answer by its deadline leaves the message in the ordinary next-turn queue; a rare duplicate after an ambiguous write is explicitly accepted. A newer edit/reservation cannot be delivered by stale flush evidence. Once flushed, retain the delivered message in history after Stop, exit or VM loss even if Claude never reads it; that loss is accepted and the owner may resend. Show Waiting until delivery, then remove the tray entry. There is no steering retry reconciler or uncertain-delivery choice; initial-input recovery retains its own rule.

Retain positive RPC flush receipts independently of ordered transcript projection.
Existing attempt settlement projects known receipts before releasing queued work and
releases reservations without known delivery. A late receipt may restore its original
binding only while the revision and delivery generation are still current.

<a id="g5.conversation.providers.claude-followups"></a>
[D · g5.conversation.providers.claude-followups]

Claude’s subsequent native turns, including steered input, remain in the same attempt. Background reports continue the current assistant segment; [flushed user input](#g5.conversation.delivery.steering) starts another. Preserve completed units in provider order and accumulate usage across native turns. Success requires successful results for all started turns and ordinary execution-outcome evidence. Sub-agent frames are not direct reply units. Background-waiting input queues; this does not create a provider process resident across attempts.


## Chat navigation

<a id="g5.chat.ui.header-title"></a>
[I · g5.chat.ui.header-title]

The chat header gives the current chat's title the width the other header controls leave, on one line, with an ellipsis only when it does not fit and the full name on hover; the title still opens the [chat picker](#g5.chat.ui.picker). A separate pencil button immediately to its right renames the current chat through the existing rename, which makes a renamed or cleared title the person's own. Enter saves, Escape cancels, and saving and failure are shown. The center, detached and narrow layouts stay usable.

<a id="g5.chat.ui.picker"></a>
[I · g5.chat.ui.picker]

Chat picker rows carry no rename controls. They keep selection, title search and a distinct selected-chat highlight, and show the [activity indicator](#g5.chat.activity) and unambiguous times: `Started 2h ago · Last active 5m ago`, `Active now` during a turn, and `No finished turns` before the first turn finishes.

<a id="g5.chat.activity"></a>
[I · g5.chat.activity]

A chat shows a spinning circle while it has an active turn, a solid circle when a turn has finished with a result the current person has not [inspected](#g5.chat.inspection), and no circle otherwise. The spinner takes precedence; an earlier uninspected result stays recorded until it is inspected. Both indicators have accessible labels and tooltips.

<a id="g5.chat.activity.turns"></a>
[D · g5.chat.activity.turns]

A chat turn is one agent task of the session; its retries, continuations and steered inputs stay in that turn. The turn is active while its task is running, including in-attempt native requests and Claude background waits, or queued while the workspace's compute is starting, running or has a start in progress. Queued work behind stopped compute waits for the owner unless an [automatic continuation](../software_architecture/workspace_vm.g5.md#g5.lifecycle.start.automatic-continuation) start is pending, and a held (waiting) or terminal task is not active, so resumable work does not spin; a start, Continue or an answer makes it active again. Each settled attempt is a finished result, including failed, stopped, interrupted and suspended attempts; a turn that ends without any attempt, such as an admission refusal or a cancel before dispatch, has its task end as its result.

The active turn started when its task was created; the chat was last active when its latest result ended. Both derive from task and attempt records, never from session `updated_at`, which renames and settings writes change. A session has at most one live agent task and terminal tasks gain no attempts, so the latest result belongs to the newest task that has one and each chat's activity reads a bounded number of indexed rows.

<a id="g5.chat.activity.awaiting-input"></a>
[D · g5.chat.activity.awaiting-input]

A chat's activity also says whether its turn waits for the person: a question or permission request of its running attempt is still pending, or a deferred question holds the turn. An answered, timed-out or invalidated request no longer waits, and a turn held for any other reason only has a finished result. The fact derives from the task and its provider requests, and the list is hinted when a request opens or settles, as it is for turn starts and settlements. Reason: the [chat bubble stack](ui.g5.md#g5.workspace.ui.chat-bubble-stack) needs it for every chat of the workspace, while only the open chat's timeline carried it.

<a id="g5.chat.activity.order"></a>
[I · g5.chat.activity.order]

The chat list shows chats with an active turn first, newest turn start first, then the other chats by their last finished turn, newest first, using creation time for a chat with no finished turn and a deterministic tie-breaker. Renaming, launch or provider-setting changes and inspection do not move a chat. The center chat view's recent conversations, shown when no chat is selected, are the first three chats in this order with the [picker's](#g5.chat.ui.picker) activity time. The server orders the workspace's chats before the existing 50-chat limit; this adds no pagination.

<a id="g5.chat.inspection"></a>
[I · g5.chat.inspection]

Inspection is personal: another person's viewing never clears this person's indicator. It survives reload and converges across the person's tabs and devices. Selecting or mounting a chat acknowledges nothing by itself, so a result finishing later, in a background chat or an unfocused page, stays uninspected until it is seen.

<a id="g5.chat.inspection.acknowledge"></a>
[D · g5.chat.inspection.acknowledge]

The browser acknowledges one named result, the chat's latest, once it is displayed: the page is visible and focused, the chat surface is shown and scrolled to its end, and the transcript holds that result's finished assistant output (or it produced none). The API accepts it only for a finished result of an agent task in that chat and advances the person's per-chat watermark to the result's end time, never backwards, so a newer result that finished meanwhile stays uninspected. A result is uninspected when it ended after both that watermark and the chat's floor; existing chats' floor is the rollout time, so historical results count as inspected, and a new chat's floor is its creation. Recording needs only [read authority](#g5.chat.authority) for the chat and changes no workspace content or [activity order](#g5.chat.activity.order).

<a id="g5.chat.activity.refresh"></a>
[D · g5.chat.activity.refresh]

The session list and session reads carry each chat's activity and the caller's inspection state from one batched query. After committing an agent-task change that starts, ends, holds or releases a turn, the API publishes a content-free `chat.activity` hint on the workspace channel, except from a commit that can revoke a subscriber's read access, such as an ownership transfer, because a revoked socket receives no application data; an acknowledgement sends it only to the acknowledging person's connections. Clients refetch the list on a hint, on reconnect and when the page regains focus, and every 15 seconds while a listed chat is active, so a missed in-process hint delays but cannot prevent convergence. Clients poll no individual chat for activity.

## Ontology changes in the chat

<a id="g5.chat.ontology-card"></a>
[D · g5.chat.ontology-card]

When statements in the workspace's ontology change between the start and the end of a chat's [turn](#g5.chat.activity.turns), the turn's reply ends with a changes card. The card compares the ontology at the turn's start with the ontology at its end, and lists each statement added, modified or deleted in between, with its text, and for modified and deleted statements the Before text from the turn's start. It uses the editor's [Statement changes](documents/ontology.g5.md#g5.documents.ontology.statement-changes) presentation under the shared [matching rule](documents/ontology.g5.md#g5.documents.ontology.comparison-matching). It shows no total, because Compare shows the workspace's whole change.

- The card shows every change made during the turn, whoever made it: usually the chat's agent, but also any edit made at the same time in the editor or by another chat's turn. It does not attribute changes to writers. The owner's [edits on the card](#g5.chat.ontology-card.edit) are made between turns, so they never fall inside one, and are shown on the card, marked as theirs.
- A card keeps showing its own turn's changes after later turns, after a reload and while the VM is stopped. The newest card is expanded and earlier ones are collapsed.
- A turn with no ontology change shows no card, and neither does a turn the person stopped or one that failed. Only a turn that ends successfully has a card; one that timed out or exceeded a limit counts as failed. Changes such a turn left behind show in Compare.
- A card shows its first 5 statements, grouped by file, and a Show more control reveals the rest in place. A View all changes in workspace link opens the workspace's [All changes](documents/ontology.g5.md#g5.documents.ontology.all-changes), its whole change against its parent, not this turn's.
- A file that cannot be compared shows its refusal in the card, and the rest of the card still shows. A changed statement whose text the card cannot display is still listed and counted, with a note that it can't be shown. A turn whose changes are too large to carry on the card shows a card saying so, with the View all changes in workspace link.
- Every workspace chat has cards, whichever agent runs it. The [personal chat](personal_workspace.g5.md#g5.purpose.personal) has none, because its agent's edits land only in reference checkouts that are discarded.

<a id="g5.chat.ontology-card.governing"></a>
[D · g5.chat.ontology-card.governing]

A [changes card](#g5.chat.ontology-card) also lists the unchanged statements that govern its turn's changes, collapsed by default. The agent names up to eight, each with a one-line reason. The platform confirms that each named statement exists in the version the card's changes are based on, and shows its text. The statements are grouped by file and laid out like the card's changes under an Unchanged label, each with the agent's reason under it. A name among the card's own changes is left out, because the card already shows it; any other name it cannot find is shown flagged as not found, never silently dropped. The list is read-only, is not counted in the card's numbers, and cannot be edited on the card.

<a id="g5.chat.ontology-card.edit"></a>
[D · g5.chat.ontology-card.edit]

The workspace owner can edit the text of an added or modified statement in place on the newest [changes card](#g5.chat.ontology-card):

- Only the owner, only on the newest card, only while no turn is running, and only while the VM runs. A turn waiting for the person's answer is still running. The newest card stays editable after later turns that produce no card, such as a code-only turn. For anyone else reading the chat, and for older cards, deleted statements, a running turn or a stopped VM, the card stays read-only and shows why.
- An open edit and its unsaved text belong to the person and the workspace page. They survive the chat being rebuilt, such as a layout change or a chat switch, until saved or cancelled, and a Save in progress completes. Reloading the page discards them.
- Saving writes the statement into the workspace's ontology file. It writes only if the statement in the file still matches the card. Otherwise, or if the edited text is not a valid statement, nothing is written and the card shows why.
- Once the file write is confirmed, the edit is recorded with the card, even if a turn has started since. If recording fails, a retry completes it without writing the file again.
- In place means the statement's text becomes the editor, while the statement keeps its place in the list, its label and its Before text. The Edit control appears beside the statement's details control when the pointer is over the statement's text or it has focus, like that control.
- A saved statement carries a quiet "(edited)" mark after its text, whose tooltip says who edited it ("You" for the owner) and that the edit is saved to the ontology file. The mark is not part of the statement's text. The card shows only the edited text and otherwise remains the record of its turn.
- The platform adds a short note to the next turn's input naming the statements the person edited and their files, so the agent works from the edited text rather than undoing it. It is delivered like the platform's other notices to the agent, so it also shows in the chat.

## Composer and sign-in conveniences

<a id="g5.chat.ui.prompt-history"></a>
[I · g5.chat.ui.prompt-history]

From an empty Chat composer, Up enters submitted-prompt history. While browsing, Up and Down traverse older and newer prompts with the caret at the end, including multiline prompts; Down past the newest restores the empty composer. Editing or other caret navigation leaves browsing until the composer is cleared, so Up and Down then move normally within the text. Browsing never submits; modified arrows, selections, IME composition and open suggestion menus retain their normal behavior.

History belongs to one chat: Up and Down browse only the prompts the current person submitted in the current chat, and it remains available after a browser reload and after the workspace VM stops, restarts or is replaced. Changing chats ends browsing and loads the destination chat's history, so neither a recalled prompt nor the history position carries into another chat.

<a id="g5.chat.ui.prompt-history.store"></a>
[D · g5.chat.ui.prompt-history.store]

[Prompt history](#g5.chat.ui.prompt-history) stays in the browser per person, workspace and chat, like issue comment drafts, and is cleared on sign-out; no VM holds it, so VM lifecycle cannot lose it. Each chat keeps up to 100 typed prompts whose send was accepted; a repeat moves to the front, and references, attachments and prompts over 20,000 characters are not kept. A new chat starts empty. An accepted send is recorded against the chat that accepted it, including the chat its first send created and a send that resolves after the person switched chats. Switching chats while an unedited recalled prompt fills the composer restores the empty composer; edited text is an ordinary draft. The browser keeps at most 200 chat histories and 1,000,000 characters in total, dropping the least recently recorded chats first, so per-chat buffers cannot exhaust shared browser storage. Earlier workspace-wide buffers have no chat identity, so they are discarded rather than copied into any chat; transcripts are untouched. Without usable browser storage, history lasts for the page. Saved conversations are not the source: they hold other members' turns and appended reference labels. Navigation uses a snapshot of the entries taken when browsing begins.

<a id="g5.chat.ui.device-code-copy"></a>
[I · g5.chat.ui.device-code-copy]

A newly shown Codex device code is copied to the owner's clipboard exactly and alone, with a brief confirmation, and the sign-in link stays clickable. When copying fails or is unavailable, an explicit Copy action and the visible code remain. Refetching, remounting or replaying the same challenge does not copy it again.

<a id="g5.chat.ui.device-code-copy.once"></a>
[D · g5.chat.ui.device-code-copy.once]

The [automatic copy](#g5.chat.ui.device-code-copy) happens once per challenge in each browser tab. Its record names only the session, task and attempt, which has one challenge, never the code, so the code stays out of storage under the [device challenge](#g5.chat.device-challenge) rule. A code that appears while the page is unfocused is copied when the page regains focus, because browsers refuse clipboard writes from an unfocused page.

## Metadata

### g5.purpose.agents

- human-provided
- Expands: [Product goals](../root.g5.md#g5.purpose).
- Source: [Ben, line 219](../../../docs/ben_new_ontology.md#L219).

### g5.chat.workspace

- human-provided
- Source: [Ben, line 220](../../../docs/ben_new_ontology.md#L220).

### g5.chat.configuration

- human-provided
- Source: [Ben, line 221](../../../docs/ben_new_ontology.md#L221).

### g5.chat.account-credential-shape

- agent
- Source: owner request (2026-10-27) after the magna-demo QA diagnosis; `ProviderAccountKind::validate_secret` (g5-contracts), `lib/providerAccountSecret.ts` and the `ProviderAccountsSection` form, `ClaudeFrameMapper::with_auth_mode` in g5d-agent.

### g5.chat.native-catalog

- agent
- Source: [owner request](../../chat_intent/issue-809/request.md), [design](../../pr_ontology_diff_design/issue-809/ontology-plan.md#verification-and-unresolved-evidence).
- OpenRouter composer correction: [G5Labs/G5-Lean#1043](https://github.com/G5Labs/G5-Lean/issues/1043); regression coverage in `ChatPanel.test.tsx`. Its route refusal of plain variants protected a 200K pin from the 1M variant; it ended on 2026-11-09 when every offered family became a single 1M form.
- Fable 5.2, release order, Opus 5.5 default and the retired `[1m]` offers: [owner request](../../chat_intent/fable-5-2/request.md) and [design](../../pr_ontology_diff_design/fable-5-2/ontology-plan.md) (2026-11-09), with the Claude Code 2.1.321 pin evaluation. Evidence: `claude_models.rs` tests (`families_are_listed_newest_first_by_release_date`, `every_preferred_direct_form_is_the_plain_id`, `offered_variants_match_the_current_picker_lineup`), the regenerated `claudeModelsBaseline.ts`, `useUserSettings.test.ts` legacy-default folding, the re-recorded `tests/fixtures/agent-turn-native/claude-2.1.321/` lifecycle schedules and the native catalog gate on the 2.1.321 binary.

### g5.chat.inheritance

- human-provided
- Source: [Ben, line 222](../../../docs/ben_new_ontology.md#L222).

### g5.chat.launch

- human-provided
- Source: [Ben, line 223](../../../docs/ben_new_ontology.md#L223).

### g5.chat.queued-launch

- human-verified
- Source: owner-authorized [issue #629 fix plan](../../pr_ontology_diff_design/issue-629/ontology-plan.md).

### g5.chat.confirmed-launch

- agent
- Confirmation through first admission: owner-authorized [issue #629 fix plan](../../pr_ontology_diff_design/issue-629/ontology-plan.md), with API and dispatch regressions.
- Comparison scope clarified by the [owner's PR review](https://github.com/G5Labs/G5-Lean/pull/907#pullrequestreview-5676664038).
- Source: [coverage C21/F5](../../uplift/ben-ontology/reviews/coverage-identity.md#f5), retained settings design and `ConfigService::confirm_agent_launch`.
- Historical [bounded checks](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md): `LaunchAgreement` retained confirmed choices in local resolution; `QueuedLaunchAgreement` exposed [CI-LAUNCH-CONFIRMATION](../../../docs/code_gaps.md), repaired by retaining confirmation on the input and comparing it at first dispatch. The checks do not establish a global settings snapshot or confirmation of every continuation; current-owner credential authority still applies.

### g5.chat.confirmed-launch-recovery

- agent
- Source: [owner's PR review](https://github.com/G5Labs/G5-Lean/pull/907#pullrequestreview-5676664038); specializes [confirmed launch](#g5.chat.confirmed-launch) through the existing queue recovery path.

### g5.chat.model-env

- human-provided
- Source: [Ben, line 224](../../../docs/ben_new_ontology.md#L224).

### g5.chat.providers

- human-provided
- Source: [Ben, line 225](../../../docs/ben_new_ontology.md#L225).

### g5.chat.authority

- human-provided
- Source: [Ben, line 226](../../../docs/ben_new_ontology.md#L226).
- The workspace-agent clause follows [workspace agent parity](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-request); the agent controls chats only in its owner's other workspaces within its scope ([`g5.security.agent-chat-control`](#g5.security.agent-chat-control)).

### g5.security.agent-chat-control

- human-verified
- Source: [child-only chat control](../../chat_intent/issue-710-workspace-agent-parity/request.md#human-instructions-recorded-by-earlier-sessions): the user proposed native local sub-agents for same-workspace delegation. Covering indirect launchers is agent design.
- Implemented: every agent chat mutation (new chat, follow-up, stop turn, background end, rename, queued-input edit/withdraw/redeliver, chat-task continue/cancel) admits its target with `AgentTargetNeed::OtherOnly` ([design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#chat-boundary)); non-chat tasks also admit the agent's own workspace. `operations_reach_this_workspace_and_its_owners_other_workspaces` and `an_agent_controls_its_childs_chats_but_never_its_own` cover the self refusals.
- Gap: forking a chat into another workspace has no agent route; the non-persona session fork has no request key.

### g5.chat.queue

- human-provided
- Source: [Ben, line 227](../../../docs/ben_new_ontology.md#L227).
- Human-verified amendment: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-544/request.md). Original Ben source remains immutable; the source-edit ledger records this semantic change, not an editorial normalization.
- Agent amendment ([issue #629](https://github.com/G5Labs/G5-Lean/issues/629), finding S12): references the redelivery-authorized exception owned by [g5.chat.uncertain-delivery](#g5.chat.uncertain-delivery) so the two blocks agree.
- Agent amendment: [issue #1069](https://github.com/G5Labs/G5-Lean/issues/1069) and its [design](../../pr_ontology_diff_design/issue-1069/ontology-plan.md) define frontend submission visibility and its correlation limit.


### g5.chat.start-and-send

- human-verified
- Amended 2026-11-05 (agent): preview left the disabled list; [g5.preview.start-and-preview](../software_architecture/workspace_vm.g5.md#g5.preview.start-and-preview) binds a preview task to the consent the same way, and `start_and_send::begin` now joins the owner's Start already under way instead of superseding it (a second compound action used to withdraw the first one's queued input).
- Source: owner request in workspace chat (2026-11-03): "when vm is not attached, and i send a message in the chat, this send action will attach a vm and also send the message to queue", then "all the buttons in the ribbon that redirect to chat should be not greyed out" and "if it is greyed out, we should have a pop up message to ask for a start". Audit and mocks: [stopped-vm audit](../../../../docs/d9b145f1-1267-4a77-af5f-6de0e85f8791/stopped-vm-audit-and-start-and-send.md) in the docs repository.
- Amended 2026-11-05: Ship with agent joins the list ([decisions](../../chat_intent/ship-with-agent-wo-vm/request.md), [plan](../../pr_ontology_diff_design/ship-with-agent-wo-vm/ontology-plan.md)); the owner chose the same withdrawal rule as every other compound action over keeping the agent's message queued.
- Amended 2026-11-06: the conflicts dialog's Resolve with Agent joins the list under the same decisions, as the owner asked in the same PR ([decisions](../../chat_intent/ship-with-agent-wo-vm/request.md), [plan](../../pr_ontology_diff_design/ship-with-agent-wo-vm/ontology-plan.md#follow-up-start-and-resolve-with-agent)).
- Implemented by reusing the personal path: `routes/start_and_send.rs` (`begin`/`bind`/`abandon`) on send, start-session, the enable-auto-merge route and now `POST /tasks/:id/provider-requests/:rid/reply` (`start_compute`; `NativeReplyInput.pending_start` relaxes the held question's readiness check); frontend `startAndSend` in `pages/WorkspacePage.tsx` for every owner workspace, `StartAndSendNotice`, `useAnswerBlockedReason` (Start and answer), `CreateExplainerModal` (Start and generate), `useStartWorkbenchAction.startCompute` (Implement from an issue), the Review meeting ribbon case and stored-transcript path, and `ChatPanel.providerSettingsBlocked` dropped. Greyed controls reveal their reason through `GatedControl` (`CompactContextMenuItem.blockedReason`, the rail header action, the phone strip's Terminal).

### g5.chat.delivery

- human-provided
- Source: [Ben, line 228](../../../docs/ben_new_ontology.md#L228).

### g5.chat.uncertain-delivery

- human-provided
- Source: [Ben, line 229](../../../docs/ben_new_ontology.md#L229).
- Human-verified amendment: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-544/request.md). Original Ben source remains immutable; the source-edit ledger records this semantic change, not an editorial normalization.
- Agent amendment ([issue #629](https://github.com/G5Labs/G5-Lean/issues/629), finding S12): authorized redelivery freezes the possibly delivered revision — ordinary edit would otherwise silently mutate content the provider may already hold.


### g5.conversation.late-facts

- human-provided
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 2 (IN-cell-31, IN-cell-41, IN-cell-42).

### g5.chat.completed-output

- human-provided
- Source: [Ben, line 230](../../../docs/ben_new_ontology.md#L230).
- [TranscriptRetention](../../uplift/ben-ontology/investigations/chat-identity.md) preserved completed blocks and their order through partial output, replay, failure and interruption, with separate controls for lost or changed content. It modeled one accepted attempt and explicit block identities; provider translation and browser rendering were inspected, not runtime-tested.

### g5.chat.ontology-card

- human-provided, agent
- Source: the owner's 2026-10-28 extension: "apply the card to workspace chat as well. For every ontology changes, each turn makes, we show the card. If the turn doesn't make any ontology change, then no card is shown". The owner chose no total ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#extension-to-workspace-chats-2026-10-28)), and on 2026-10-29 no card for a stopped or failed turn, a limit of 10 statements before Show more (lowered to 5 on 2026-11-02), and the View all changes label for the link to All changes ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#remaining-questions-2026-10-29)), renamed on 2026-10-31 to "View all changes in workspace" because "View all changes" read as this reply's changes. On 2026-10-29 the owner replaced "this chat's agent's changes only" with the changes between the turn's start and end, because attributing changes to one agent is hard ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#changes-between-the-turns-start-and-end-2026-10-29)). The newest-expanded rule is the owner's, chosen for the personal-chat cards first. On 2026-10-29 the owner limited the feature to regular workspaces, with no cards in the personal chat ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#scope-narrowed-to-workspace-chats-2026-10-29)). These are agent choices: Before text from just before the turn, durability across reload and VM stop, and per-file refusal.
- On 2026-10-29 System Engineering decided that a turn that timed out or exceeded a limit counts as failed ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#system-engineering-decisions-2026-10-29)), and that the turn's start is recorded as a commit the server has, plus a patch of the ontology's differences from it and the text of new untracked files, rather than a full copy, because of size ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#the-turns-start-snapshot-2026-10-29), [design](../../pr_ontology_diff_design/saehan-ontology-chat/ontology-plan.md#system-engineering-questions)).
- Implemented: the runner records the turn's start before its provider starts and builds the card when an attempt succeeds (`backend/g5d-agent/src/ontology_turn.rs`, wired in `runtime.rs`); its Git reads and the refetch of a start commit lost locally go through g5-scm's typed operations (`native/worktree_snapshot.rs`), the refetch checked against the member's canonical URL from the dispatcher's member vector; the start and governing list are kept in `chat_turn_ontology` (migration 142) through the workspace-agent routes `/chat-tasks/:taskId/ontology` (`routes/workspace_agent.rs`); `chat_ontology::attach_card`, called from `task_lifecycle::apply_outcome`, copies the `ontology.card` event onto the reply when the turn succeeds, writing the message through ChatService; the browser renders it in `frontend/src/components/chat/OntologyChangesCard.tsx` from `lib/ontologyCard.ts`, rebuilding statements with the editor's parser.
- Evidence (2026-10-29, this branch): g5d-agent `ontology_turn` tests on real Git repositories rebuild staged, unstaged, untracked, deleted, renamed and unpushed starts exactly without writing the index (also for a file touched without a change), refs or stash, and refetch a pruned start commit; `backend/g5-api/tests/chat_ontology_cards.rs` settles turns through the coordinator (a succeeded turn's card on its reply, none for a failed turn); vitest `OntologyChangesCard.test.tsx` and `ChatPanel.test.tsx`; the dev-stack Playwright flow `frontend/e2e/flows/chat-ontology-cards.spec.ts` with the fake Claude (a card for added statements, none for a code-only or stopped turn, an editor save during a turn on that turn's card, newest expanded and earlier collapsed, kept after reload and with the VM stopped).
- Gaps: a VM image older than this change records no start, so its turns have no card; live models were not exercised.

### g5.chat.ontology-card.governing

- human-provided, agent
- Source: the owner's 2026-10-28 question "should we also add unchanged but relevant / governing ontology statements?" and their choice of agent-named statements checked by the platform ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#governing-statements-on-the-card-2026-10-28)). These are agent choices, following the design plan's "Governing blocks that stay unchanged" list: one-line reasons, the limit of eight, collapsed by default, flagging unknown names, and read-only.
- On 2026-10-29 the owner chose a `g5d` command with which the agent records the list on its turn, over a per-turn file, parsing the reply or a new MCP tool ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#handing-over-the-governing-list-2026-10-29), [design](../../pr_ontology_diff_design/saehan-ontology-chat/ontology-plan.md#system-engineering-questions)). As System Engineering, the owner approved it on 2026-10-29.
- Implemented: `g5d chat governing` (`backend/g5d/src/cli/chat.rs`) checks the ID in the checkout and records it on the running turn; the runner resolves the list in the files as they stand when the turn ends; the agent harness names the command (`.agents/AGENTS.md`).
- Evidence (2026-10-29): `chat_ontology_cards.rs` (only the running turn's own VM and epoch may record, at most eight, a repeated ID replaces its reason, ended turns and other workspaces refused); `cli::chat` unit tests; the Playwright flow records a governing statement through the fake Claude's and the fake Codex's shell and shows it with its reason.
- Gaps: Codex's own shell-environment policy passing `G5D_TASK_ID` was not exercised (the fake Codex is a plain process); relevance is agent judgment, checked only for existence.

### g5.chat.ontology-card.edit

- human-provided, agent
- Source: the owner's 2026-10-28 question "Can we allow inline ontology statement editing on the card?" and their choices: inline editing on the newest card only ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#inline-editing-on-the-card-2026-10-28)). The owner confirmed on 2026-10-28 that the newest card stays editable after later turns without a card, and on 2026-10-29 limited editing to the workspace owner ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#scope-narrowed-to-workspace-chats-2026-10-29)). On 2026-10-29 the owner kept a record of each card edit with the card, over deriving the mark and the next agent's note from the file ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#recording-card-edits-2026-10-29)). These are agent choices: no editing while a turn runs or the VM is stopped, text edits only (no adding or deleting statements), the write-if-unchanged check, the "Edited" mark, and the next agent being told. On 2026-10-30 the owner chose a quiet mark with an explanatory tooltip and no way to view the agent's original text on the card ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#quieter-edit-mark-2026-10-30)).
- On 2026-10-29 the owner chose a note in the next turn's input over relying on the agent re-reading its files ([chat intent](../../chat_intent/saehan-ontology-chat/request.md#telling-the-next-agent-of-card-edits-2026-10-29), [design](../../pr_ontology_diff_design/saehan-ontology-chat/ontology-plan.md#system-engineering-questions)). As System Engineering, the owner approved it on 2026-10-29.
- Implemented: `POST /api/v1/sessions/:id/messages/:mid/ontology-card/edits` (`routes/chat.rs::edit_ontology_card`) with `chat_ontology::{plan_edit, replace_statement_body, record_edit}`, writing through the editor's live file read and `expected_hash` write; card-edit notices are queued again rather than withdrawn when an attempt or the workspace's VM ends first (`chat_service`).
- Evidence (2026-10-29): `chat_ontology_cards.rs` (stale card text, malformed text and a concurrently changed file refused without a write; one recording and one notice per key, a key reused for a different edit refused, and a recorded edit replayed after a turn has started; the notice kept through a workspace Stop; readers, earlier cards and running turns refused); `replace_statement_body` unit tests; vitest edit tests; the Playwright flow edits on the newest card, checks the file and the next turn's input naming the edit.
- Gap: if recording fails after the write and is not retried in time, the edit stays unrecorded (see the [design](../../pr_ontology_diff_design/saehan-ontology-chat/ontology-plan.md#limits-and-gaps)).

### g5.chat.replay

- human-provided
- Source: [Ben, line 231](../../../docs/ben_new_ontology.md#L231).
- Amendment: [#966 request](../../chat_intent/issue-966/request.md) and [design](../../pr_ontology_diff_design/issue-966/ontology-plan.md): shared execution identity and reduction for live and saved rows.

### g5.chat.presentation

- human-verified
- Source: [issue #817](https://github.com/G5Labs/G5-Lean/issues/817), assigned by the workspace owner.
- Human-provided amendment (workspace request, 2026-10-27): put assistant/agent messages and timeline activity in bubble boxes like user messages, with a different color.
- Human-provided amendment (workspace request, 2026-10-30): both bubbles size to their content with an eight-inch maximum width, with no other change.
- Scoped evidence and gap: [issue 817 validation](../../pr_ontology_diff_design/issue-817/ontology-plan.md) uses controlled browser transport fixtures; live Claude/Codex delivery was not exercised locally.

### g5.chat.presentation-owner

- agent
- Design: [shared chat turns](../../pr_ontology_diff_design/issue-817/ontology-plan.md).

### g5.chat.file-citations

- human-verified
- Issue [#875](https://github.com/G5Labs/G5-Lean/issues/875) and the owner's answers of 2026-10-27 in the Project Docs question set `docs/dc2d7087-7895-46c6-a5ca-f20e910fba19/questions/agent-file-links.g5q.json` (docs repository, `open-questions.md` beside it): chat replies only (`rule-surfaces`), agent-chosen link text (`link-text`), links open at the line (`line-references`), recorded as intent (`record-in-ontology`), unopenable files not covered (`non-openable-files`), Claude and Codex (`acceptance-providers`). [Design](../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md).
- Implemented in `.agents/AGENTS.md` (Chat file links section). Evidence, 2026-10-27 ([verification](../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md#implementation-and-verification--2026-10-27)): `dispatch_modules::agent_turn_claude_reads_the_provided_harness_outside_the_checkout` and `…codex_rejects_interactivity_before_spawn_and_runs_with_stdin_eof` assert that the instructions installed for each provider carry the rule (both fail without it); the browser flows show Claude and Codex replies' links open through the real dispatch with fake providers.
- Live check (`acceptance-evidence`), 2026-10-27, in the owner's full-stack preview of this branch: fresh real Claude and Codex agents each answered a question about a Project Doc and a code line with `g5://file` links (including `#L2`), and both links opened when clicked. The Claude run was agent-driven; the owner ran and verified the Codex run ([record](../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md#implementation-and-verification--2026-10-27)).
- Gap: one live run per provider can pass by chance (accepted in `acceptance-evidence`), and no automated check shows that models follow the rule.

### g5.chat.file-citations.instructions

- human-verified
- Owner answers of 2026-10-27 in the same question set: `.agents/AGENTS.md` only (`skills-and-prompts`) and the wording names G5 chat (`terminal-sessions`). [Design](../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md).
- Implemented: the rule is the Chat file links section of `.agents/AGENTS.md`, which the harness installs byte-for-byte as Claude Code's `~/.claude/CLAUDE.md` and Codex's `~/.codex/AGENTS.md` (`backend/g5d-agent/src/harness.rs`); no persona, workbench action or skill carries it. Evidence: the dispatch tests named under [g5.chat.file-citations](#g5.chat.file-citations).
- Gap: a person running a provider CLI in the workspace terminal may still get `g5://file` links.

### g5.chat.file-citations.repo-qualifier

- human-verified
- Owner answer of 2026-10-27 in the same question set: a repository only with several code repositories (`repo-qualifier-when`), accepting that older unqualified links stop opening when a code repository is added. [Design](../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md).
- Evidence: `WorkspacePageFileLinks.test.tsx` opens a `?repo=<repository id>` link in the named repository and refuses the unqualified link, with the candidates, in a two-code-repository workspace; a live click in such a workspace was not run.

### g5.chat.file-citations.repo-id

- agent
- The owner left the identifier to System Engineering (`repo-qualifier-identifier`: "Could you check what works in code?"). Checked 2026-10-27: `resolveDeepLinkRepoId` matches an exact repository id, then a unique G5 repository name; `/work/.g5/layout.json` lists `repo_id`, `role` and `root_path`, not the name, and each checkout's `origin` URL ends with its repository id.

### g5.chat.view-state

- agent
- Design and scoped validation: [shared chat turns](../../pr_ontology_diff_design/issue-817/ontology-plan.md).

### g5.chat.history-window

- human-verified
- Source: [issue #847](https://github.com/G5Labs/G5-Lean/issues/847) expected behavior and fix validation; marker, scroll loading and coverage rule are agent choices. Design: [history coverage](../../pr_ontology_diff_design/issue-847/ontology-plan.md).
- Implemented by `lib/historyCoverage.ts` (per-turn coverage), `AssistantTurn` (marker; no #819 head-gap prose for an incomplete turn) and `ChatPanel` (marker loading, reading anchor on the row at the viewport top). Evidence: `historyCoverage.test.ts`, `AssistantTurn.test.tsx` and `ChatPanel.test.tsx` ("history coverage") at the message/timeline boundary, the updated `chat-timeline-history-pagination.spec.ts`, and a Chromium harness check of loading range, anchoring, failure and retry; see the [design's validation](../../pr_ontology_diff_design/issue-847/ontology-plan.md#validation).
- Gap: the issue's QA passages need a fresh browser view after deployment.

### g5.chat.history-window.unloaded-text

- human-provided
- Source: the workspace owner chose withholding the text (option A) over showing it or a collapsed toggle; [chat decision](../../chat_intent/issue-847/unloaded-text.md). Design: [history coverage](../../pr_ontology_diff_design/issue-847/ontology-plan.md).

### g5.chat.history-refresh

- human-verified
- Source: [issue #847](https://github.com/G5Labs/G5-Lean/issues/847), "the loaded history should never shrink because of a background refetch". Design: [history coverage](../../pr_ontology_diff_design/issue-847/ontology-plan.md).
- Implemented by `useSessionTimeline` (`hooks/useChat.ts`), which replaced the infinite query, its five-page bridge cap and its drop. Evidence: `useChat.test.ts` ("useSessionTimeline"): one read per refresh, a read-back over nine pages, resumption after a cancelled read, history end, a prepend racing a refresh, failure and retry, session switch, and reading back to an open request until a reply row or the attempt's outcome settles it.

### g5.chat.native-permission-actions

- agent
- Based on the native reply adapter in `frontend/src/pages/WorkspacePage.tsx` and its supported response fields.

### g5.chat.native-requests

- human-provided
- Source: [Ben, line 232](../../../docs/ben_new_ontology.md#L232).

### g5.chat.controls

- human-provided
- Source: [Ben, line 233](../../../docs/ben_new_ontology.md#L233).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 3 (CH-Q2).
- Amendment: [#966 design](../../pr_ontology_diff_design/issue-966/ontology-plan.md#agent-task-controls): repeated-Stop convergence, disposition in the Stop response, `Stopping…` state.

### g5.chat.control-locks

- agent
- Source: the lock cycle reproduced in [#966](https://github.com/G5Labs/G5-Lean/issues/966) (seven QA Stops, one aborted in a database deadlock while output was still being replayed) and the [design](../../pr_ontology_diff_design/issue-966/ontology-plan.md#agent-task-controls); `TaskService::end_chat_turn` and its siblings in `task_lifecycle.rs`; regression `api_chat_turn_stop.rs` (two Stops behind a held advisory lock, a 20-request burst, a reconciliation waiter, a foreign share holder).

### g5.chat.cancel-outcomes

- human-provided
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 4 (Q1, L2-Q3, TK-cell-21).

### g5.chat.fork

- human-provided
- Source: [Ben, line 234](../../../docs/ben_new_ontology.md#L234).

### g5.chat.persistence

- human-provided
- Source: [Ben, line 235](../../../docs/ben_new_ontology.md#L235).
- Issue 817 local validation covers fixture reload/hydration, not real backend reconnect/persistence; see the [scoped evidence](../../pr_ontology_diff_design/issue-817/ontology-plan.md).

### g5.chat.ownership

- human-provided
- Source: [Ben, line 236](../../../docs/ben_new_ontology.md#L236).

### g5.chat.creation

- human-provided
- Source: [Ben, line 237](../../../docs/ben_new_ontology.md#L237).

### g5.chat.send-acceptance

- human-verified
- Source: owner-authorized [issue #629 fix plan](../../pr_ontology_diff_design/issue-629/ontology-plan.md).

### g5.chat.creation.idempotency

- agent
- Source: [workbench design](../../pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md) and [typed attachment design](../../ontology-plan-issue-231.md); specializes the [atomic creation boundary](#g5.chat.creation).
- [KeyedCreation](../../uplift/ben-ontology/quantitative-refinement/identity.md) checked scoped retry identity, atomic results, changed-attachment refusal and frozen persona resolution through rollback and lost responses. Database and storage primitives remain assumptions; other digest fields were inspected, and workspace-fork idempotency and account/tenant erasure are outside the check.
- Gaps: ordinary history forks accept but ignore the key. Physical child deletion would erase its retry fence, but no standalone production session-deletion caller was found.

### g5.chat.dispatch

- human-provided
- Source: [Ben, line 238](../../../docs/ben_new_ontology.md#L238).

### g5.chat.frozen-attempt

- human-provided
- Source: [Ben, line 239](../../../docs/ben_new_ontology.md#L239).
- Human-verified amendment: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-544/request.md). Original Ben source remains immutable; the source-edit ledger records this semantic change, not an editorial normalization.
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.attachment-delivery

- agent
- Source: [issue 231 request](../../chat_intent/issue-231/request.md) and [typed-content design](../../ontology-plan-issue-231.md), implemented in `28a692f1`; [coverage C13/F3](../../uplift/ben-ontology/reviews/coverage-identity.md#f3).
- [AttachmentDelivery](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked authorized, validated content before delivery, explicit materialization failure and separation of history references from current input bytes. Hashing, image decoding, private temporary-file integrity and provider comprehension remain assumptions; inspected runtime support covers images and explicitly refuses unsupported file content.

### g5.chat.runtime

- human-provided
- Source: [Ben, line 240](../../../docs/ben_new_ontology.md#L240).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.incarnation

- human-provided
- Source: [Ben, line 241](../../../docs/ben_new_ontology.md#L241).

### g5.chat.adapters

- human-provided
- Source: [Ben, line 242](../../../docs/ben_new_ontology.md#L242).
- Consolidates [native translation, line 263](../../../docs/ben_new_ontology.md#L263); its stable ID references this owner.
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.accepted-input

- human-provided
- Source: [Ben, line 243](../../../docs/ben_new_ontology.md#L243).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.journal

- human-provided
- Source: [Ben, line 244](../../../docs/ben_new_ontology.md#L244).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).
- Amendment: [#966 design](../../pr_ontology_diff_design/issue-966/ontology-plan.md): the journal as recovery source, cumulative acknowledgement with retry for ended attempts (`task_attempts.journal_acked_seq`).

### g5.chat.live-delivery

- agent
- Source: [#966](https://github.com/G5Labs/G5-Lean/issues/966) incident evidence and the [design](../../pr_ontology_diff_design/issue-966/ontology-plan.md#attempt-follower-api-process-local); implemented by the attempt follower in `g5-api-core` (`services/agents/attempt_follower.rs`).
- Capacity: one API replica per environment (`infra/helm/g5/values*.yaml`); the fan-out is process-local like the previous `chat.chunk` path.

### g5.chat.history-batches

- human-verified
- Source: the owner's five-second decision recorded in the [#966 issue](https://github.com/G5Labs/G5-Lean/issues/966) and [request](../../chat_intent/issue-966/request.md); batch mechanics in the [design](../../pr_ontology_diff_design/issue-966/ontology-plan.md#history-batch-taskservice); `TaskService::apply_journal_batch`.

### g5.chat.live-tail-recovery

- agent
- Source: [#966 design](../../pr_ontology_diff_design/issue-966/ontology-plan.md#tail-read); `GET /api/v1/sessions/:id/live-tail` and the browser's timeline merge (`lib/ws.ts`, `hooks/useChat.ts`).

### g5.chat.live-tail-recovery.render-cost

- Expands: [g5.chat.live-tail-recovery](#g5.chat.live-tail-recovery)
- agent
- user-facing-behavior
- test-coverage
- Source: [follow-up design](../../pr_ontology_diff_design/live-relay-followups/ontology-plan.md#3-the-transcripts-render-cost-follows-what-changed); `components/chat/unifiedTimeline.ts`, the memoized `AssistantTurn`, `lib/providerOutput.ts` (paragraph pieces).
- Verified by the browser harness `e2e/browser/chat-large-response.spec.ts` (mean commit at 2,000 held rows within 2× of 200, at 10,000 within 4×), `unifiedTimeline.test.ts` and `providerOutput.test.ts`.
- Known gap: at ten thousand held rows a commit still pays a dozen whole-window passes (projection, ordering, transcript filters, activity label, approval classification), 25 ms against 8 ms at two hundred; an incremental transcript index is the next step.


### g5.chat.settlement

- human-provided
- Source: [Ben, line 245](../../../docs/ben_new_ontology.md#L245).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.recovery

- human-provided
- Source: [Ben, line 246](../../../docs/ben_new_ontology.md#L246).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.cancellation

- human-provided
- Source: [Ben, line 247](../../../docs/ben_new_ontology.md#L247).
- Scope: cancellation cleanup differs from the explicit [normal-completion release](../software_architecture/workspace_vm.g5.md#g5.architecture.agent_execution_tree); normal turns need not kill agent-managed databases.
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.authentication

- human-provided
- Source: [Ben, line 248](../../../docs/ben_new_ontology.md#L248).

### g5.chat.claude-auth

- human-provided
- Source: [Ben, line 249](../../../docs/ben_new_ontology.md#L249).

### g5.chat.codex-auth

- human-provided
- Source: [Ben, line 250](../../../docs/ben_new_ontology.md#L250).

### g5.chat.credentials

- human-provided
- Source: [Ben, line 251](../../../docs/ben_new_ontology.md#L251).

### g5.chat.provider-transport

- human-verified
- Source: [accepted proxy removal](../../chat_intent/issue-577/request.md) and [design and native-client probes](../../pr_ontology_diff_design/issue-577/ontology-plan.md#llm-proxy-removal-assessment).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [App Server design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); local transport preserves the existing native endpoint/credential owner.


### g5.chat.accounts

- human-provided
- Source: [Ben, line 252](../../../docs/ben_new_ontology.md#L252).

### g5.chat.account-continuity

- agent
- Amended 2026-11-02 by [g5.chat.account-defaults](#g5.chat.account-defaults): removal clears same-scope assignments instead of refusing; the model's assignment-versus-deletion safety (no assignment names a deleted account) is preserved because both commit together.
- Source: [coverage C22/F6](../../uplift/ben-ontology/reviews/coverage-identity.md#f6), retained provider-account design; original human request not located.
- [AccountAssignments](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) preserved account identity, compatible assignments and assignment-versus-deletion safety with three settings writers sharing account locks. Database locking and same-organization identity are assumptions; management authority remains distinct from launch visibility and cannot override organization role ceilings.

### g5.chat.account-defaults

- agent
- Source: owner request (2026-11-02), recorded in [chat intent](../../chat_intent/account-defaults/request.md); design in [ontology-plan](../../pr_ontology_diff_design/account-defaults/ontology-plan.md). Evidence of the gap: `ProviderAccountService::create` writes no assignment; `ChatPanel` and `CloneDialog` fall back to the `runtime_config` sentinel while `RuntimeEnvConfig` (`legacyProviderKeysPresent`) already hides it without raw keys; `confirm_agent_launch` fails such a launch with `missing_llm_credential`. Owner decisions (2026-11-02): removal clears same-scope defaults, the Auto rule is included, no backfill for existing accounts.

### g5.chat.fork-start-account

- agent
- Source: owner request [G5Labs/G5-Lean#987](https://github.com/G5Labs/G5-Lean/issues/987) (2026-10-30). `components/workspace/CloneDialog.tsx` (`forkDefaultAccountValue`, the Account field on the Start work row), `components/agent/AgentLaunchConfigControls.tsx` (account props and explicit account preference), `lib/providerAccountSelection.ts` shared with the chat composer; the fork service already resolved `launch_preferences.account` for the new workspace (`ProviderAccountService::resolve_for_launch`). Tests: `CloneDialog.test.tsx` ("account for the first chat"), `AgentLaunchConfigControls.test.tsx` ("Account field").

### g5.chat.hints

- human-provided
- Source: [Ben, line 253](../../../docs/ben_new_ontology.md#L253).

### g5.chat.api-key-precedence

- human-provided
- Source: [Ben, line 254](../../../docs/ben_new_ontology.md#L254).

### g5.chat.login-before-prompt

- human-provided
- Source: [Ben, line 255](../../../docs/ben_new_ontology.md#L255).
- Human-verified amendment: the owner's expectation that messages sent during sign-in stay queued ([issue #1189](https://github.com/G5Labs/G5-Lean/issues/1189)) and its [design](../../pr_ontology_diff_design/issue-1189/ontology-plan.md). Replaces the earlier rule under which a new message superseded the pending login and abandoned the prompt.

### g5.chat.device-challenge

- human-provided
- Source: [Ben, line 256](../../../docs/ben_new_ontology.md#L256).

### g5.chat.failed-login

- human-provided
- Source: [Ben, line 257](../../../docs/ben_new_ontology.md#L257).

### g5.chat.failed-login.notice

- human-provided
- Source: [#810 additional scope](https://github.com/G5Labs/G5-Lean/issues/810#issuecomment-6392096323).

### g5.chat.error-notice

- human-verified
- Source: [owner's PR #812 follow-up](../../chat_intent/issue-810/error-presentation.md).
- Historical messages without a diagnostic marker or owned timeline evidence retain the generic error fallback; readers do not classify arbitrary assistant prose as a diagnostic.

### g5.chat.credential-home

- human-provided
- Source: [Ben, line 258](../../../docs/ben_new_ontology.md#L258).

### g5.chat.credential-replacement

- human-provided
- Source: [Ben, line 259](../../../docs/ben_new_ontology.md#L259).

### g5.chat.module

- human-provided
- Source: [Ben, line 260](../../../docs/ben_new_ontology.md#L260).

### g5.chat.module-input

- human-provided
- Source: [Ben, line 261](../../../docs/ben_new_ontology.md#L261).

### g5.chat.supervisor

- human-provided
- Source: [Ben, line 262](../../../docs/ben_new_ontology.md#L262).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).
- Amendment: [#910 request](../../chat_intent/issue-910/request.md) and [retirement design](../../pr_ontology_diff_design/issue-910/ontology-plan.md): every managed Codex attempt, including formal review, uses App Server.

### g5.chat.codex-configuration

- agent
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).

### g5.chat.codex-exec-retirement

- agent
- Amendment: [#910 request](../../chat_intent/issue-910/request.md) and [retirement design](../../pr_ontology_diff_design/issue-910/ontology-plan.md); refines the [#809 rollout](../../pr_ontology_diff_design/issue-809/ontology-plan.md#pin-approved-configuration-and-rollout), which retained exec for older guests and formal review.
- Implemented: `agent_dispatch.rs` (`frozen_module`, admission freezing `codex_transport`), `g5d-dispatch` module validation refusing Codex without App Server, `g5d-agent` runtime without an exec transport. Evidence: [design record](../../pr_ontology_diff_design/issue-910/ontology-plan.md#implementation-stage-1).
- Gap: stage 2 removes the API's exec-frozen Codex reconstruction after the per-deployment [inventory](../../pr_ontology_diff_design/issue-910/ontology-plan.md#stage-2-inventory-read-only-per-deployment) shows none; deployed inventories have not been observed.

### g5.chat.codex-runtime-refusal

- agent
- Amendment: [#910 retirement design](../../pr_ontology_diff_design/issue-910/ontology-plan.md#retirement-boundary); reuses the existing admission-refusal notice and kept input.
- Implemented in `reserve_queued_agent_dispatch` (`CODEX_RUNTIME_REFUSAL`, `CODEX_REVIEW_RUNTIME_REFUSAL`); `api_task_lifecycle::codex_app_server_module_and_transport_remain_frozen_on_redispatch` exercises older, unknown and missing catalogs for ordinary turns; `api_formal_agent_approvals::ready_formal_tasks_use_harness_modes_without_ordinary_conversation_settings` exercises one pre-formal catalog for formal review. Formal unknown/missing catalogs share the same `catalog_supports` check, unit-tested in `agent_dispatch.rs`.

### g5.chat.translation

- human-provided
- Source: [Ben, line 263](../../../docs/ben_new_ontology.md#L263).

### g5.chat.prompt-limit

- human-provided
- Source: [Ben, line 264](../../../docs/ben_new_ontology.md#L264).

### g5.chat.background-work

- human-provided
- Source: [Ben, line 265](../../../docs/ben_new_ontology.md#L265).
- Human-verified amendment: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-544/request.md). Original Ben source remains immutable; the source-edit ledger records this semantic change, not an editorial normalization.
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.background-stop

- human-provided
- Source: [Ben, line 266](../../../docs/ben_new_ontology.md#L266).

### g5.chat.usage

- human-provided
- Source: [Ben, line 267](../../../docs/ben_new_ontology.md#L267).
- Amendment: [#809 request](../../chat_intent/issue-809/request.md) and [migration design](../../pr_ontology_diff_design/issue-809/ontology-plan.md); implementation and executed coverage tracked in its [matrix](../../pr_ontology_diff_design/issue-809/coverage.md).


### g5.chat.subscription-usage

- human-verified
- Human request: [subscription meters](../../chat_intent/subscription-meters/request.md).
- Design and provider evidence: [live subscription meters](../../pr_ontology_diff_design/subscription-meters/ontology-plan.md).

### g5.chat.dispatch-secrets

- human-provided
- Source: [Ben, line 268](../../../docs/ben_new_ontology.md#L268).

### g5.chat.secret-admission

- agent
- Source: [issue #695 fix design](../../pr_ontology_diff_design/issue-695/ontology-plan.md); the config writer, both dispatch builders, the tenant dispatch loop and the guest parsers were inspected directly. Specializes [g5.chat.dispatch-secrets](#g5.chat.dispatch-secrets) and preserves [g5.chat.dispatch-eligibility](#g5.chat.dispatch-eligibility).
- The guest module parser stays looser than the producer; direct RPC injection is outside this policy.

### g5.chat.secret-admission.kept-input

- agent
- Expands: [g5.chat.secret-admission](#g5.chat.secret-admission).
- Source: [issue #695 fix design](../../pr_ontology_diff_design/issue-695/ontology-plan.md), following the unsupported-permission-policy precedent in `agent_dispatch.rs`.

### g5.chat.secret-admission.reconcile

- agent
- Expands: [g5.chat.secret-admission](#g5.chat.secret-admission); uses the attachment-preflight path of [g5.chat.attachment-delivery](#g5.chat.attachment-delivery).
- Source: [issue #695 fix design](../../pr_ontology_diff_design/issue-695/ontology-plan.md), options C4–C6.

### g5.story.reuse-native-memory

- human-provided
- Source: [Ben, line 269](../../../docs/ben_new_ontology.md#L269), with no additional harness and agent-controlled saving and pruning; on 2026-10-27 the owner replaced its sharing "across sessions and teammates through the codebase's Git" with private memory in the personal repository: "we want to go fully private" ([chat intent](../../chat_intent/personal-agent-memory/request.md)).

### g5.chat.native-question-recovery

- agent
- Source and evidence: [chat/identity reconstruction](../../uplift/ben-ontology/investigations/chat-identity.md). `NativeRecovery` checked ready first acceptance, a frozen answer, matching owner/session restoration and atomic answer/input settlement after transport-write evidence.
- The check covered one question across attachment and detachment, with owner/session changes only before reservation; later authority relies on runtime enforcement, and transport handoff does not prove model comprehension.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 5 (CH-Q1).
- Deferral condition, exit-time confirmation, bounded restoration and recovery: issue [#968](https://github.com/G5Labs/G5-Lean/issues/968) and its [design](../../pr_ontology_diff_design/issue-968/ontology-plan.md). Evidence: the pinned 2.1.308 CLI on Linux, driven by `scripts/test-native-claude-question-overlap.py` against a loopback model, reissues a plainly deferred question on resume, consumes a follow-up buffered before or just after the deferring hook reply and then reissues nothing, and completes live questions with a queued follow-up in one turn. The adapter gate (`handle_claude_control_frame`), confirmation (`native_question_lost`) and bounded restoration (`native_question_restore_failed`) are covered in `dispatch_modules.rs`; TaskService's hold, end reasons and retry disposition in `api_native_requests.rs`. Live questions keep the live-request contract on Stop, VM loss and transfer: the request ends with the process and the agent sees it unanswered when the owner continues.

### g5.chat.native-question-delivery-state

- human-provided
- Source: the owner's 2026-11-03 instruction that the card must not surface the new labels or cause explanations ([chat intent](../../chat_intent/issue-968/request.md)), narrowing issue [#968](https://github.com/G5Labs/G5-Lean/issues/968)'s "show saved, delivered and restoration-failed states accurately" to the resend action; [design](../../pr_ontology_diff_design/issue-968/ontology-plan.md) item 5 and its round 5. The timeline exposes `nativeRequestEndReason` and `nativeRequestAnswer` beside `nativeRequestStatus` so the card can offer the resend; the stopped label, the absence of explanations and the labeled answer continuation are covered in `AssistantTurn`/`ChatPanel` tests.

### g5.chat.native-request-boundary

- agent
- Source: [issue #628 fix design](../../pr_ontology_diff_design/issue-628/ontology-plan.md); the producer and API paths (`g5d-agent/src/runtime.rs`, `g5-api-core/src/services/agents/task_lifecycle.rs`) were inspected directly against the `provider_requests` persistence contract.
- Extends [g5.chat.native-question-recovery](#g5.chat.native-question-recovery) replay from deferred questions to every accepted answer. The same audit's job-rollup race (S23) was mooted by the #618 Jobs removal.

### g5.chat.session-fork

- agent
- Source and evidence: [chat/identity reconstruction](../../uplift/ben-ontology/investigations/chat-identity.md). `ChatFork` preserved transcript order, new message identities and destination attachment bindings, stopped unfinished assistant copies, and excluded queued input and native continuation.
- The bounded check used three messages and one attachment; transaction, storage and byte-integrity primitives were assumptions, not corruption proofs.

### g5.chat.dispatch-eligibility

- agent
- Source: [chat/identity supplement](../../uplift/ben-ontology/investigations/chat-identity.md); task, settings, lease and dependency locks were directly inspected.
- Bounded `ModuleLifecycle` checks preserved guest slot limits, one launch per execution identity, pre-invoke cancellation fencing and retained outcomes. An already reserved runner can still spawn after cancellation; competing database scheduler transactions and global fairness were not checked.

### g5.chat.dispatch-reachability

- agent
- Source: [issue #844](https://github.com/G5Labs/G5-Lean/issues/844) and [dispatch ownership design](../../pr_ontology_diff_design/issue-844/ontology-plan.md). Specializes `g5.chat.dispatch-eligibility` using the existing process-local host tunnel registry.
- Implemented in TaskService reservation/retry selection and task maintenance's active-attempt claims, using WorkspaceRuntime's live-tunnel snapshot. Six PostgreSQL/two-AppState regressions cover foreign-first claims, competing passes, transfer/reconnect, lost invoke replies, command/preview dispatch and stopped/replaced/expired incarnations; [the design](../../pr_ontology_diff_design/issue-844/ontology-plan.md#evidence) distinguishes these controlled-host results from real dispatcher and browser/CI validation.

### g5.chat.execution-defaults

- human-verified
- Source: [explicit default-16 request](../../chat_intent/workspace-execution-slots-16/request.md#human-instructions); the concurrency-only interpretation is retained agent design in that record. The completed backfill is not a recurring migration obligation.
- [ExecutionCapacity](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked the default of 16, admission up to that limit and queueing beyond it. It did not establish per-turn resource reservations or sufficient CPU and memory.

### g5.chat.execution-headroom

- agent
- Source: [catalog and compatibility design](../../pr_ontology_diff_design/workspace-execution-slots-16/ontology-plan.md), [coverage C12/F8](../../uplift/ben-ontology/reviews/coverage-identity.md#f8).
- [ExecutionCapacity](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) checked separate API reservations and VM invocation, operation headroom, old/new catalog limits and an idempotent-retry witness. The recorded catalog allowed 20 total slots—16 turns and two each for Changes and diagnostics—while old four-slot VMs allowed two turns; higher custom limits may wait unconfirmed, and fairness was not checked.

### g5.chat.spend-thresholds

- human-provided
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 6 (CH-Q3).

### g5.chat.spend-scope-question

- agent, uncertain
- Source: [coverage F4](../../uplift/ben-ontology/reviews/coverage-identity.md#f4), prior API/storage and settings design, and the newer v9 configuration target which retains spend ownership but marks full configuration authority post-MVP. Existing code/settings alone do not settle product scope.
- Provisional deferral recommends an explicit scope disposition; it is not a historical human decision or a claim that enforcement works. No new model assumes a spend cap. All-work/period accounting and admission enforcement need separate implementation qualification if retained.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 7 (CH-Q3).

### g5.agents.output-bounds

- human-provided
- architecture-decision
- test-coverage
- Source: the owner's [decision to remove the per-turn output limit](../../chat_intent/issue-1161/request.md) for [G5-Lean#1161](https://github.com/G5Labs/G5-Lean/issues/1161), which replaced the 64 MiB turn budget, its settlement reserve and the catalog ceiling of the [earlier contract](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/agents.g5.md); the per-line and per-event bounds keep their [owner-reviewed scope](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/pr_ontology_diff_design/issue-503/ontology-plan.md).
- The line ceiling is 16 MiB, an event 1 MiB, the stderr log 8 MiB and the control-reply bound 1 MiB; the bound's value and the `provider_control_backlog` notice are the implementing agent's choices. There is still no oversized-ID guard: a rejection echoes the provider's request id and the reply bound holds it.
- Compatibility: the API keeps sending `max_output_bytes` (64 MiB) because a worker from before the removal refuses a turn without it; a current worker accepts and ignores it, and its catalog refuses an `agent.turn` entry that declares an output cap. Stop sending the field only when no such worker can receive a turn.
- Gaps the owner accepted with the removal: the VM journal can grow while the API is not acknowledging events. Found while implementing it and not yet decided: nothing caps what one turn adds to the API's stored timeline, and for the length of a turn the supervisor keeps the text of the final messages it recorded and the ids of its native requests and tool uses.
- Tests: [dispatcher and supervisor](../../../backend/g5d/tests/dispatch_modules.rs) (`agent_turn_ignoring_over_64_mib_of_background_agent_frames_succeeds_and_saves_its_session`, `agent_turn_recording_over_64_mib_of_events_completes_with_or_without_the_retired_limit`, `agent_turn_floods_run_until_the_owner_stops_them`, `agent_turn_flood_of_unread_control_replies_ends_as_a_provider_error`), the [reply queue](../../../backend/g5d-agent/src/runtime.rs) and the [catalog](../../../backend/g5d-dispatch/src/modules/catalog.rs).

### g5.conversation.delivery.steering

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-544/request.md).
- The owner’s October 15 direction supersedes replay-confirmation experiments. Upstream process, API and UI tests cover flush-before-read; pinned recordings are protocol evidence, not live-model qualification. A receipt no longer needs to survive an output limit: a turn [has none](#g5.agents.output-bounds).
- Admission closed by a deferring question hook: issue [#968](https://github.com/G5Labs/G5-Lean/issues/968), [design](../../pr_ontology_diff_design/issue-968/ontology-plan.md) item 1; the window between the hook reply and the `tool_deferred` result was shown unsafe on the pinned CLI.

### g5.conversation.delivery.steering-recovery

- human-verified
- Receipt and settlement clarification: owner-authorized [issue #629 fix plan](../../pr_ontology_diff_design/issue-629/ontology-plan.md).
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-544/request.md).

### g5.conversation.providers.claude-followups

- agent
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-544/request.md); [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/conversations.g5.md).

### g5.chat.ui.header-title

- human-provided
- Source: [issue #823](https://github.com/G5Labs/G5-Lean/issues/823), feature 1. Design: [chat activity and per-chat recall](../../pr_ontology_diff_design/issue-823/ontology-plan.md).
- Evidence: `ChatPanel.test.tsx` (header rename, Escape, failure, reader) and the browser lane `e2e/browser/chat-header.spec.ts` (center and panel layouts at 1100, 420 and 320 px: single line, truncation only when needed, hover title, pencil placement, no overflow); `e2e/flows/chat-activity.spec.ts` renames through the stack.

### g5.chat.ui.picker

- human-provided
- Source: issue #823, feature 2. The exact time wording was left to design; the issue's proposed form is kept and `No finished turns` is an agent choice.
- Evidence: `ChatPanel.test.tsx` (picker rows, indicators, times, search, no rename controls) and `e2e/browser/chat-header.spec.ts`. The center no-chat recent list uses the same order and activity time.

### g5.chat.activity

- human-provided
- Source: issue #823, feature 3.

### g5.chat.activity.turns

- agent
- Source: issue #823 asks design to map task states to turns and to keep resumable work from spinning; the mapping follows `tasks.status` and `task_attempts` as settled by [g5.chat.settlement](#g5.chat.settlement). Failed and stopped turns counting as finished follows the issue's recommendation.
- Amended during implementation: a VM stop or release leaves queued agent tasks queued (`settle_agents_after_release` only cancels previews), so queued counts as active only while `workspace_vms` is starting or running or carries a pending start intent.
- Implemented in `backend/g5-api-core/src/services/agents/chat_activity.rs`; `backend/g5-api/tests/api_chat_activity.rs` covers running, queued with and without compute, held, refused-before-attempt and successful turns against real Postgres and dispatch.
- Amendment ([G5-Lean#984](https://github.com/G5Labs/G5-Lean/issues/984)): a pending automatic continuation start keeps the queued turn active, the same as an owner's pending start intent; see the [design](../../pr_ontology_diff_design/issue-984/ontology-plan.md).

### g5.chat.activity.awaiting-input

- agent
- Source: the owner's chat bubble stack request, "waiting for a question" including permission prompts ([chat intent](../../chat_intent/chat-bubble-stack/request.md), [design](../../pr_ontology_diff_design/chat-bubble-stack/ontology-plan.md)). `reply_pending` not counting as waiting and a timed-out request reading as a finished result are agent choices.
- `SessionActivity.awaiting_input` in `chat_activity.rs` (one `EXISTS` over the session's tasks and their `provider_requests`); `TaskService::apply` hints after `provider_request.*` journal events. `api_chat_activity.rs` covers a pending permission on a running attempt (with the hint), its reply, and a deferred question holding the turn; `chatSessionActivity.test.ts` covers the client derivation.

### g5.chat.activity.order

- human-provided
- Source: issue #823, feature 4. The tie-breaker (session id, descending) is an agent choice.
- The recent-conversations rule is human-verified from the [owner's choice](../../chat_intent/issue-823/recent-conversations.md) that renaming alone must not move a chat to the top.
- Evidence: `api_chat_activity.rs` (active-first by turn start, finish order, creation fallback, rename and inspection not reordering, ordering before the 50-chat limit) and `e2e/flows/chat-activity.spec.ts` with fake Claude and Codex.

### g5.chat.inspection

- human-provided
- Source: issue #823, feature 3 and its acceptance criteria.

### g5.chat.inspection.acknowledge

- agent
- Source: issue #823 recommends acknowledging the specific displayed result rather than an "opened at" time and treating historical results as inspected at rollout; the visibility, scroll and output conditions are agent choices.
- Implemented by migration 116 (`chat_session_inspections`, `chat_sessions.inspection_floor_at`), `PUT /api/v1/sessions/:id/inspection` and `hooks/useChatResultAcknowledgement.ts`. Evidence: `api_chat_activity.rs` (personal, reader-writable, monotonic, refused foreign/unsettled results, rollout floor), `migration_116_chat_activity_inspection.rs` (populated upgrade as a non-superuser runner), `ChatPanel.test.tsx` (hidden surface, unfocused page, scrolled away, stale transcript, newer result) and the stack spec (background completion, unfocused page, second-tab convergence).

### g5.chat.activity.refresh

- agent
- Source: issue #823 asks to reuse workspace task events, batch the list query and avoid per-chat polling. No workspace-channel task event was published before this change; the WebSocket hub fans out in-process only, so the hint is advisory and the list stays authoritative.
- Evidence: `api_chat_activity.rs` (hint on enqueue and settlement, none for streamed output, acknowledgement only to its person), `api_ws_revocation.rs` (no hint to a socket a transfer revokes) and `ws.chatActivity.test.ts`. Not every lifecycle writer hints (VM release paths without a scoped transaction rely on host VM events and the refresh); the 15-second refresh covers the rest.

### g5.chat.ui.prompt-history

- human-provided
- Source: [issue #810](https://github.com/G5Labs/G5-Lean/issues/810), part 2, and the owner's PR #812 workspace follow-ups specifying Codex-style browsing: end caret, consecutive Up/Down across multiline prompts, other caret navigation or editing exits browsing, and clearing the composer enables it again. This replaces the earlier draft-at-start and line-boundary rules. `ChatPanel.test.tsx` ("prompt history") covers these transitions, oldest/newest boundaries, edit-and-undo without re-entry, pointer navigation, workspace changes, modified keys, IME composition and the reference picker. G5 has no slash-command or mention autocomplete in the composer today.
- Amendment: [issue #823](https://github.com/G5Labs/G5-Lean/issues/823), feature 5, scopes history to the current chat (it was shared across a workspace's chats) and requires survival across reload and VM stop/start.
- Amendment evidence: `ChatPanel.test.tsx` ("prompt history") adds chat A/B isolation, a switch during browsing, a new chat's first send, a send resolving after a switch, workspace changes and reload; `e2e/flows/chat-activity.spec.ts` recalls per chat across a reload and a VM stop/start.

### g5.chat.ui.prompt-history.store

- agent
- Source: issue #810 asks for Claude Code and Codex desktop parity, which recall earlier sessions' prompts; storage and limits are agent choices. History does not follow a person to another browser.
- Amendment: issue #823 makes the buffer per chat and suggested starting per-chat buffers empty; the browser-wide 200-chat and 1,000,000-character bounds are a necessary correction because per-chat buffers multiply storage. Design: [issue 823 plan](../../pr_ontology_diff_design/issue-823/ontology-plan.md).
- Evidence: `chatPromptHistory.test.ts` covers per-chat scoping, legacy discard, chat and character budgets, unavailable storage, malformed storage and sign-out clearing.

### g5.chat.ui.device-code-copy

- human-provided
- Source: [issue #810](https://github.com/G5Labs/G5-Lean/issues/810), part 1. `CodexDeviceAuth.test.tsx` ("device code copy") covers the exact code, the brief confirmation, refetch/remount/reload, a new attempt, a rejected or missing Clipboard API with the manual Copy action, and codes that are not shown.

### g5.chat.ui.device-code-copy.once

- agent
- Source: issue #810's replay criterion, read as once per tab; the backend lets an attempt publish only one challenge. A reload keeps the record; a different tab copies the same challenge again. The unfocused-page wait is covered by `CodexDeviceAuth.test.tsx`.
