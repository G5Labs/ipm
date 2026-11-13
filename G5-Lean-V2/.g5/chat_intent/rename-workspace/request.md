# Rename a workspace without touching Git

## Human request

On 2026-10-22 the user asked for an analysis before any change:

> I want to add a feature for user to be able to rename their workspace name.
> - UI side, we can add a rename button between the trashcan/delete button and Fork
> - today, we enforce the workspace name matches the git branch name, we need to relax this because rename a workspace must NOT touch git (i.e, rename git branch name)
> - workspace name are widely used in the url, we should allow auto-redirect, i.e, after rename, user must still be able to use the old name in the url to access
>
> please analyze the feasibility and blast radius, and also let me know if i forget any thing

The analysis found that the workspace name is not merely required to match the branch: since migration 039 it *is* the branch (no separate column, every Git-facing query derives the branch from the name, and a trigger makes the name immutable). The URL resolves through one endpoint (`GET /api/v1/resolve`), which is also what `g5d clone <url>` uses. The GitHub branch is stored separately on the remote integration, and Project Docs are keyed by workspace id.

### Decisions the user made (2026-10-23)

Asked to choose among itemized options, the user answered:

> 1. Option B, reserve: foo redirects to bar for as long as bar exists, and nobody else can use foo in that org. Every rename permanently consumes a name.
> 2. Option A, keep the git rules: names look exactly as they do today, and the only new freedom is changing them.
> 3. Option A, allow: the owner can rename a frozen workspace, treating the name as housekeeping like visibility.
> 4. Option A, allow: review copies get the same Rename button as any other workspace.

## Agent interpretation

- **The branch gets its own column.** Each non-Base workspace records the branch it owns; it is set from the name at creation (with the existing uniqueifier) and never changes. The name stays a valid Git branch name (decision 2) so creation keeps deriving the branch from it.
- **Former names are reserved by the workspace** (decision 1): they resolve to it, the client replaces the URL with the current name, and no other workspace in the organization can take them while it exists. A workspace may take one of its own former names back. Deleting the workspace releases its names.
- **Rename is owner housekeeping**: the same gate as a visibility change. Base is never renamed. Read-only workspaces and review copies are renamable (decisions 3 and 4).
- **Rename on Home rows**, between Delete and Fork, as requested; the switcher rows and details header are not changed.
- **"Must not touch git"** also means the old branch stays owned by the renamed workspace, so nobody can later create a workspace whose branch would be the old name in the same repositories. Under decision 1 the name is reserved anyway, so the two rules agree.

### Review corrections the user forwarded (2026-10-23)

The user forwarded a Codex review of the design, asked for the agent's own evaluation, and restated the decisions:

> - Reserve historical names while the workspace exists; retire aliases after completed deletion.
> - Keep the existing name syntax for this feature.
> - Allow the current owner to rename read-only workspaces.
> - Allow ordinary reviewer-owned review copies to be renamed.

The review's points, each checked against the code: PR publication and merge must be tested through a rename and
compare branches rather than names (confirmed: two prepare-versus-commit checks compare names, and the PR body
document is found by a slug of the name); the review service's `workspace_owner_and_branch` returns the name as
the branch (confirmed, missed by the first sweep); bare-name `g5d clone` matches current names only (confirmed);
the metadata PATCH reads Git for its response, so rename needs a database-only path (confirmed); tab session
memory is keyed by the tab path (confirmed); formal agent-review workspaces must keep their prohibition on human
changes (confirmed; the scoped access already hides them, now stated); regression coverage must use fixtures
where name and branch differ (accepted); rollout order and the rollback boundary must be stated (accepted). The
proportionality notes were accepted: the renamed event is optional best-effort freshness, Home is the only
placement, no forced clean rebuild without evidence, grants verified under real roles, and a durable rename log
rather than the former-names set is the audit record.

### Agent interpretation of "retire aliases after completed deletion"

Deletion tombstones the workspace, deletes its branches, then removes its repository membership. "Completed"
is that last step, which is also when the branches stop being owned today. Until then the deleting workspace
reserves its current and former names like a live one.

### Follow-up after PR #756 merged (2026-10-24)

> The pr has been merged, but I want to move the rename button to the workspace detail page (i.e, `?view=details`)
> 1/ please complete remove the rename button in home page between the delete button and Fork button
> 2/ I attached a screenshot of a mock, basically add one row before the Description in the Overview section and add the rename button there, similar as the Edit button for the Description row

The mock shows the Overview card with a NAME row (label, the current name, and a pencil "Rename" control at the right) above DESCRIPTION, followed by Owner, Created, Last updated and Forked from.

#### Agent interpretation

- Home rows go back to Delete and Fork; no rename affordance remains on Home.
- The Workspace details Overview gains a Name row before Description with a "Rename" control styled like Description's "Edit", offered to the current owner of a non-Base workspace (read-only workspaces and ordinary review copies included, as decided). It opens the existing rename dialog. Former names move from the header into that row.
- Renaming from the details view of the workspace being shown: the page's own former-name handling replaces the URL (keeping `?view=details`) and rewrites the tab, so the page stays mounted.

