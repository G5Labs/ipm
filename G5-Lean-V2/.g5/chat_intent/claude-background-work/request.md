# Claude background work survives the turn

## User words

Context: a QA chat on `test-env` started `docker-ci`, Claude moved it to the
background on its foreground-command timeout, and the next turn could not find it.
After learning that per-turn exit and descendant cleanup end such work, the user wrote:

> the expected design/behavior was that background task should survive, it is up to agent to kill or not

On starting long jobs detached as a workaround:

> this in the next turn, how can agent get the execution results

> for both codex and claude, can we rely on claude and codex to device when to exit the cli? I expect that they won't exit if there are background task or sub-agent running

After probe results showed that only Claude's background sub-agents keep the CLI alive:

> ok, feel free to try on linux, and then please land a plan to build it for claude, also upgrade claude binary
>
> please create a clean (do not carry any changes) git worktree under folder `~/Workspace` to continue the work

## Agent interpretation

Build this for Claude only. A Claude attempt should stay live while work Claude
itself owns, background tasks or background sub-agents, is still running, so the
agent decides whether that work continues or stops and Claude can report its
results. Upgrade the pinned Claude Code CLI as part of the same plan. Codex keeps
its current turn-oriented transport because `codex exec` ends its own background
work when its turn completes.

## Decisions

The agent asked three questions: whether new owner messages should go straight into
a waiting Claude process (recommended: no, they wait until the background work ends);
whether a cleanly ended wait should count as success with a notice (recommended) or as
stopped; and whether the one-hour default limit is enough. The user answered:

> i agree with your recommendation for 1 and 2, as for 3, docker-ci may run hours. what is the pros and cons to have a timeout?

The agent then laid out the trade-offs of a time limit and offered a separate wait
limit (recommended), no wait limit, or one longer attempt limit. The user chose
"No wait limit" and, for the default length, answered "no limit".

The agent kept the existing one-hour limit per Claude turn. The user asked:

> why do we need this?

The agent explained that the limit backstops a hung turn and a runaway loop, and
offered a no-progress hang guard (recommended), a hang guard plus a cost cap, a turn
time limit, or no automatic guard. The user chose "No automatic guard".

The user added:

> as long as we have a reliable way for user to end the turn, i believe there is no need to have this timeout as worst case is at most 24 hours

The user then extended both rules to Codex:

> regarding (no timeout) limit and stop, please also make it the same for codex

On delivery, the user wrote:

> I also want to ship in one PR, but you proposed 3 PRs, please summarize pros of cons to split into 3 PRs

When asking to ship, the user added:

> just one more note, when upgrade codex binary, you may also need to update the list of LLM models, in case some are no longer supported

Asked whether the same PR should also upgrade the Codex CLI (0.153.3 to 0.159.3, the
npm latest) and re-check both providers' model lists, the user chose "Upgrade Codex too".
