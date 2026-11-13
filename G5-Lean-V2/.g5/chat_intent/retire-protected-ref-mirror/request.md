# Retire the protected-ref mirror

## Human instructions

The user saw QA's Base workspace (`https://qa.g5labs.ai/g5/Base?view=details`) show the remote-tracking "ahead" chip and said: "I didn't think this was supposed to happen."

Investigation found Base's `main` equal to GitHub `main`, while its synchronization watermark stayed at the QA cutover commit. The protected-ref mirror's five-minute poll had written `refs/heads/main` directly. That ref is also Base's working branch, so the write bypassed remote tracking. The agent then explained that, for the only ref it touches in practice, the mirror duplicates Base remote tracking and conflicts with it: it ignores the auto-pull toggle, leaves the watermark unchanged and overwrites non-fast-forward history.

The user then said: "yes, start the planning workflow to retire it. Ensure that Base is treated just like any other workspace. Except that auto-pull should be on by default for Base."

## Agent interpretation

- Remove the protected-ref mirror as a writer of workspace branch refs. A provider tip reaches any workspace branch, including Base, only through remote tracking.
- Base gets the same remote-tracking states, actions, watermark, advisory observation and auto-pull/auto-push policy as other remote-tracked workspaces. Divergence and pause handling also match. Base-only special cases in these paths are removed unless another governing rule requires them.
- The single requested difference is the default: auto-pull starts enabled for Base's remote-tracked repositories, and the owner can still turn it off.
- Out of scope: Base's other existing distinctions, such as its fixed branch name, deletion protection and role as clone parent. Neither the user nor the investigation raised them.

## Clarification

Asked how far "treat Base just like any other workspace" should go, the user chose **remote tracking only**. Base uses the same remote-tracking engine, states, watermark, advisory, per-repository pull/auto-pull and owner controls as other workspaces. Base stays content-read-only, with no VM or working branch, and its read-only presentation rules are unchanged.

## Migration decision

On the plan's Q1 (existing Base auto-pull settings), the user said: "During the migration, switch all Base on." The migration therefore enables auto-pull for every existing Base remote-tracking binding, whatever its previous setting.

## Base provider-history acceptance

On the open question about provider rewrites on Base, the user said: "I like the owner approval that resets base when base has a history rewrite. I also need a way to "Make Remote" on Base successfully where the goal is to adopt the remote repo and discard the local. I think that this button would do that, too. Currently, the make remote would result in a divergence, which because Base cannot have a VM, can never be resolved."

### Agent interpretation

- One owner action, **Accept provider history**, resets a Base repository to the provider tip. It serves both a provider rewrite and a Make Remote whose provider history differs from Base's.
- Make Remote on a Base that is ahead of or diverged from the provider no longer refuses. It binds the repository paused, and the same action adopts the provider history, discarding Base's local commits.

## Base push invariant and co-landed imports

After reviewing this branch and the uncommitted acceptance work, the user asked for three follow-ups in the existing plan:

1. "Base never pushes (safety, do first)." Base must never yield a provider push for any trigger or push mode. A Base that is ahead pauses like a divergence and reconciles through Accept provider history. `push_to_remote` on Base is refused again. Base's push mode must not offer auto-push in the API or UI; the agent decides between forcing manual and ignoring the mode, and records the decision. The base-parity block is amended to pull-side parity only, with nothing pushed on any path, and the "only difference" wording in the auto-pull default is corrected.
2. "Land due imports together." When a cycle imports, every other member of the same workspace that is bound, unpaused, eligible under its own policy and a strict fast-forward of a freshly fetched provider tip lands in the same cut. Per-repository isolation stays: a paused, failing, non-fast-forward or busy member is left out and never blocks the primary. Background auto-pull must not half-import a multi-repository G5 workspace PR whose provider PRs are not all merged, or the gap is recorded. This applies to every workspace. The user asked to confirm that H014 does not forbid it: "If you think it does, stop and ask me before coding."
3. Investigate a possible deadlock in the acceptance landing's lock order and fix it if needed; repair the `ProviderHistoryPreview` doc comment; confirm whether `remote_sync_runs` rows awaiting review need closing.

### Agent interpretation

- Base's push mode is forced to manual (migration, creation, and a refusal to set auto). The guarantee does not depend on the mode: the classifier never yields a push for Base, and every push executor and enqueuer refuses Base.
- H014 decides individual repository pushes and no atomic multi-repository cut. Co-landing is opportunistic and makes no atomicity promise, so H014 allows it; the earlier citation of H014 for per-repository imports over-read it.

## Consistent fork capture of a multi-repository workspace

The user asked that a fork's capture of a multi-repository workspace be consistent: "The fork must capture either both old heads or both new heads, never a mix." The required outcome: "A fork's frozen source vector is always a vector some landing actually committed, or the untouched prior vector. This applies to every source workspace, not only Base." The user named two holes: fork admission reads source heads without the commit fence, and a landing restores its refs after a refused database commit, after the fence is released. The user offered three ways to exclude rolled-back heads and asked the agent to choose and justify one:
- (a) a head cache;
- (b) a lock held across commit and restore;
- (c) something better.

The user also asked for an inventory of other multi-repository cut readers, and for two decisions to be brought back rather than implemented:
- whether forking should warn about held, paused or behind members;
- whether an explicit Pull should respect the merged-PR hold, and whether closing a partly merged PR lifts it.

### Agent interpretation

- Option (a) is unavailable: migration 039 dropped the head cache, and `g5.data.git-authority` forbids a second SQL head registry.
- Option (b) needs a second pooled connection per landing, or a ScopedDb that outlives its transaction.
- The agent chose (c): Git acceptance under the fence is the landing's commit point.
  - Every database check that can refuse a landing runs before its refs move.
  - A Git failure partway through a cut is restored under the fence.
  - Refs are never retracted after the fence is released, which matches what cancellation and an unknown COMMIT outcome already do.
- Fenced readers therefore see only completed cuts.

## Fork advisories and the explicit-Pull hold

On the two decisions the plan brought back, the user said: "accept your suggestions for decisions 1 and 2."

### Agent interpretation

- Forking shows a non-blocking advisory, before and in the fork response, that names each remote-tracked source member that is held by the merged-PR hold, paused (with its reason) or behind its provider observation. Forking is never refused for these.
- An explicit Pull keeps bypassing the merged-PR hold. Its response and the tracking popover name the half-merged workspace PR and say the pull imports that repository alone.
- Closing a partly merged workspace PR lifts the hold, as it already does; the ontology now states it.
