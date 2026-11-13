# Organization administrators act on Base pull requests

## User request

> G5's "Show all PRs" interface only shows buttons to the workspace owner.  For Base workspace, should also show them to organization admins

(2026-10-08, in the session for PR #382.)

## Agent interpretation

The buttons are the per-row pull-request actions in the Show All PRs list: Refresh, Close and Merge. The list itself already opens for every reader. Showing those actions is useful only if they work, so organization administrators of Base's organization gain the server authority to refresh, close and merge open pull requests whose target is Base, alongside Base's owner. Organization owners count as administrators, as everywhere else in G5 (`org:admin`).

Pull requests targeting any other workspace stay their owner's. The change grants no Base content writes, live compute, remote-binding controls or source-side publication; merge admission (Integration Engineering requirements) is unchanged and the acting administrator's provider authority and attribution apply.

## Follow-up

> put it into the current branch - I need to untangle a G5 PR and need this.  yes, would need backend changes - this isn't just about button visibility.

(2026-10-08.) The user confirmed the server authority and asked to implement on the PR #382 branch. Scope stays the Show All PRs row actions.

## Pull and auto-pull

> also enable Base's Pull and auto-pull to be done by org admins

(2026-10-08.) Interpretation: an organization administrator may trigger Base's Pull ("Fetch from Remote") and toggle each Base repository's auto-pull. The import itself keeps acting under Base's current owner, as Base parity requires; Push stays absent on Base, and Accept provider history, Retry sync and binding changes stay the owner's.

## Make Remote and Accept provider history

> As an admin, I need to be able to "Make Remote" on the Base workspace and to adopt revision history.

(2026-10-09.) Interpretation: an organization administrator of Base's organization may Make Remote on a Base repository and Accept provider history into Base, as Base's owner may. "Adopt revision history" is the existing Accept provider history action, which resets a bound Base repository to the reviewed provider tip. Both are explicit decisions by the acting administrator, so they run under that administrator's own provider authority and are attributed to them. Bound repositories then keep acting under Base's owner for their sync cycles. The page also has to offer Make Remote on Base at all: its gate currently requires a writable source, which Base never is. Retry sync and changes to an existing binding stay the owner's.

## Make Remote button and adopting after binding

> Our new repository actions methodology avoids using chips for actions - let's adopt that for make remote too. Make Remote should be a button that says "Make Remote" similar to other branch actions. When Make remote completes it usually then offers the "use an agent to reconcile conflicts" - it should offer the alternative of adopt revision history (especially if the local repo is just the default init commit)

(2026-10-09.) Interpretation:
- Make Remote moves off the repository chip and becomes a labelled "Make Remote" button on the repository card, styled like Pull and Push. Publish, the other chip action, was left for the user to decide (answered [below](#publish-absent-provider-branch-and-credentials)).
- On any workspace, a Make Remote that binds paused offers "Adopt provider history" beside the agent resolution. Adopting is recommended when the local history is only the initial commit.
- Outside Base, the reset runs in the owner's VM; on Base, it runs as the existing server-side acceptance.

## Publish, absent provider branch and credentials

> 1. Publish becomes a button just like Make Remote.
> 2. Absent provider branch - ask the user to confirm creation if it doesn't exist
> 3. Use admin's credentials if the admin is doing it.

(2026-10-09, answering the plan's open decisions.) Interpretation:
- Publish moves off the repository chip to a labelled "Publish" card button, like Make Remote.
- Make Remote on Base, when the provider branch doesn't exist, asks the actor to confirm creating it, then creates it from Base's history. This is the one exception to Base never pushing to its provider. The question was asked about Base, so other workspaces keep creating an absent branch without asking.
- When an administrator makes Base remote or adopts its provider history, the provider operations use the administrator's own credentials and are attributed to them.
