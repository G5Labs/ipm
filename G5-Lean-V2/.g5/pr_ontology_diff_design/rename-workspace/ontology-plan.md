# Rename a workspace without touching Git

Source: [chat intent](../../chat_intent/rename-workspace/request.md) (2026-10-22, decisions 2026-10-23,
review corrections 2026-10-23).

An owner can rename a non-Base workspace. The rename changes nothing in Git: the workspace's
branch keeps its name in every repository. Former names stay reserved by the workspace and keep
resolving to it, so old URLs, emails and CLI references continue to work and land on the current
name. The rename is a database-only transaction.

## Ontology changes

| Block | Change | Why |
|---|---|---|
| `g5.workspace.identity` (amended I, workspaces) | The name is unique among the organization's live workspaces regardless of case, addresses the workspace in URLs, and may be changed by the owner; the id is the durable identity. | Today the name is immutable because it is the branch. |
| `g5.workspace.branch` (amended I, workspaces) | The branch is derived from the name when the workspace is created and never changes afterwards; renaming leaves every repository branch as it is. | "rename a workspace must NOT touch git". |
| `g5.workspace.rename` (new D, workspaces) | The current owner may rename a non-Base workspace, including a read-only one or an ordinary reviewer-owned review copy, to a name that passes creation's validity and reserved-segment rules and that no live workspace holds or reserves. Formal agent-review workspaces are excluded with every other human change to them. Base stays fixed as a product rule. Only the name changes. | User request and decisions 2, 3, 4; review correction 6. |
| `g5.workspace.former-names` (new D, workspaces) | A workspace keeps every former name while it exists: references by a former name resolve to it and are replaced by the current name; no other workspace can take a former name; the workspace may take its own back; the names retire when deletion completes. | Decision 1, reserve rather than release, so a link never silently points at a different workspace. |
| `g5.ui.home-workspaces` (amended D, ui) | Row actions are Delete and Fork; Rename is not offered from Home. | First "add a rename button between the trashcan/delete button and Fork", then moved to Workspace details after PR #756 (2026-10-24). |
| `g5.workbench.tabs` (amended D, ui) | A tab follows its workspace through a rename, keeping its place and session memory; a browser arriving by a former name replaces the stale tab instead of adding one. | Tabs and their session memory are keyed by name. |

## Governing ontology

- [g5.workspace.mutation-authority](../../ontology/features/workspaces.g5.md#g5.workspace.mutation-authority) and
  [g5.workspace.audience](../../ontology/features/workspaces.g5.md#g5.workspace.audience): the current owner is the
  one who changes workspace metadata such as visibility; rename uses the same gate, which also refuses while an
  ownership transfer is pending.
- [g5.workspace.content-policy](../../ontology/features/workspaces.g5.md#g5.workspace.content-policy): read-only blocks
  application content writes; the name is not content (decision 3), so rename stays available like visibility.
- [g5.workspace.base](../../ontology/features/workspaces.g5.md#g5.workspace.base): Base is never renamed. This is a
  product rule: Base is the organization's fixed root and its name is a navigation anchor, not a technical
  consequence of branch immutability.
- [g5.agent-approval.human-read-only](../../ontology/features/agent_approvals.g5.md#g5.agent-approval.human-read-only):
  formal agent-review workspaces accept no human changes; rename is one more such change.
- [g5.workspace.review](../../ontology/features/workspaces.g5.md#g5.workspace.review): ordinary review copies are
  identified by their fork record, not their generated name, so renaming one changes nothing in review behavior
  (decision 4).
- [g5.workspace.delete](../../ontology/features/workspaces.g5.md#g5.workspace.delete): deletion tombstones the
  workspace, removes its branches, then its repository membership; names retire with the membership.
- [g5.workspace.project-docs-path](../../ontology/features/workspaces.g5.md#g5.workspace.project-docs-path): Project
  Docs are keyed by id. One file inside the folder, the PR body document, is named after the workspace.
- [g5.workspace.git-authority](../../ontology/features/workspaces.g5.md#g5.workspace.git-authority): repository branch
  metadata stays the authority on branch state; the branch column names which branch that is.

## High-level design

### Data model

- **Branch column.** `workspaces.branch_name`: null for Base (Base's branch in each repository is that
  repository's default branch), otherwise required, Git-valid and immutable after insert. Backfilled from
  `name` for existing non-Base rows. Every reader that treats the name as a branch reads this column instead.
  Found by sweep of SQL that selects the name from `workspaces` and of Rust that passes it to Git: the two
  `authz` definer functions from migration 039; the manifest and workspace reader; fork source capture and the
  branch-ownership check; remote-integration policy, repositories and merge runs; the git push-target lookup;
  VM start, source-rebase and incarnation; deletion; approval config; and the review service's
  `workspace_owner_and_branch`, whose callers use the second value both as a branch (review publication,
  suggestion threads, internal agent handoffs) and as a label. The producer returns the branch; label callers
  switch to the existing `workspace_name` helper. The sweep is repeated at implementation with the name made
  different from the branch in a fixture so any missed reader fails a test rather than silently keeping working.
- **Name.** Still a valid Git branch name and not a reserved segment (decision 2), now mutable for non-Base
  workspaces. A new partial unique index on `(org_id, lower(name))` over live workspaces makes the organization
  uniqueness a database fact; today only the fork service checks it, with the Git branch collision as a backstop
  that the rename removes. The migration refuses to apply while live case-insensitive duplicates exist, like
  migration 013 did for fork claims.
- **Former names.** `workspace_former_names (org_id, workspace_id, name, created_at)`, unique on
  `(org_id, lower(name))`, foreign key to the workspace, tenant RLS and grants like `remote_integrations`.
  A rename inserts the previous name. During deletion the current name stays reserved through the workspace's
  repository membership, which the name-holder check reads for tombstoned rows; completed deletion removes that
  membership, after the branches are gone, and then the workspace's former-name rows. That is the moment the
  branches themselves stop being owned today. The table holds one row per distinct name a workspace has had
  other than its current one; renaming back to a former name moves rows, it does not add.
- **Rename log.** `workspace_renames (org_id, workspace_id, previous_name, name, actor_user_id, renamed_at)`,
  append-only, one row per rename. The former-names set cannot show a rename-back or the order of renames; the
  log can, and is the durable source for an Activity entry. It is distinct from any transient notification.
- **Immutability trigger.** The existing trigger function keeps `is_base` immutable, makes `branch_name`
  immutable, and keeps Base's name immutable; non-Base names may change.

### Rename operation

`POST /api/v1/workspaces/:id/rename { name }` is a narrow endpoint whose transaction touches only the database.
The existing metadata PATCH builds branch summaries with Git-derived divergence, so rename does not share it.
In one transaction, under the owner gate the PATCH applies to description and visibility (current owner, refused
while a transfer is pending), through the same scoped access that hides formal agent-review workspaces from human
writes, the workspace service:

1. refuses Base; validates the name with the fork service's shared check (Git-valid, at most 255 bytes, not
   reserved) and returns the same validation errors as creation;
2. takes the organization's name lock, `pg_advisory_xact_lock` keyed by `g5:workspace-name:<org_id>`, and
   locks the workspace row;
3. refuses, as a conflict naming the holder's kind, a name held by another live workspace, a former name of
   another workspace that has not completed deletion, or an active fork claim (`workspace_forks.proposed_name`
   while pending or aborting); the comparisons are case-insensitive;
4. if the new name is one of this workspace's own former names, removes that row; records the previous name as
   a former name unless it equals the new name case-insensitively (a case-only change creates no alias);
   appends the rename log row;
5. updates `name` and `updated_at`;
6. returns `{ id, name, formerNames }` with former names newest first. Workspace detail also carries
   `formerNames`. A stale submission, one whose `name` equals the current name, is a no-op success.

The fork path takes the same lock around its availability check and claim insert, and its availability check
also refuses a reserved former name. This serializes rename against fork claims; the two partial unique indexes
already serialize rename against rename and claim against claim. The branch-ownership check is unchanged and
still protects Git.

### Branch readers that compare names

Two flows compare workspace names as an identity check between a prepare step and its commit: workspace PR
creation (prepare versus publish) and merge revalidation. Both compare the workspace id and the branch instead,
so a rename between the steps no longer aborts them. The provider PR lifecycle, update and recovery read the
source branch from the preflight stored at PR creation, which is the branch, so they are unaffected; the names
in that context are labels. The PR body document inside Project Docs is named by a slug of the workspace name;
the reader tries the current name's slug, then each former name's slug newest first, so a body written before
a rename is still found.

### Resolution and redirect

- `resolve_slugs` looks up a live workspace by name, then a former name joined to a live workspace, and returns
  the id together with the current name (`workspaceName`). `org` accepts the organization slug or its id so
  the CLI can resolve from the org id it holds. The 404 behavior for unknown names is unchanged.
- The SPA's slug resolution keeps the canonical name. When it differs from the route segment, `WorkspacePage`
  replaces the URL's workspace segment with the canonical name, preserving the sub-route, query and fragment,
  and the tab strip rewrites the typed name's tab to the canonical one. The page component stays mounted, so
  editor drafts, selected content and chat state, which are keyed by workspace id, are untouched.
- `g5d clone <url>` already resolves through this endpoint. The bare-name form of `g5d clone` matches an exact
  name against the first page of the organization's workspaces today; it switches to the resolver so a bare
  former name resolves too. The default clone directory stays the name as typed.
- Links built from the name at send time (owner emails, PR and issue text) keep working through the reservation.

### Browser state

Tabs are stored per user as organization slug plus name, and the session memory that returns a tab to its last
URL and snapshot is keyed by the tab's path. A rename, learned from the rename response, from the optional
event, or from arriving through a former name, rewrites the tab entry in place so its position is kept, re-keys
its memory and rewrites the workspace segment of the remembered URL. When both the old and the new name are
open, the earlier position wins and the other entry is dropped, keeping the memory of whichever was shown last.
Close behavior and the left-neighbour rule are unchanged because the list is edited in place. Snapshots already
carry the workspace id and remain valid.

### Freshness

The rename response updates the renaming browser. Other browsers learn the new name on arrival through a
former name, on the workspace detail and list refetch that focus and polling already perform, and, optionally,
from a `workspace.renamed { workspaceId, name, previousName }` event on the workspace channel. If the event is
built it is best-effort invalidation like the policy-changed event: forget the name resolutions, invalidate the
detail and list queries, rewrite the tab and replace the URL if the browser is on the previous name. No delivery
guarantee is added; a missed event costs one stale label until the next refetch or arrival.

### UI

- **Workspace details, Overview.** A Name row before Description shows the current name, its former names
  beneath when there are any, and a "Rename" control styled like Description's "Edit", offered to the current
  owner of a non-Base workspace (read-only workspaces and ordinary review copies included). It opens the rename
  dialog, prefilled with the current name, with the note that the Git branch keeps its current name and that
  links using the old name keep working; errors show the server's validation message. On success the
  resolutions cached for this workspace learn the new name in place, so the page showing it replaces the URL
  through its former-name handling, keeping `?view=details` and rewriting the tab without leaving the page.
- **Home rows** offer Delete and Fork only; Rename was first placed there and moved after PR #756 (owner,
  2026-10-24). The switcher rows and the details header offer no Rename.
- **Fork dialog** label changes from "exact branch name" to say the name is also the new workspace's branch
  name and can be changed later.

## Rollout and rollback

- Order within the release: the migration adds and backfills the branch column and the reservation tables;
  every branch reader, database function and writer uses the branch column; only then does the rename endpoint
  and button ship. All of this lands in one deployment, and the backfill makes name and branch equal, so a
  binary that still derives branches from names behaves identically until the first rename.
- After the first rename, name and branch diverge for that workspace. Rolling back to a binary that derives
  branches from names is unsafe: it would address a branch that does not exist for every renamed workspace.
  Rollback after that point needs the data repaired first (rename each diverged workspace back to its branch
  name, which the reservation permits since that is its own former name) or a forward fix. The design records
  this as the rollback boundary.
- The migration is verified under the actual application and definer roles, not only as the migration owner:
  the definer role already has table-level SELECT on `workspaces`, and the test asserts the two rewritten
  functions return the branch for a renamed workspace.
- The migration checksum manifest gains the new entry. The data crate's build script tracks the migration
  directory, so no forced clean rebuild is planned; one is used only on evidence of a stale embedded set.

## Settled questions

- **Former names: reserve or release.** Reserve while the workspace exists; retire after completed deletion
  (user, 2026-10-23). Release on reuse was offered and rejected.
- **Name rules.** Unchanged Git-branch rules (user, 2026-10-23); loosening the character set was declined.
- **Read-only workspaces** can be renamed (user, 2026-10-23). **Ordinary review copies** can be renamed (user,
  2026-10-23); formal agent-review workspaces keep their prohibition on human changes.

## Decision notes

- **Branch column over display column.** Adding a separate display name would change what `name` means for
  every client and URL; adding the branch column changes only Git-facing readers, and the API, CLI and client
  keep reading `name` as today.
- **Former-names table over a single name registry.** A registry holding current and former names in one
  unique index would still need the application check against pending fork claims and the organization lock,
  and would add a second home for the current name. The former-names table plus the partial unique index on
  live names gives database-enforced uniqueness within each table; the organization lock covers the cross-table
  and claim cases.
- **Narrow endpoint over PATCH.** The property that matters is a database-only transaction. Keeping rename
  out of the PATCH avoids its Git-reading response and keeps the existing metadata path untouched.
- **The old branch stays taken.** After renaming foo to bar, a fork explicitly named foo is refused on the name
  reservation, and would be refused on branch ownership in the shared repositories even if the reservation were
  released. The rename dialog states that the branch keeps its name.
- **Case.** The resolver already matches names case-insensitively while Git is case-sensitive, so the new
  uniqueness and reservation are case-insensitive and the migration guards existing collisions.
- **Unlimited renames.** No count limit or cooldown; every new name is consumed for the organization until the
  workspace's deletion completes, which decision 1 accepted.
- **Capacity.** One former-name row per distinct name, one log row per rename, one index probe per resolution
  miss; no limits are introduced.

## Acceptance

Backend API tests, with fixtures whose name differs from the branch so equality cannot mask a missed reader:
owner renames and the response, detail and list show the new name and former names; a member, an organization
admin who is not the owner, Base, a formal agent-review workspace and a workspace with a pending transfer are
refused; invalid and reserved names return creation's validation errors; conflicts with another live name
(differing only in case), another workspace's former name, a deleting workspace's names before its deletion
completes, and an active fork claim are refused; A to B to C to A succeeds with every intermediate name
resolving to the workspace and the log showing the sequence; a case-only rename creates no alias; a stale
submission and a retry are no-ops; resolve by a former name returns the id and current name, by slug and by org
id; after completed deletion the former names neither resolve nor block a new workspace; a fork named after
another workspace's former name is refused; the manifest, branch list and git push authorization are unchanged
after a rename; a running dirty VM flushes, stops and starts after a rename and restores the same branch;
parent and child relationships, repository attachment and the review source branch survive renaming either
side, including a review copy and its source; an existing workspace PR is renamed on either side, then saved,
updated, merged and recovered; the PR body document written before a rename is still used; deletion cleanup
retires the names; concurrent rename against fork claim, transfer and deletion each end with one winner and no
duplicate name.

Frontend tests: Home row order and owner-only Rename; the dialog's success, validation and conflict paths; URL
replacement preserving a doc sub-route, query and fragment with the page left mounted; tab rewrite in place with
order kept, dedupe of old and new, memory re-key and remembered URL rewrite, on rename, on arrival by a former
name and, if built, on the event. The alignment-shell browser flow gains a rename followed by opening the old URL.

Migration: applies on a database with existing workspaces under the application and definer roles, refuses on
live case-insensitive duplicates, and leaves every existing branch resolvable through the branch column; the
checksum manifest gains the new entry.

## Implementation notes (2026-10-23)

- Migration 108 runs its backfill under the definer role with the migration 102 membership snapshot
  and takes schema CREATE only to replace the two lookups; the unique index is the duplicate guard.
  `migration_108_workspace_rename.rs` applies it as a non-superuser migrator that owns the schemas.
- Two existing tests encoded the old rules and were updated: `workspace_repository_model` expected a
  name update to fail (it now succeeds and ref selection still follows the branch), and the PR-target
  test created a name tie by renaming a Base workspace, which neither Base immutability nor
  case-insensitive uniqueness allows any more; the fallback order is asserted without a tie.
- Not executed here: the concurrent rename-versus-fork, transfer and deletion races (serialized by the
  organization name lock and the owner gate, each covered sequentially), the provider PR lifecycle
  against a live GitHub repository, and a browser rename round trip; the CI browser flow asserts only
  that the owner's row offers Rename. The CLI's `g5d clone` bare-name path is covered by the resolver
  tests, not by a CLI test.
- The optional `workspace.renamed` event was not built; freshness relies on the rename response, the
  redirect on arrival and the existing focus and poll refetches.

## Follow-up (2026-10-24): Rename moves to Workspace details

After PR #756 merged, the owner asked to remove Rename from Home rows and offer it in the Overview card of the
workspace details view (`?view=details`) as a Name row above Description, styled like Description's Edit. The
rename endpoint, dialog, resolver fallback and tab follow-through are unchanged. The rename mutation now updates
cached name resolutions for the workspace in place rather than dropping them, so a page showing the workspace
under its old name replaces its URL through the existing former-name effect without a pending flash.

