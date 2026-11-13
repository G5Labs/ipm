# Ontology changes cards in workspace chats

Intent: [chat intent](../../chat_intent/saehan-ontology-chat/request.md), 2026-10-28 and 2026-10-29.
Mocks: Project Docs `docs/fafcf403-8fad-4752-82c7-a61ba8a12094/mocks/index.html` (the `mock` preview profile); flow 2 is in scope.

When the ontology changes during a regular workspace chat's turn, the turn's reply ends with a changes card. The card compares the ontology at the turn's start with the ontology at its end. It shows each changed statement with its Before text, whoever made the change, and the unchanged statements that govern the changes. The workspace owner can correct a statement's wording in place on the newest card, and the next turn works from the correction. A turn without ontology changes has no card.

**Scope (owner, 2026-10-29):** regular workspaces only. The personal chat has no cards. The earlier Ontology-mode design for the personal chat (proposals, a Base check, creating and updating an ontology proposal issue) was removed. Its decisions stay recoverable in Git and in the [chat intent](../../chat_intent/saehan-ontology-chat/request.md).

## Ontology changes

| Block | Change | Summary |
|---|---|---|
| [`g5.chat.ontology-card`](../../ontology/features/chat.g5.md#g5.chat.ontology-card) | New (owner) | A turn during which ontology statements changed ends with a card of the changes between the turn's start and end, whoever made them, with Before text and no total; no card otherwise; none in the personal chat. |
| [`g5.chat.ontology-card.governing`](../../ontology/features/chat.g5.md#g5.chat.ontology-card.governing) | New (owner) | Each card lists, collapsed, up to eight unchanged governing statements the agent names with reasons. The platform checks they exist and shows their text, and flags unknown names. |
| [`g5.chat.ontology-card.edit`](../../ontology/features/chat.g5.md#g5.chat.ontology-card.edit) | New (owner) | The workspace owner edits an added or modified statement in place on the newest card. The write happens only if the statement is unchanged, the result is marked as edited, and the next turn's agent is told. |

Governing blocks that stay unchanged: [turns](../../ontology/features/chat.g5.md#g5.chat.activity.turns),
[Statement changes](../../ontology/features/documents.g5.md#g5.documents.ontology.statement-changes) and its
[comparison matching](../../ontology/features/documents.g5.md#g5.documents.ontology.comparison-matching),
[message identities](../../ontology/features/chat.g5.md#g5.chat.replay), and
[reference checkouts](../../ontology/features/personal_workspace.g5.md#g5.workspace.identity.reference), which explain why the personal chat has no cards.

## Owners and interactions

- **Changes card (in-VM runner, chat API, frontend chat transcript).**
  - When a turn starts, the runner records the start of the workspace's ontology files (`.g5/ontology/**/*.g5.md` in each code member) as a commit plus the difference from it ([A](#system-engineering-questions), System Engineering, 2026-10-29). When the turn's result ends, it rebuilds the start files and compares them with the files as they stand, statement by statement, under the shared matching rule with `g5_semantic::ontology_comparison`, the server-side twin of the browser's `compareOntologyStatements`. The runner already runs inside the `g5d` binary, which includes `g5-semantic`, so this adds a dependency to `g5d-agent` but nothing to the binary. A turn spans its retries and continuations, so the snapshot belongs to the turn, not to one attempt.
  - Every change in between is included, whoever made it: the agent through its edit tools or shell commands, an editor save, or another chat's overlapping turn. The card does not attribute changes.
  - The result is stored with the turn's settled reply, so the card stays fixed after later turns, after a reload and while the VM is stopped. The browser renders it with `selectOntologyChanges` and the Statement changes presentation (`OntologyBeforeStatement`, plus the current/Before pair from `OntologyChangesView`, exported for reuse). It has no total, and a "View all changes" link to [All changes](../../ontology/features/documents.g5.md#g5.documents.ontology.all-changes), the workspace's whole change against its parent rather than this turn's.
  - A turn without statement changes has no card. That includes code-only turns and changes only to metadata or citations. A turn the person stopped, or one that failed, has no card either, even if it changed statements (owner, 2026-10-29); its changes show in Compare. Only a turn whose final result succeeded has a card. A turn that timed out or exceeded a limit counts as failed, and a cancelled one as stopped. An interrupted or suspended attempt doesn't end the turn, so its changes appear on the card of the attempt that finishes it.
  - A card renders its first 5 statements, grouped by file in file order, and a Show more control reveals the rest in place. Edit is available on revealed statements of the newest card as on the others. A file that can't be compared shows its refusal inside the card. The newest card is expanded and earlier ones are collapsed.
  - The personal workspace never shows cards.
- **Governing statements (agent, frontend).**
  - Each turn, the agent names the unchanged statements governing its changes, up to eight, each with a one-line reason. It uses the judgment it applies to a design plan's "Governing blocks that stay unchanged".
  - The platform resolves each name against the workspace's ontology at the turn's end, shows the statement's text with the reason labelled as the agent's, and flags a name it can't find. The agent records the list with a `g5d` command, which checks each name when it is recorded ([E](#system-engineering-questions), System Engineering, 2026-10-29).
- **Editing on the newest card (frontend, the editor's write path).**
  - Only the workspace owner sees a working Edit, on each added or modified statement of the newest card, while no turn runs and the VM runs. A turn waiting for the person's answer still counts as running. Others reading the chat see Edit disabled with the reason, as they do for Preview and the terminal.
  - Saving is one API operation with a retry key. The browser sends the card, the statement's ID, its text as the card shows it, the new text and the hash of the file it read. The API replaces that statement's text in the workspace's ontology file through the editor's existing file write, which already refuses a write when the file's hash differs (`expected_hash`). The write is refused if the file's statement differs from the card, for example after a concurrent agent or editor change, and malformed statement text is refused by the parser the editor uses.
  - Only after the VM confirms the write does the API record the edit with the card: the statement, its file, the owner's text and the time (owner, 2026-10-29). Once the write has landed, the edit is recorded even if a turn has started since. If recording fails, the operation reports failure and the browser retries with the same key. On the retry, a file that already holds exactly the new text is not written again, and only the record is completed. So an edit is never written twice.
  - The card keeps the agent's text and shows the owner's text marked as edited, from that record, also after a reload and while the VM is stopped. Card edits happen only between turns, so they never fall inside another turn's start-to-end comparison. The platform tells the next turn's agent of them in a short note added to its input ([D](#system-engineering-questions), System Engineering, 2026-10-29).

## Implementation decisions (2026-10-29)

Found while implementing; each keeps the owner's decisions above.

- **The start snapshot goes through the workspace-agent API, not the runner's input.** The runner's input is frozen per `agent.turn` module version and rejects unknown fields, so a new field would need a module and catalog bump and would break older VM images. The dispatcher's journal also refuses events over 1 MiB. So the runner reads its turn's snapshot with its workspace credential and, when the turn has none yet, records one before the provider starts (`/api/internal/workspace-agent/:id/chat-tasks/:taskId/ontology`). The API accepts it only for a running agent task of that workspace on the calling VM and epoch, keeps the first one recorded and drops it when the turn ends. A runner on an older VM image never calls it, so its turns simply have no card.
- **The card travels as one bounded event and is stored on the reply's message.** When an attempt succeeds, the runner emits an `ontology.card` event of at most 768 KiB. The API copies it onto the turn's last assistant message (`chat_messages.artifacts.ontology_card`) only when the turn settles as succeeded. The message keeps its identity, reload and live updates agree, and the card survives the 30-day pruning of task events. A card that would exceed the bound is replaced by one that says the turn changed too many statements to show and links to All changes. A fork copies the chat's cards with its messages.
- **Statements are stored, not files.** Each change keeps its statements' authored text and footer metadata. The browser rebuilds them with the editor's own parser and renders them with the All changes presentation, so a card costs the size of its changed statements rather than its files.
- **Card edits and the next agent's note.** A card edit is recorded on the same message, with its retry key, and in the same transaction the platform queues a platform notice for the chat ([edit](../../ontology/features/chat.g5.md#g5.chat.ontology-card.edit)). The existing notice delivery adds it to the next turn's input; like other platform notices, it also shows in the chat. The API validates the edited file with `g5_semantic::ontology_comparison`, the server twin of the editor's parser, and refuses a file with CRLF line endings rather than rewriting them.
- **The governing list.** `g5d chat governing --add <id> --reason <text>` checks the ID against the checkout's ontology and records the ID, its file and the reason on the running turn through the same workspace-agent route; a ninth distinct ID is refused. At the turn's end the runner reads the list back and resolves each ID in the files as they stand; a name it can't find is flagged.

## Choices

- **Changes between the turn's start and end, no total.** The owner chose this on 2026-10-29, replacing "this chat's agent's changes only", because attributing each change to one writer is hard: an agent can edit through shell commands as well as its edit tools, and the editor and other chats write the same files. The card answers "what changed in the ontology during this reply". Compare already shows the workspace's whole change.
- **No cards in the personal chat.** Its agent's edits land in reference checkouts, which are discarded at the next Start and never published.
- **Agent-named governing statements, checked by the platform.** These are the owner's choice over link-only selection. Link-only selection misses rules that matter without a link, and agent-only selection is unchecked.
- **Inline editing, owner only, newest card only.** These are the owner's choices. Older cards stay a record of their turns, and a write that might overwrite a concurrent change is refused rather than merged.

## Limits and gaps

- **No attribution.** An editor save or another chat's overlapping turn shows up on this turn's card as if it were part of the reply. Two chats running at once both show the changes from the overlap. The card doesn't say when another writer was active either; the owner decided against such a note on 2026-10-29.
- **Model behavior.** The governing list is agent judgment, which CI checks only for existence, not relevance.
- **Card size.** A turn can change many statements; the real #776 data in the mock has eleven in one turn and four files in another. A card shows 5 statements before Show more (owner: 10 on 2026-10-29, 5 on 2026-11-02), and cards collapse when they're no longer the newest. The comparator's existing limits (5,000 statements, 10 MiB) still apply; beyond them the card shows the refusal.
- **Stopped and failed turns.** Their changes have no card, so a reader of the chat sees them only in Compare.
- **An unrecorded card edit.** If recording fails after the file was written and the owner does not retry while the card is still the newest with no turn running, the file keeps the edit without the card's mark or the next agent's note.

## System Engineering questions

The owner asked for options and a recommendation on each question. The owner, who is also System Engineering, decided A, D and E on 2026-10-29.

Relevant facts in the code today:
- The in-VM runner (`g5d-agent`) sees each agent's file-edit events: Claude's `Write`/`Edit` tool events with `file_path`, and Codex's `file_change` items. It does not see edits made through shell commands.
- The browser editor writes files through the `workspaces.write_file` route, so its saves are attributable.

- **How the agents themselves track a turn's file changes** (the owner asked, 2026-10-29). This is background for question A, as far as it is known from the agents' public behavior and our runner's events.
  - *Claude Code* reports each edit-tool call (`Write`, `Edit`, `MultiEdit`, `NotebookEdit`) with the file path and, in the tool result, a patch for that one call. Its checkpoints back up a file before its own edit tools change it, so a rewind restores those files. It keeps no turn-level diff, and edits made through Bash commands are in neither. Our runner sees the tool events (`ToolStarted`/`ToolCompleted` with `tool_name`).
  - *Codex* reports `file_change` items listing the paths and kind of change its patches made, which our runner forwards as `changes`. Internally it keeps a turn diff: a baseline copy of each file is taken the first time its own patch tool touches it in the turn, and that baseline is compared with the file at the end. Files changed only by shell commands are not in it. A file it did touch is compared as a whole, so another writer's edit to that same file does show up.
  - Neither agent's own record covers shell edits or files it never touched, and the two work differently. That is why the card compares snapshots of the ontology itself at the turn's start and end, the same way for both agents.
- **A. Where the turn's snapshots are taken and how the card is stored.** In every option the in-VM runner compares at the turn's end, and only the card's result is stored with the turn's settled reply. The options differ in what the start snapshot holds. A full copy of this repository's ontology is 25 files, about 860 KiB, or about 250 KiB compressed, for every turn (owner: too large, 2026-10-29).
  1. *A durable commit plus the difference from it.* For each code member, the runner records when the turn's first attempt starts:
     - the base commit: HEAD if the server already has it, otherwise the newest commit HEAD shares with the pushed branch (`git merge-base HEAD origin/<branch>`), so a continuation after VM loss can still read it;
     - a patch of the tracked ontology files against that base (`git diff <base> -- .g5/ontology`), covering staged, unstaged, deleted and renamed files, and changes in unpushed commits;
     - the full text of untracked ontology files, found with `git --no-optional-locks status --porcelain=v2 --untracked-files=all -- .g5/ontology`.

     These are stored with the turn. At the end, the runner rebuilds each start file from the base commit and the patch, which applies cleanly because it was made from that exact commit. In the usual case, with HEAD pushed and no uncommitted ontology edits, the snapshot is only the commit ID, and otherwise it is about the size of the uncommitted ontology work. It writes no Git state: no commit, stash, ref or index change. Commits, rebases, resets and branch switches during the turn don't affect it. If the base commit's objects are gone locally, the runner fetches them from the server by ID.
  2. *A full copy of the ontology files,* stored with the turn. It is the simplest and doesn't depend on Git, but it is large for every turn.
  3. *Files stored once by content hash,* with each turn recording only its paths and hashes. It is small after the first turn and doesn't depend on Git, but it needs a new server-side store and cleanup of files no turn uses.
  4. *A Git tree written from a temporary index,* with its ID recorded. It is the smallest, but the objects stay only in the VM: they are lost with the VM and `git gc` may delete them. Pushing them to a private ref, as the flush's recovery refs do, would fix that, at the cost of a push every turn and private refs that keep growing.
  5. *Platform-side.* Read the files through the API at the start and end. No VM change, but two reads of every ontology file per turn, through the file route.

  **Decision: 1** (System Engineering, 2026-10-29). It records "the start of the turn" exactly, costs almost nothing in the usual case, survives a VM loss mid-turn and touches no Git state.
- **D. How the next turn's agent learns of edits made on a card.**
  1. *Dispatcher notice.* The platform records card edits since the last turn and adds a short note to the next input's prefix, as it does for platform notices: "The person edited `<id>` in `<file>`". It is deterministic and works for Claude and Codex alike.
  2. *The agent re-reads its files each turn.* There is nothing to build, but an agent that keeps the files in its context may overwrite the edit.

  **Decision: 1** (System Engineering, 2026-10-29). The edit must not be silently undone by the next reply.
- **E. How the agent hands over the governing list, and where the platform reads its text.**
  1. *A per-turn file.* The agent writes `governing.json` (ID, file, reason) in a session scratch area. The platform reads it when the turn settles.
  2. *A `g5d` command* such as `g5d chat governing --add <id> --reason …` that records the list on the turn. It is explicit and validated on entry, but adds CLI surface. It follows how the agent already uses platform features, such as `g5d issue` and `g5d question create`, through the `g5d` shim on its PATH, and works the same for Claude and Codex. The command reads its turn from `G5D_TASK_ID`, which the runner sets in the agent's environment and a continued turn keeps, so the list survives a VM loss mid-turn. The agent never passes it. The ID is a claim, not proof, because `g5d` signs in as the workspace VM rather than as a task: the server accepts it only for a chat task of this workspace whose turn is running. The harness tells the agent to use it.
  3. *Parse it from the reply's text.* Nothing to build, but fragile.
  4. *A G5 tool through MCP,* recorded by the runner as an event, as the runner records Claude's native `AskUserQuestion` call for the question card. It is closest to the question card, but G5 gives the agents no MCP server today, so it means building one for both agents, for the same result as 2.

  **Decision: 2** (System Engineering, 2026-10-29). Recording the list on the turn makes it part of the card the platform stores (question A), and an unknown ID is caught immediately. For the text, the platform resolves IDs with the same reads the card uses, again at the turn's end, so a statement removed after it was named is flagged.

Questions B (the personal chat's card) and C (reading the latest Base for the personal agent) were removed with the personal-chat scope on 2026-10-29.

## Assumptions checked during implementation

- Verified: the card rides on the turn's existing reply message, so the [message identities](../../ontology/features/chat.g5.md#g5.chat.replay) are unchanged; reload returns it on the same message.
- Verified: the card-edit operation reuses the editor's live file read and write (`workspace_file_save::read_live_file` and `write_live_file` with `expected_hash`), as the owner, without the browser calling `workspaces.write_file`.
- Verified: the workspace-agent routes accept a task only for a running agent turn of the calling workspace on its VM and epoch, outside the personal workspace; an ended turn, another chat's VM and another workspace are refused. A misbehaving agent could still name another chat's running task on the same VM; that is no weaker than today, since both agents run as one user and can edit each other's files.
- Verified on 2026-10-29: the workspace's Git server lets a VM fetch a commit by its ID. The snapshot is sent outside the event stream, and the card event is bounded below the journal's 1 MiB event limit.
- Not verified: that the real Codex passes `G5D_TASK_ID` to its shell commands. G5 sets no Codex shell-environment policy, and Codex's documented default drops only names containing `KEY`, `SECRET` or `TOKEN`; the fake Codex used in tests is a plain process.

## Testing

- **Frontend unit:**
  - Changes card: every statement changed between the turn's start and end, Before text from the start, no total, no card for a code-only or metadata-only turn or for a stopped, cancelled, failed, timed-out or limit-exceeded turn, 5 statements then Show more, per-file refusal, newest expanded, earlier collapsed, unchanged after reload and while the VM is stopped.
  - Governing list: collapsed by default, at most eight, read-only and uncounted, the reason labelled as the agent's, and an unknown ID flagged.
  - Card editing: Edit only for the owner, only on the newest card while no turn runs and the VM runs, and disabled with the reason for readers. A conditional write is refused when the statement changed; a malformed statement is refused; the edited mark and the agent's text.
- **Runner unit:** the start snapshot is only the commit ID when HEAD is pushed and the ontology is clean; with staged, unstaged, untracked, deleted and renamed ontology files and an unpushed ontology commit, the start files are rebuilt exactly; Git's index and refs are unchanged; a commit, rebase or branch switch during the turn still compares against the start.
- **API integration:** the start snapshot survives a turn's later attempts; the card is stored with the turn's result and returned on reload; a non-owner's card write is refused; a card edit is recorded only after its file write is confirmed, and a retry after a failed record completes it without writing the file again; the next turn's prompt names the owner's card edits. The `g5d` governing command records on its chat's running turn, refuses an unknown ID, and is refused after the turn ends, for another chat's turn and for another workspace's task; a list recorded before a VM loss is still on the turn after its continuation.
- **E2E, workspace chat** (Playwright, `G5_CLAUDE_CODE_FAKE`): a scripted turn edits an ontology statement and shows a card. A code-only turn shows none. An edit made through a shell command appears. A concurrent editor edit to another statement made during the turn appears on that turn's card. An owner edit on the card is saved and named to the next turn. A Codex turn's shell sees `G5D_TASK_ID`, and its `g5d` governing command records on that turn.
- **Manual, with a live model:** governing lists for real changes, and cards for turns that edit the ontology through shell commands.
