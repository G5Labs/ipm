# Abstraction leakiness: round 2

## Human request

On 2026-10-03, after the five round-1 PRs (G5Labs/G5-Lean#295, #296, #297, #300, #301) were merged on the user's instruction, the user asked:

> when all are merged, please re-evaluate the leakness again, the same way as the file `abstraction-leakiness-review-2026-10-01.md` I supplied at the very beginning and plan for the next round of refactor/fix

Earlier the same day, about explanations of the round-1 PRs:

> the explainer you created are too complicated with too many details, for each PR, please use 1-2 sentence to explain me the issue and 1-2 sentence to explain your fix in the PR, make your sentence human readable

Round 1's instructions still frame the work ([round 1 request](request.md)): critical leaks only, sequential PRs, simple rollout.

After reading the review and plan, the user answered the plan's three open points (approve the new rule that host control acts only within the host's organization; keep round 1's budget of five PRs under 5,000 changed lines each; use the stricter kebab-case rule for preview profile names):

> looks good to me

and on the five-PR plan:

> looks good to me, please go ahead to implement

with the same process as round 1: stacked PRs, each targeting the previous PR's branch, never merged by the agent; a Codex review before each PR is filed; bot comments and CI handled after filing.

On 2026-10-04, after the five PRs were filed (G5Labs/G5-Lean#309–#313), the user answered the follow-up the agent had flagged, which rechecks VM leases after row-lock waits in paths PR 4 did not touch:

> please make this as the 6th pr in this round

On 2026-10-04 the user answered the open question PR 2 left (whether Start continues held work) with a decision on the chat's Stop:

> Chat Stop ends the turn, keeping its partial output, like most chat apps.
> Your next message starts a new turn in the same conversation. The agent keeps its context because the provider session belongs to the chat, not the turn (agent_dispatch.rs:1072).
> No Continue is needed in the chat; the drawer's "Stop and hold" stays for anyone who wants a pause.
> It matches H023 ("cancel is required; pause is not needed", human-decisions.md:35).
> It also removes the worst case from the Ready question, because chat turns would no longer carry an owner_stop hold.
> It changes a documented design, so the ontology needs updating too.

After reviewing PR 7's design and its four choices (a job's chat keeps the hold, the next message ends a turn held for any reason but an input of unknown delivery, Claude's End ends the turn, the usage gap stays), the user said, the same day:

> implement PR 7

The PR's review then added one exception to the held-turn rule: a turn the platform started, such as a merge-conflict resolution, keeps today's hold on Stop, a new message and Claude's End, because its merge run follows that task. The user confirmed it the same day, before the merges:

> the platform-turn exception looks right, keep it

## Agent interpretation

- "The same way" means the same five abstractions, the same table counts and frontend scan, and the same 1–5 rating, with the corrections the round-1 audit asked for. Leaks are classified against the ontology's owners, ratings follow a written rubric, and claims are checked by hand. The result is a review dated 2026-10-03, kept outside the repository like round 1's audit.
- "Refactor/fix" includes the behavior bugs the re-evaluation found; they come first.
- The budget is round 1's: at most five PRs, each under 5,000 changed lines, critical and cheap first (confirmed by the user).
- Rollout stays as in round 1: one PR at a time, merge, QA redeploy and a short manual check.
- Plain, short writing is preferred over detail.
- Chat Stop ending the turn is the owner's decision; it becomes PR 7, designed first and implemented after the owner reviews the design. Context carries over through the rebuilt chat history, which already includes stopped replies: providers start fresh on every turn because native resume is off.
- A chat that belongs to a job keeps today's hold, because cancelling its turn would fail the job's dependent work (agent's choice, confirmed by "implement PR 7").
- "No Continue is needed in the chat" also covers turns held for any other reason (a workspace stop, idle stop, VM loss, transfer, a restart the host needed, or a question that went unanswered): the chat offers no Continue for them either, so the owner's next message ends such a turn and runs. An input whose delivery is unknown keeps the chat's Redeliver choice (agent's proposal, confirmed by "implement PR 7"). Narrowed in the PR's review: a turn the platform started, such as a merge-conflict resolution, keeps its hold on Stop, on a new message and on Claude's End alike, because its merge run follows that task; and a message sent while the turn was still stopping ends it once it is held.
- PR 6 is a fix within existing intent (`g5.security.admission`): the same race PR 4's review found, in its remaining paths. It follows the same process and budget as the other PRs.
