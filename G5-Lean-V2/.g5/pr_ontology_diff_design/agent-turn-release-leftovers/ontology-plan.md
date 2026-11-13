# Release an agent's leftover processes after a normal turn

> Ontology rebase note: links to current owners were retargeted. Immutable main-commit links preserve the historical ontology/source context of this record; they are not additional current contracts.


Intent: [chat record](../../chat_intent/agent-turn-release-leftovers/request.md).
In QA, two finished Codex turns never settled because each agent had started
PostgreSQL with `sudo`. The turn's execution owner runs as uid 1000, adopts that
uid-`postgres` daemon, cannot signal it and waits for it forever; the dispatcher records
the turn only after the owner exits, so queued messages waited indefinitely.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| [`g5.architecture.agent_execution_tree`](../../ontology/software_architecture/workspace_vm.g5.md#g5.architecture.agent_execution_tree) | The owner keeps the dispatcher identity; only the supervisor drops to the module identity. Forced reap now follows stop, cancellation, interruption, limits, supervisor-forced outcomes, failed turns and supervisor failure, but not a normal provider finish, which releases the remaining descendants. Capacity is held until reap proof or that release. | The owner must out-rank whatever the agent can start through the guest's sudo, and the owner wants work it deliberately started (a test database) to survive into the next turn. |
| [`g5.conversation.providers.claude-background.untracked`](../../ontology/software_architecture/workspace_vm.g5.md#g5.architecture.agent_execution_tree) | A detached process keeps running after a normal finish and is stopped only when the attempt is stopped, limited or fails. | Same release rule; it still never extends the attempt. |

Unchanged and relied on: [Stop/Cancel must end an attempt in every state](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/agents.g5.md#owner-stop-reliably-ends-any-agent-attempt),
[outcome evidence](../../ontology/software_architecture/workspace_vm.g5.md#g5.architecture.agent_execution_tree)
(success still needs the provider's terminal result, output closure and exit) and
[guest sudo as MVP compatibility](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/security.g5.md#treat-guest-user-setup-as-compatibility).

## Design

```
dispatcher (root) ── private socket ──> execution owner (root, child subreaper)
                                           └─ agent-turn (module uid) ─> provider ─> tools / daemons
```

| How the turn ends | Supervisor exit | Owner action |
|---|---|---|
| Provider result succeeded, or native question deferred with a clean provider exit | release status | Release: write receipt, exit 0, leftovers adopted by the VM init |
| Supervisor-forced outcome (interrupt, setup deadline, frame/read limit, ended background wait, expired or undeliverable native request), an owner End that took effect, a reached turn budget (`max_turns_reached`), provider error, or an interruption/deadline the supervisor sees while finalizing | 0 | Reap every descendant, then receipt |
| Dispatcher stop (cancel, user Stop, output/log/journal limit, dispatcher loss) | any | Reap every descendant, then receipt |
| Supervisor error, panic or signal | non-zero | Reap every descendant, then receipt |

- **The supervisor decides, the owner applies.** Only `agent-turn` knows whether the
  provider ended the turn itself. It reports that with a reserved exit status, which
  the owner translates to 0 so the dispatcher's success/failure mapping is unchanged.
  The owner announces support through the supervisor's environment; without an owner
  (macOS, direct tests) the supervisor keeps exiting 0. Tools cannot forge the status:
  it is the supervisor's own exit, and the provider's environment is rebuilt from scratch.
- **Root owner, unprivileged turn.** The dispatcher starts the owner without the module
  identity and passes uid/gid in the owner's private argv; the owner applies them to
  the supervisor only. Kill selection is unchanged and narrow: the owner signals only
  its own direct children, which can only be the supervisor or orphans of this turn.
- **Stop wins.** The owner checks for a dispatcher stop request before releasing, so a
  cancel never turns into a release once it has reached the owner.
- **The drain only stops waiting.** After the provider exits, the bounded output drain
  no longer signals the provider's process group by itself. The turn is classified as
  soon as the provider exits, before the bounded harvest; the group is stopped then
  whenever the turn is not released, so a normal finish kills nothing and other turns
  do not keep provider-group work running through harvest. An interruption or
  deadline the supervisor sees later vetoes the release, and the execution owner reaps
  the rest; the supervisor does not signal the provider's group after harvest, when its
  id may have been reused.

## Decision notes

- Rejected: release on any supervisor exit 0. The supervisor exits 0 after outcomes it
  forces, such as the setup deadline, and those must still reap (existing
  `agent_execution_tree_is_reaped_before_settlement`, `setup_timeout`).
- Rejected: the dispatcher deciding from the outcome payload. It cannot tell when the
  supervisor has exited, so it would need a two-way owner protocol.
- Rejected by the owner: killing leftovers every turn (data unreachable until restart),
  an always-on PostgreSQL per VM (cost), and a chat notice about leftovers (confusing).
- Unchanged: the supervisor's group stops (in-turn and right after the provider
  exits) can signal the group id of a provider it has just reaped, a short id-reuse
  window shared with `terminate_tokio_child`; closing it means keeping the leader
  unreaped until the last group signal, a separate change.
- macOS has no execution owner, and a dispatcher cancel there kills only the
  supervisor's own group (unchanged), so a provider group a successful turn left
  behind survives a later cancel or interruption on macOS. Linux is the guarantee.
- Accepted risks: a forgotten leftover runs until the agent or workspace Stop ends it;
  a leftover writing into `/work` after a finished reply can race the workspace's next
  save or turn. PostgreSQL writes outside `/work`.
- Acceptance: a normal finish with a live descendant (including another uid) settles
  promptly and the descendant survives; stops, limits, forced outcomes and supervisor
  failure still reap every descendant, including another uid's; the supervisor and its
  descendants keep the module identity; processes outside the turn are never signalled.
