# Issues

<a id="g5.issue"></a>
[I · g5.issue]

An issue is a unit of work, tracked outside any one workspace, that workspaces can link to and act on. G5 serves issues through a provider-neutral shape, so every surface and `g5d` behaves the same way whatever tracker holds the issue.

<a id="g5.issue.providers"></a>
[I · g5.issue.providers]

G5 supports GitHub Issues and its own [native tracker](#g5.issue.native); Jira Cloud support is planned.

<a id="g5.issue.domains"></a>
[I · g5.issue.domains]

Organization administrators configure which GitHub repositories, and whether the organization's native tracker, are available as issue domains.

<a id="g5.issue.domain-policy"></a>
[I · g5.issue.domain-policy]

Domains control content trust; external domains separately control whether G5 may publish comments.

<a id="g5.issue.search"></a>
[I · g5.issue.search]

Users can search configured domains by text, issue state and assignment.  

<a id="g5.issue.references"></a>
[I · g5.issue.references]

Issues can be resolved from references and linked to workspaces as primary or related.  

<a id="g5.issue.primary"></a>
[I · g5.issue.primary]

Each workspace has at most one primary issue; each issue has at most one primary workspace within its organization and tracker.

A review copy never holds a primary link: linking an issue to a review copy as primary is refused, for its owner and its agent alike, while related links stay available, because a temporary copy holding that claim would block the workspace doing the real work.

<a id="g5.issue.base-links"></a>
[D · g5.issue.base-links]

Base takes no issue links. It is [read-only and not a unit of work](workspaces.g5.md#g5.workspace.base), so every link creation refuses it, as primary or related, and the UI offers link creation on Base (Import task, quick-add, the classic link form) disabled with the reason while Detach and Unlink stay available for existing links; the Home issues tile does not list Base as a link target. Work on an issue starts in a fork.

<a id="g5.issue.identity"></a>
[I · g5.issue.identity]

Links store stable issue identity and are checked against the issue returned by its tracker.

<a id="g5.issue.link-authority"></a>
[I · g5.issue.link-authority]

Linking an issue does not grant access to its repository.  

<a id="g5.issue.live-read"></a>
[I · g5.issue.live-read]

Issue details and discussion are fetched live from their tracker under current domain authorization; native issues are read from G5's own store.

<a id="g5.issue.activity"></a>
[I · g5.issue.activity]

Workspace Activity combines the primary issue’s discussion with G5 workspace activity.

When the primary issue's history is longer than G5 reads from its tracker at once, Activity shows the newest entries and says that earlier history is on the tracker.

<a id="g5.issue.read-failure"></a>
[I · g5.issue.read-failure]

A failed tracker read leaves workspace activity available and reports the issue error separately.

<a id="g5.issue.comment-identity"></a>
[I · g5.issue.comment-identity]

Authorized collaborators post comments under their own identity: their connected GitHub identity on GitHub issues, their G5 account on native issues.

<a id="g5.issue.comment-entry"></a>
[D · g5.issue.comment-entry]

Comments on the primary issue use the existing issue composer, labelled with its destination, and the same composer serves any issue's [G5 page](#g5.issue.native.page). An issue reply is a comment in the original issue, never a workspace thread or an invented tracker hierarchy; general workspace feedback has its [own entry](reviews.g5.md#g5.governance.review.comments).

- General GitHub comment creation stays in Issues; Reply on an Activity comment opens the composer with a quoted reference, and the reference alone cannot be posted.
- Reuse [draft identity](#g5.issue.comment-draft-identity), [publication authority](#g5.issue.comment-authority) and [recovery](#g5.issue.comment-recovery), preserving saved text and retry identity while the quoted text is unchanged.

<a id="g5.issue.comment-authority"></a>
[I · g5.issue.comment-authority]

Publishing a comment to an external tracker requires domain permission and refuses public or unknown-audience destinations.

<a id="g5.issue.comment-recovery"></a>
[I · g5.issue.comment-recovery]

An uncertain external comment write is reconciled by finding its hidden marker on the issue, never by blindly posting again. The marker is derived from the organization, author, origin and idempotency key, as [issue creation](#g5.external-issues.creation.recovery) derives its own, so G5 keeps no record of issue comments; an unclear answer is reported as unknown, and posting again with the same key looks first.

- A repeat with the same key returns the earlier comment when G5 finds it among the issue's comments of the last 14 days, before the destination is re-checked.
  - The audience, visibility and connected-account checks run only when none is found, and the same key with other text is refused.
- Limited lookup and delayed provider visibility can still allow a rare duplicate.
- Editing a comment replaces its text, so repeating an edit is harmless and records nothing.

<a id="g5.issue.comment-draft-identity"></a>
[D · g5.issue.comment-draft-identity]

Keep browser drafts and submission identities bound to the original person, workspace and issue destination, or, on an issue's page, to the person and that issue. Moving the composer or changing the primary link cannot retarget an old draft or create a fresh submission identity for a retry; a draft whose comment GitHub did not confirm keeps its key.

<a id="g5.issue.platform-bug"></a>
[I · g5.issue.platform-bug]

Any user can report a platform bug with a “Bug” button, available in all practical UI states. Create these clearly labeled reports under separate deployment authority, with an optional screenshot assembled asynchronously from the page content.

## Publication and partial results

<a id="g5.issue.fork-primary-race"></a>
[D · g5.issue.fork-primary-race]

If another workspace claims an issue as primary while a fork is being created, finish the fork with a related-issue link. Keep the winner’s primary link exclusive.

<a id="g5.issue.search-completeness"></a>
[D · g5.issue.search-completeness]

When searching several domains, return successful matches alongside domain errors and truncation notices. Never present partial results as complete.

<a id="g5.issue.search.one-path"></a>
[D · g5.issue.search.one-path]

Every surface that finds an issue to open or link uses the organization’s issue search: the Home Issues tile, Import task and the workspace link form. A query that is a GitHub issue URL or `owner/name#n` finds that issue in the domain for its repository, and native search matches `KEY-n`, `#n` and its issue URL, so pasting a reference works in any of them. The link form links the chosen result through the link route, which keeps resolving references for `g5d issue link` and fork linking.

<a id="g5.issue.comment-edit-observation"></a>
[D · g5.issue.comment-edit-observation]

Only the connected provider account that wrote a comment may edit it. Before accepting an edit, require the destination and update time to match those observed, unless the requested text is already present; preserve G5’s reference marker. G5 cannot prevent a later edit at the provider.

## Issue creation

<a id="g5.external-issues"></a>
[D · g5.external-issues]

External issues live in a connected tracker such as GitHub, so G5 writes to them through the provider's own protocol under the domain's permissions and keeps no copy of their content.

<a id="g5.external-issues.creation"></a>
[D · g5.external-issues.creation]

An organization member with write permission may create a GitHub issue in the organization’s default creation repository using their connected account, and a workspace agent uses its owner’s account, shown as author without inventing an agent identity. An issue is created directly with a title, Markdown description, optional assignees, optional [labels](#g5.issue.labels.contract) and an optional parent, unassigned by default.

- Every entry point follows the same [permission checks](#g5.external-issues.creation.admission), [retry rules](#g5.external-issues.creation.recovery) and [parent-link rules](#g5.external-issues.creation.parent).
- Templates and Jira creation are outside this feature.
- When the default creation domain is the [native tracker](#g5.issue.native), the same entry points and fields create a native issue under the [native write rules](#g5.issue.native.writes) instead, and those three GitHub rules do not apply.

<a id="g5.external-issues.creation.admission"></a>
[D · g5.external-issues.creation.admission]

Creation uses the [comment-publication permissions](#g5.issue.comment-authority): external issues and G5 comments enabled, the destination currently private or internal, and the acting account connected. The content is screened for secrets, and these checks cannot prevent permissions, visibility or feature settings changing before the provider writes.

- For an untrusted destination, confirm the exact rendered payload, including marker, assignees and destination; a changed default target needs fresh review.
- Status changes derived from G5's operations stay deferred; the [hand transition](#g5.external-issues.transition), by a person or their agent, is the only status write.

<a id="g5.external-issues.creation.recovery"></a>
[D · g5.external-issues.creation.recovery]

Issue creation stays stateless in G5: a hidden description marker is derived from the submission key, organization and author, and before sending or answering a status check G5 searches that author’s recent issues in the target repository for the marker. After a network error, timeout or server error, G5 reports an unknown outcome and reuses the same key to check before resending.

- Limited lookup and delayed provider visibility can still allow a rare duplicate; exactly-once creation is not promised.
- The platform-bug delivery ledger is not reused; issue comments follow the [same approach](#g5.issue.comment-recovery).

<a id="g5.external-issues.creation.parent"></a>
[D · g5.external-issues.creation.parent]

An optional parent is validated before the issue is created: it must be an existing issue in the same repository, not a pull request. The child is created first, then attached; attachment failure keeps the child and reports partial success.

- A retry finds the same child and attaches it only if it has no parent, never moving it from another parent; a status check only reads.
- This link is separate from workspace issue links and needs no parent ledger.

<a id="g5.external-issues.transition"></a>
[D · g5.external-issues.transition]

A person closes (as completed) or reopens an issue linked to a workspace, from that workspace, or any issue from its [G5 page](#g5.issue.native.page), under their own connected GitHub account; the workspace's agent makes the same transition through `g5d issue close|reopen` on any issue `view` can read, under its owner's account. This is the tracker's Manual policy: a person changes the state, and G5 derives no transition from its own operations until an administrator maps one.

- It uses the [comment-publication permissions](#g5.issue.comment-authority), and a review copy is refused for person and agent alike.
  - These require external issues on, the domain's G5 comments switch, a private or internal destination, and the connected account.
- The request carries no text, so nothing is screened; the write is idempotent, so G5 records nothing and no key or marker applies.
- GitHub's refusal is reported as given, and no answer is an unknown outcome that re-reads the issue.
- No write follows a provider redirect, so a moved repository or transferred issue is refused; only the admitted destination may receive it.
- A native issue changes state under the [native write rules](#g5.issue.native.writes).

<a id="g5.external-issues.edit"></a>
[D · g5.external-issues.edit]

The workspace's agent edits an issue's title, description and assignees as its owner through `g5d issue edit`, on any issue `view` can read, and a person edits the same fields from the issue's [G5 page](#g5.issue.native.page); a review copy is refused. Listeners, watchers and milestones are out of scope; labels change by [their own deltas](#g5.issue.labels.contract); comments are edited only by their author.

- On GitHub the edit uses the [comment-publication permissions](#g5.issue.comment-authority) and the owner's connected account, screens title and description for secrets, keeps the [creation marker](#g5.external-issues.creation.recovery) the description carries, and follows no redirect.
- A native issue is edited under the [native write rules](#g5.issue.native.writes).

<a id="g5.external-issues.edit.assignees"></a>
[D · g5.external-issues.edit.assignees]

Assignees are added and removed against the issue's current set: GitHub logins on GitHub, member emails on G5 issues.

- An edit is refused before any write when it names a person G5 can tell is not assignable, names a person both added and removed, or names no change.
- Adding someone already assigned, or removing someone not assigned, changes nothing.

<a id="g5.external-issues.edit.outcome"></a>
[D · g5.external-issues.edit.outcome]

Each edit is one whole write that applies or is refused as a unit, and G5 records nothing, since setting the same values again is harmless. An unapplied change is never reported as applied: an assignee change GitHub's answer does not show is reported as not applied, and no answer is an unknown outcome to rerun.

- The agent's GitHub edit is last-writer-wins, like `gh issue edit`.
- A person's GitHub edit is refused when GitHub's current value differs from what their form showed.
  - Fields are compared, not GitHub's update time; this narrows but cannot close the window, since GitHub's write is unconditional.
- A native edit applies to the version read in its own transaction (for a person, the version their editor saw), and a change committed in between is refused for a rerun.

<a id="g5.external-issues.cli"></a>
[D · g5.external-issues.cli]

`g5d issue` mirrors `gh issue` within what G5 supports, naming the issue the same way; a gh flag, value or query qualifier G5 cannot honour is refused with what to use instead, never ignored.

- `create` uses the [creation rules](#g5.external-issues.creation) with gh-style title, body/body-file, assignee (`@me`) and parent flags, and returns the issue URL or JSON without linking it.
- `link` uses the existing permissions and defaults to related, primary on explicit request; `unlink` ends one of the workspace's links, primary included, matching only that workspace's links and reading no tracker, so it works while external issues are off.
- `view` (alias `show`) reads [any issue the owner could read](#g5.external-issues.cli.reads) like `gh issue view`, the primary by default, comments on request; `search` searches within [a shared budget](#g5.external-issues.cli.search-budget); `attachments` saves the issue's [attachments](#g5.attachments.cli).
- `comment`, `edit`, `close` and `reopen` act as the owner on the primary or any issue `view` can read, under the [comment](#g5.issue.comment-authority), [recovery](#g5.issue.comment-recovery), [edit](#g5.external-issues.edit) and [transition](#g5.external-issues.transition) rules, or the [native write rules](#g5.issue.native.writes) for a G5 issue.
- `edit` also changes labels under the [label contract](#g5.issue.labels.contract) with `--add-label`/`--remove-label`, `create` and `search` take `--label`/`--no-label`, and `g5d issue label` manages a domain's [catalog](#g5.issue.labels.surfaces).
- For `view`, `comment`, `edit`, `close` and `reopen`, a bare number names a linked issue with that number, else one in the creation domain; `link` and `fork --issue` use the creation domain; several matches are reported, never guessed.
- `--json FIELDS` selects gh field names where G5 has the data, and `--json` alone prints G5's full record; G5's own fields appear only when named, under names that cannot be mistaken for gh fields.
- `g5d workspace fork --issue REF` links an existing issue to the [new workspace](workspaces.g5.md#g5.story.agent-fork-child) as the Start-workspace dialog does, inside the fork and primary unless the agent asks for related, so `g5d issue create` then fork is the CLI's Create & Fork.

<a id="g5.external-issues.cli.reads"></a>
[D · g5.external-issues.cli.reads]

The agent's `view` and `search` read only the organization's configured issue domains, with the installation token, and only while the workspace owner is a member; an issue elsewhere is refused. The owner is the viewer for which workspaces link an issue. Issue text reaches the agent as context, not instructions, and a domain not marked trusted is named as such.

<a id="g5.external-issues.cli.search-budget"></a>
[D · g5.external-issues.cli.search-budget]

Agent searches are limited per workspace and, together, per organization, to a share of GitHub's search allowance that leaves the rest to people; people's own searches are not limited by G5. A refused search sends nothing to GitHub and says when to retry, and agent search is refused when the shared limit cannot be checked. Federated search spends the organization's shared installation token once per domain, and GitHub's search limit is low.

<a id="g5.workbench.issues"></a>
[D · g5.workbench.issues]

The workbench's issue surfaces — Home's Issues tile and a workspace's Issues & tasks rail — create and track issues in place, without leaving the page.

<a id="g5.workbench.issues.create"></a>
[D · g5.workbench.issues.create]

Home’s Create opens an empty form, whatever the active filters, showing the account, destination and any reason creation is unavailable; Create opens the issue page, and Create & Fork opens the existing Start-workspace dialog with the issue as primary. Creation stays out of the ordinary fork dialog, and the issue is kept if the fork fails.

- Workspace quick-add, for the current owner of a non-review workspace, creates the issue through the same flow and then links it as related.
- It preselects the primary issue as parent only when that issue is in the creation domain, and allows changing or clearing it.
- If linking fails, the issue stays available for normal import or linking.

<a id="g5.workbench.issues.arrival"></a>
[D · g5.workbench.issues.arrival]

A link that appears in the workspace's Issues & tasks list after the list first loaded is spotlighted once in place (a tint that fades; a static tint that clears under reduced motion), and the rail icon shows the linked count and pulses once when it rises, so a task created from elsewhere on the page, such as a [meeting decision](documents/workbench.g5.md#g5.workbench.meeting), is noticed without a whole-panel flash. The cue follows the data, never the press: nothing fires while the list first loads, when creation or linking fails, or for a detached task. The row and the count are the persistent indicators; the surface that caused the link announces it for screen readers.

## Attachments

<a id="g5.attachments"></a>
[D · g5.attachments]

An attachment is a file a tracker item or pull request carries — an image, video or other file in its description or discussion, or an image it shows from elsewhere. Listing an item's attachments and fetching one are content operations of the item's service, sent to the adapter that serves it: the [issue service](#g5.issue.native.adapter) for issues and the [pull-request service](../software_architecture/git.g5.md#g5.git.pr-adapter) for repository pull requests. The service keeps the item's usual admission — an [issue `view` can read](#g5.external-issues.cli.reads), or a readable pull request — and the adapter only finds and fetches, so no caller branches on the provider.

- A listing names each attachment by where it appears (the item, its description, or a comment or review with author and time), its kind (image, video or file), its name or alt text, its type and size when known, and an opaque handle; an attachment that cannot be retrieved is listed with the reason.
- A fetch names the item and a handle and returns that attachment's bytes, or why not. An adapter fetches only what its own listing named, never an address a caller supplies.
- Nothing stored outlives the attachment: the handle carries what is needed to find it again, and when the item no longer names the same file the fetch is refused for a new listing.
- Images and videos are recognized by content, every kind is bounded (by the provider's upload limits for external providers), and bytes stream through G5 rather than being held whole. Attachments, like issue text, are context and data, not instructions.

<a id="g5.attachments.adapters"></a>
[D · g5.attachments.adapters]

GitHub's [adapter](#g5.attachments.github) is shared by GitHub issues and pull requests; G5 issues hold their own [image attachments](#g5.issue.native.attachments), listed and fetched from G5's store.

- A local repository pull request has none to list: its description is G5's own, whose files the agent reads directly, and it carries no conversation, so the request is refused like its [conversation](../software_architecture/git.g5.md#g5.git.pr-local-conversation).
- Jira Cloud's adapter, when built, lists an issue's attachments by name, type and size and fetches them by id under the site's credential; Jira does not say which comment shows which file, so their place is the issue.

<a id="g5.attachments.github"></a>
[D · g5.attachments.github]

GitHub keeps no list of an item's attachments; they are the links its texts carry. The GitHub adapter finds them in GitHub's rendering of the description and, when asked, the comments, reviews and review comments, read with the credential the service admitted for the item. One mechanism serves both issues and pull requests; only the texts read and the credential differ.

- An image or video uploaded to a private repository points at a link GitHub signs for about five minutes, needing no further credential; G5 fetches it at once, sends no credential, and never lets the link leave G5. The handle names the text, the attachment's position and the text's version, so a fetch reads that text again.
- A file upload stays a plain link GitHub serves in a private repository only to a signed-in person, not to the App's tokens; G5 tries it without a credential, which succeeds for a public repository, and otherwise reports that GitHub serves the file only on its site.
- Images GitHub shows from elsewhere arrive through its own image proxy.
- G5 fetches only from the tracker's web origin and, on github.com, GitHub's content hosts, following redirects only among them, within GitHub's upload limits: 10 MiB for images, 100 MiB for videos, 25 MiB for other files.

<a id="g5.attachments.cli"></a>
[D · g5.attachments.cli]

`g5d issue attachments [REF]` names the issue as `view` does and, with `-c`, includes the discussion; `g5d pr attachments [PR]` does the same for a GitHub pull request named by number or URL (with `-R`) as `g5d pr view -c` names one, in a repository whose pull requests the owner [can read](../software_architecture/git.g5.md#g5.git.pr-read-repositories). Both share output and exit status, and one command per item family keeps the noun stable as kinds and adapters are added.

- It saves each retrievable attachment as a new file, never overwriting one, and prints its path with where it appears; `--list` only lists, so a large video is not fetched blind.
- An attachment not retrieved, or a listing cut off at its bound, is reported with its reason and the command exits 1 after saving the rest, so the agent says it could not compare rather than rely on what it has not seen.
- `view` points at the command when an issue's text carries attachments.
- A G5 PR's name, or a number in a repository not on GitHub, is refused before any read, as its [conversation](../software_architecture/git.g5.md#g5.git.pr-local-conversation) is; `g5d pr view` names a G5 PR's GitHub pull requests.

<a id="g5.attachments.cli.attach"></a>
[D · g5.attachments.cli.attach]

`--attach FILE`, repeatable on `g5d issue create`, `comment` and `edit`, adds images to a G5 issue as the owner, recorded as the agent's ([native attachments](#g5.issue.native.attachments)) and shown in order at the end of the text being written. An upload is keyed by its issue, file name and content, so a retry or re-attaching the same file adds it once. When the issue is on GitHub, or new issues go to GitHub, `--attach` is refused before anything is sent, since GitHub's API accepts no uploads.

- `comment` and `edit` upload before sending the text, so a failure leaves at most attachments listed at the issue.
- `create` creates the issue, uploads, then adds the images to the description only if it is still the one created; otherwise it leaves them listed at the issue, says so, and exits 1.
- Files must be regular files within the native bounds, checked before they are read whole, and the server still recognizes each by its content.

## Native G5 tracker

<a id="g5.issue.native"></a>
[I · g5.issue.native]

An organization can track issues in G5's built-in tracker, which stores them in G5. Search, primary and related links, Activity, comments, close/reopen, Create & Fork and `g5d issue` then work without an external integration. [image attachments](#g5.issue.native.attachments) return and [labels](#g5.issue.labels) return as a capability of every tracker.

<a id="g5.issue.native.scope"></a>
[I · g5.issue.native.scope]

Native issues carry only the common envelope: title, Markdown description, open/closed state, assignees, [labels](#g5.issue.labels), chronological comments, a parent and image attachments. Boards, custom workflows, templates and custom fields are out of scope; organizations that need them connect GitHub or Jira.

<a id="g5.issue.native.adapter"></a>
[D · g5.issue.native.adapter]

One issue service serves every issue surface, keeping the provider-neutral rules — the organization switch, caller authority, domains, links with their [primary](#g5.issue.primary) and [Base](#g5.issue.base-links) rules, and fork linking — and sending each content operation to the adapter for the domain's provider. Surfaces and `g5d` use a provider-neutral issue shape and name the issue's tracker instead of assuming GitHub.

- Content operations are resolve, search, read, activity, create, comment, comment edit, issue edit, state change, assignable users, listing and fetching [attachments](#g5.attachments), and the [label catalog and label changes](#g5.issue.labels.contract).
- External publication guards belong to the GitHub adapter: the comment switch, audience check, connected account, secret screen, markers, agent search budget and refused redirects. No native write leaves G5, so none apply to it.
- The native adapter is not a repository provider, so it sits outside ScmProvider.

<a id="g5.issue.native.attachments"></a>
[D · g5.issue.native.attachments]

A G5 issue's attachments are images uploaded to it — PNG, JPEG, GIF or WebP, recognized by content, at most 10 MiB each and 50 per issue — that belong to the issue, as on Jira. Each records who uploaded it (and whether the agent did, as its owner), when, and its file name, type, size and digest. Postgres keeps each record with the issue; the bytes live in the [attachment store](#g5.issue.native.attachment-store), and other files and video wait for a decision.

- A listing derives where each appears from the description and comments rather than storing a place, so an attachment no text shows is listed at the issue and nothing is orphaned.
- Readers see an attachment through an authenticated read that sends the image with its recognized type, `nosniff` and an inline disposition, never as a page.
- The uploader or an organization administrator removes one, which deletes its bytes; text that showed it then shows it as removed.
- GitHub issues take no uploads from G5, since GitHub's API accepts none. The agent lists and fetches images with [`g5d issue attachments`](#g5.attachments.cli) and adds them with [`--attach`](#g5.attachments.cli.attach).

<a id="g5.issue.native.attachments.upload"></a>
[D · g5.issue.native.attachments.upload]

Members with write permission upload by pasting, dropping or choosing a file, in the issue page's description editor and comment composers before the text is posted, and on the creation form. An upload carries the caller's idempotency key, so a retry returns the first upload and the same key with other content is refused.

- The creation form creates the issue first, then uploads and edits the description to show the images; an upload that fails leaves the issue created and says which image was not added.

<a id="g5.issue.native.attachments.placement"></a>
[D · g5.issue.native.attachments.placement]

Each uploaded image goes on its own line at the caret. In a box the person has not clicked or typed in, whose caret means nothing, it goes at the end — except that an image chosen with Attach image for a description is only attached to the issue, since a description need not show it.

<a id="g5.issue.native.attachment-store"></a>
[D · g5.issue.native.attachment-store]

Attachment bytes live outside Postgres, in G5's attachment store, so the database and its backups do not grow with them. Each is a write-once object named by its organization's id and a random id G5 chooses (`<org>/<two hex>/<uuid>`), never the uploaded file name. Only the attachments' owner reaches the store, and only inside a transaction scoped to the object's organization, which refuses another organization's name before building it, so tenancy rests on both row-level security and that check.

- The interface writes, streams and deletes whole objects and exposes no paths, renames or listings, so an object-store bucket can replace the directory later.
- An upload writes its bytes before its record and a removal deletes them after the record commits, so a record never names missing bytes.
  - A failure between leaves bytes no record names, which nothing reads and nothing yet collects.
- Hosted deployments keep the store on the Git Filestore share as its own API-only subdirectory mount, separate from the repository mount; self-hosted and development installations keep it under `.attachments` in the repository storage directory unless configured.
- The bytes share Git storage's [durability](repositories.g5.md#g5.repo.durability) rather than the SQL backup boundary, so like Git they are not backed up until hosted Filestore backups exist. Organization deletion, not yet offered, removes the organization's directory.

<a id="g5.issue.native.store"></a>
[D · g5.issue.native.store]

Native issues, their assignees, comments, state events and attachment records live in organization-owned Postgres tables under tenant row-level security, with no system access path; the tables fall inside the existing SQL backup boundary ([durability](repositories.g5.md#g5.repo.durability)) and are deleted with their organization. An issue's stable identity is a numeric G5 id, used as its link identity, and its display key is the domain key plus a per-domain number (`ACME-12`), fixed when the domain is created.

- Issue URLs point at the G5 issue page; summaries build them from the deployment's public address and the organization's slug when read, and a link keeps the URL it was made with, as GitHub links do.
- Deactivating or removing a member keeps their issues and comments.
- The issue holds its current state for filtering; each actual change of state or assignees also appends an event, which Activity shows.

<a id="g5.issue.native.domain"></a>
[D · g5.issue.native.domain]

Every tenant organization tracks issues with G5 issues from the start: its native domain, keyed from its slug and trusted, exists when the organization is created, and issues are on unless an administrator turns the switch off; the operator organization gets none. An administrator manages the native domain, external trackers and the issues switch from the organization's Issues settings on the Admin page, which no longer require the GitHub App.

- An administrator may remove the native domain, add it again under a chosen key, or add external trackers beside it.
  - Native and external domains coexist with one default creation domain, so connecting a tracker never requires moving native issues first.
- A native domain is trusted content by default, since only members and their agents write it, and the administrator can change that.
- The switch governs native issues like external ones: turning it off keeps native issues and leaves cleanup available.
- Removing a native domain that still has issues is refused, since nothing else could reach them.

<a id="g5.issue.native.writes"></a>
[D · g5.issue.native.writes]

An organization member with write permission may create, comment on, and close or reopen native issues through the existing entry points, and viewers read. A workspace agent acts as its owner for creating, commenting, closing, reopening and [editing](#g5.external-issues.edit) any issue it can read, linking and reading, shown under the owner; created issues and comments record agent origin. Native content is never screened for secrets or confirmed as a rendered payload, because it stays inside the organization, like chats and documents.

- Creation and comments carry the caller's idempotency key, unique per author; a retry returns the first result and reusing a key with other content is refused, so native creation is exactly-once.
- A parent must be an existing native issue in the same domain and is set in the creating transaction, so creation never partly succeeds.
- A state change applies the requested state and records an event only if the state actually changed.
- An author's comment edit applies only if the comment is unchanged since the editor read it, checked in the same update, so no edit is silently overwritten.
- G5 sends no notifications for native issues and derives no state change from its own operations, the latter pending the completion mapping.

<a id="g5.issue.native.search"></a>
[D · g5.issue.native.search]

Native search uses Postgres text search over title and description, filters by state and by G5 assignee (`@me` is the caller), and joins federated search like any domain under [search completeness](#g5.issue.search-completeness). It spends no external allowance, so the [agent search budget](#g5.external-issues.cli.search-budget) counts only external domains, and comments are not searched.

- Every word or quoted phrase must match; `-` negates and `OR` accepts either neighbour.
- A word matches the stemmed title and description, the state (`open`, `closed`), or an assignee's current name or email, so naming a person finds their issues. `assignee:` and `author:` take `@me`, a name or an email, and `no:assignee` keeps unassigned issues.
- `label:` keeps issues carrying the named label, matched without regard to case (an unquoted `a,b` names either), and `no:label` keeps unlabelled ones.
- Reserved qualifiers (`is:`, `state:`, `repo:` …) are dropped as on GitHub domains, and other GitHub qualifiers are refused with what G5 issues support.
- People are matched at query time from current profiles, never copied into the issue, so a renamed member is found under the new name.

<a id="g5.issue.native.page"></a>
[D · g5.issue.native.page]

Every issue's G5 page offers the write operations its adapter supports, so nobody has to leave G5. Members with write permission can comment on any issue there, edit their own comments, close or reopen it, edit its title, description, assignees and labels, and on a G5 issue add [image attachments](#g5.issue.native.attachments) (only the uploader or an organization administrator removes one); viewers read. Edits stay within the [scope](#g5.issue.native.scope), so milestones stay on the tracker.

- On a GitHub issue each write is the person's own under the [comment guards](#g5.issue.comment-authority), the untrusted-domain review, the [edit rules](#g5.external-issues.edit) and [comment recovery](#g5.issue.comment-recovery).
  - The page shows why a write is unavailable, such as a missing GitHub connection, and offers the assignees GitHub lists; a native edit applies only to the version the editor saw.
- A workspace tab shows an issue read-only with a link to its page, and the page links back to Home and to the workspace that holds the issue as primary, when the viewer can see that workspace. The agent edits through [`g5d issue edit`](#g5.external-issues.edit).

<a id="g5.issue.native.migration"></a>
[D · g5.issue.native.migration]

Connecting GitHub or Jira later leaves native issues and their workspace links working beside the new domain, and the administrator chooses the default creation domain. An administrator can export the native domain's issues, comments, events and attachments, with its label catalog and each issue's labels, as JSON, which G5 streams while holding one attachment's bytes at a time, so a domain's attachments never have to fit in memory together.

- A one-way move tool is later work, which stable native identity keeps feasible: it would create each issue under the mover's account with original authors quoted and attachments linked back to G5 (GitHub's API accepts no uploads), close the native issue with a pointer, and re-point active links, primary included, in one transaction.

## Labels

<a id="g5.issue.labels"></a>
[I · g5.issue.labels]

Labels classify issues in every issue domain, native and GitHub alike. People and workspace agents acting as their owners manage a domain's label catalog and tag the issues they may write, in the UI and through `g5d`.

- A name the catalog does not have is refused, never silently created.
- G5 ships no built-in taxonomy or workflow: no seeded labels, exclusive groups, automation, boards or custom fields.
  - `priority:p0`–`priority:p3` and `source:formal-verification` are suggested conventions only; a label classifies an issue, it is not evidence that verification passed.

<a id="g5.issue.labels.contract"></a>
[D · g5.issue.labels.contract]

Labels are [adapter](#g5.issue.native.adapter) content operations behind the one issue service, with no separate label service or authority: list a domain's catalog, create, edit and delete a catalog label, change an issue's labels, and select them at creation. A label has an opaque id in its domain, a name, an optional six-hex-digit colour and a description. Callers name labels by name, matched without regard to case, or by catalog id, and labels are never identified or synchronized across domains.

- The adapter validates a request before asking any provider.
  - A blank name, an invalid colour, a label both added and removed, or an over-limit request is refused; a provider may refuse more.
- Label changes are deltas, so a concurrent change to another label survives.
  - Replacing a priority is removing the old label and adding the new; there is no exclusivity and no atomic multi-label promise.
  - Catalog edits are last-writer-wins.
- Each requested change reports its own outcome with the labels then observed, so a partial result is visible.
- A request that also edits issue fields checks everything before it writes.
  - A native issue changes its fields and labels in one transaction.
  - On GitHub the field edit goes first, then each label change reports its own outcome, so a later label refusal leaves the request partial.
- A catalog create is safe to retry: a taken name returns the matching label or reports the conflict, and nothing is overwritten.

<a id="g5.issue.labels.native"></a>
[D · g5.issue.labels.native]

A native domain's catalog and each issue's labels live in tenant Postgres tables beside the [native store](#g5.issue.native.store). A label's stable id keeps its assignments through a rename, and issues read the catalog's current name and colour; names are unique within the domain, case-insensitively. Members with write permission change the catalog and tag issues, the agent as its owner, and viewers read.

- A label change runs in one transaction, locking the labels and then the issue and recording events only for actual changes.
- Deleting a catalog label removes it from every issue in the same transaction, recording the removal on each.
  - Events keep the label's name at the time, so a later rename or deletion does not rewrite history.
- Creation attaches labels in its existing exactly-once transaction, and label history reuses the event stream and export.

<a id="g5.issue.labels.github"></a>
[D · g5.issue.labels.github]

GitHub stays authoritative for its catalog and issue labels. The catalog is read with the domain's installation token; every label write is the person's or agent owner's own under the [comment-publication permissions](#g5.issue.comment-authority), with secret screening and no redirects. GitHub may let someone tag issues but not manage the catalog, and its refusal is reported as given.

- An added label must already exist, since GitHub's add would otherwise create it; a label deleted between the check and the add can still be recreated.
- A label change can partly apply: a refusal or unknown answer stops it and leaves the rest not attempted.
  - An unknown outcome is re-read before an explicit retry, which may reassert a membership another writer changed.
- Labels selected at creation ride on the [creation protocol](#g5.external-issues.creation.recovery); GitHub silently drops labels the author may not set, so missing ones are reported and repaired by an explicit change.

<a id="g5.issue.labels.surfaces"></a>
[D · g5.issue.labels.surfaces]

The UI uses one label picker for creation and the issue page, the existing coloured chips for display, and a small catalog editor for a domain, reached from the picker and the organization's issue settings; it shows why a write is unavailable and any partial or unknown result. A label change refreshes every displayed copy of the issue, a catalog edit refreshes the catalog and the labels shown, and a selection stays bound to the issue and domain it was opened for, even after navigation or a change of default domain.

- `g5d issue label` manages a domain's catalog, and `g5d issue edit --add-label/--remove-label` and `create --label` tag issues.
  - `--domain` defaults to the creation domain; only `label delete` removes a catalog label.
- Search keeps a label's exact meaning and quoting through both trackers.

## Metadata

### g5.issue

- agent
- Defines the `g5.issue` ID level: a unit of work tracked outside any one workspace, served through a provider-neutral shape. Composed from [Ben's ontology](../../../docs/ben_new_ontology.md#L293) issue lines and the [native tracker](#g5.issue.native) ([issue #681](https://github.com/G5Labs/G5-Lean/issues/681)).

### g5.issue.providers

- human-provided
- Source: [Ben, line 293](../../../docs/ben_new_ontology.md#L293).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.domains

- human-provided
- Source: [Ben, line 294](../../../docs/ben_new_ontology.md#L294).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.domain-policy

- human-provided
- Source: [Ben, line 295](../../../docs/ben_new_ontology.md#L295).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.search

- human-provided
- Source: [Ben, line 296](../../../docs/ben_new_ontology.md#L296).

### g5.issue.references

- human-provided
- Source: [Ben, line 297](../../../docs/ben_new_ontology.md#L297).

### g5.issue.primary

- human-provided
- Source: [Ben, line 298](../../../docs/ben_new_ontology.md#L298).
- [IssueLinks](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks primary-link uniqueness for both workspaces and issues when claims race. It assumes the database's unique indexes work as specified; authorization and deletion locks were inspected separately.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 13 (IS-Q2).

### g5.issue.base-links

- human-verified
- Source: the owner's [issue #482](https://github.com/G5Labs/G5-Lean/issues/482) ("If Base is read-only, it's a little weird you can attach an issue to it."). Refusing with the read-only error, keeping unlink available and leaving Base out of Home's link targets are agent decisions.
- `create_workspace_link` (`g5-api-core` external issues) refuses a Base target with `WORKSPACE_READ_ONLY`, which covers the person's route, the agent's internal route and `g5d issue link`; the workspace page and the classic details panel carry it as `linkBlockedReason` (`BASE_ISSUE_LINK_REASON`), separate from the review copy's `writeBlockedReason` so unlinking is not blocked. Tests: `api_external_issues.rs` `base_takes_no_issue_links`, `WorkspacePageAlignment.test.tsx`, `HomePage.test.tsx`.

### g5.issue.identity

- human-provided
- Source: [Ben, line 299](../../../docs/ben_new_ontology.md#L299).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.link-authority

- human-provided
- Source: [Ben, line 300](../../../docs/ben_new_ontology.md#L300).

### g5.issue.live-read

- human-provided
- Source: [Ben, line 301](../../../docs/ben_new_ontology.md#L301).
- [IssueRead](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks domain authorization and stable issue identity before linked content is shown. Permission changes during a provider request and provider parsing are outside the model.
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.activity

- human-provided
- Source: [Ben, line 302](../../../docs/ben_new_ontology.md#L302).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 14 (IS-Q1).

### g5.issue.read-failure

- human-provided
- Source: [Ben, line 303](../../../docs/ben_new_ontology.md#L303).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.comment-identity

- human-provided
- Source: [Ben, line 304](../../../docs/ben_new_ontology.md#L304).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.comment-entry

- human-provided: the owner requested consistent Activity Reply actions; [direction](../../chat_intent/issue-816-workspace-landing/activity-cleanup.md). Agent: reuse the existing destination-bound issue composer with a quote rather than create a second comment lifecycle.
- agent: [follow-up interpretation](../../chat_intent/issue-704/approvals-follow-up.md) separates workspace feedback from comments published to GitHub while giving each one creation destination.
- Design: [unified Approvals](../../pr_ontology_diff_design/issue-704/ontology-plan.md).

### g5.issue.comment-authority

- human-provided
- Source: [Ben, line 305](../../../docs/ben_new_ontology.md#L305).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.issue.comment-recovery

- human-provided
- Source: [Ben, line 306](../../../docs/ben_new_ontology.md#L306).
- 2026-10-29: the owner replaced issue comments' publication records with key-derived markers ("What if we deleted it, then we wouldn't have a problem with issues w/o a workspace?", then "Your recommend looks good") in the [#808 chat intent](../../chat_intent/issue-808-g5d-issue-edit/request.md#later-instruction-2026-10-29-external-issues-edited-in-the-platform); PR comments follow in [G5-Lean#950](https://github.com/G5Labs/G5-Lean/issues/950). Implemented: `keyed_marker_id`, `publish_github_comment` (look up with `find_marked_comment`, then admit and post) and `edit_github_comment` in `routes/external_issues.rs`. Existing issue-comment rows are left unread, without a migration, until the table is dropped (#950; the owner's decision of 2026-10-30). Tests: `an_unclear_comment_is_found_by_its_marker_when_posted_again`, `comments_post_as_the_person_only_to_approved_non_public_destinations`, `edits_replace_only_the_person_s_own_unchanged_comment` and the `sec_h02_*_in_flight` lock tests (`api_external_issues.rs`).
- [CommentRecovery](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks a fixed destination, recording before send, duplicate-send prevention and recovery after a lost response. A crash before sending can remain unresolved, and provider marker visibility is assumed.
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 15 (IS-Q3).

### g5.issue.comment-draft-identity

- agent: preserve the existing draft and replay bindings while moving the entry, as recorded in the [design](../../pr_ontology_diff_design/issue-704/ontology-plan.md).

### g5.issue.platform-bug

- human-provided
- Source: [Ben, line 307](../../../docs/ben_new_ontology.md#L307).

### g5.issue.fork-primary-race

- agent
- Source: merged change `1331c4a5`.
- [IssueLinks](../../uplift/ben-ontology/investigations/git.md) checks that a losing fork can finish with a related link while the primary claim stays exclusive. Database uniqueness and organization/tracker separation are assumed.

### g5.issue.search-completeness

- agent
- [IssueRead](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks that a failed search domain remains visible alongside successful results. Provider pagination, parsing and truncation behavior were inspected rather than modeled.

### g5.issue.search.one-path

- human-verified, agent
- Source: the owner in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md#later-instruction-2026-10-23-search) ("This box should use the same code to avoid having multiple issue search mechanisms"); reference lookup through search is an agent decision in the [design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#structured-native-search-and-one-search-for-linking).
- Reason for one path: a second, resolver-only path failed on plain words and on deployments without the GitHub App.
- Implemented: the reference path in `search_github_domain` (`services/git/external_issues.rs`); `WorkspaceIssuesPanel` and `ImportTaskPicker` share `useIssueSearchText` and `IssueSearchOutcome`; the Home Issues tile keeps its own pause on the same `lib/issueSearchQuery.ts` rules. Tests: `search_resolve_and_detail_read_github_within_the_domain`, `WorkspaceIssuesPanel.test.tsx`.

### g5.issue.comment-edit-observation

- agent
- [IssueRead](../../uplift/ben-ontology/reviews/workspace-git-full.md) checks author, destination and observed version before accepting an edit. It does not make the provider's later write conditional or prevent intervening provider edits.

### g5.external-issues

- agent
- Defines the `g5.external-issues` ID level: issues in a connected tracker such as GitHub, written through the provider's protocol. Source is the external-issue [design](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md); detail is in the creation, transition, edit and CLI children below.

### g5.external-issues.creation

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md).
- “Member” does not override organization role ceilings. Neither issue creation nor issue comments keep a ledger (G5-Lean#808).
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.external-issues.creation.admission

- agent
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md).
- Reuses [comment authority](#g5.issue.comment-authority); provider permissions still apply. The UI and workspace-agent routes reach the same admission owner.

### g5.external-issues.creation.recovery

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md); [accepted stateless design](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/pr_ontology_diff_design/issue-478-create-issue/ontology-plan.md).
- The lookup covers at most 24 hours and 1000 author issues. A check never creates; an explicit retry may create after not-found. These bounds and provider-listing delay limit deduplication.

### g5.external-issues.creation.parent

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md).

### g5.external-issues.transition

- human-verified
- Source: the owner's instruction of 2026-10-15 in the [chat intent](../../chat_intent/issue-tracker-writes/request.md); the Manual reading, the reuse of the comment permissions, the stateless idempotent write and the no-redirect rule (found by review) are agent decisions in the [design](../../pr_ontology_diff_design/issue-tracker-writes/ontology-plan.md), following the design doc's [status defaults](../../../docs/external-issue-integration-design.md#status-defaults-and-the-definition-of-done) ("Manual: a user publishes or changes state").
- Any readable issue for the agent's close/reopen is the owner's 2026-10-29 answer in the [#808 chat intent](../../chat_intent/issue-808-g5d-issue-edit/request.md), replacing "an issue linked to its own workspace".
- The agent's transition follows the owner's 2026-10-22 decision in the [G5-Lean#710 question set](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace) ("allow agents to close and reopen"), replacing the earlier exclusion; using the owner's connected account and the same permissions is agent design.
- `POST /workspaces/:id/issue-links/:linkId/state` (`change_linked_issue_state`, sharing `admit_person_write` with comments and creation); `g5-scm` `update_issue_state`; writes go through a client that follows no redirect. The agent's `POST /api/internal/workspace-agent/:id/issue-state` (`g5d issue close|reopen`) chooses the issue like `comment` and calls `change_issue_state_as`, which the person's `change_linked_issue_state_as` also calls after finding the link; the agent's transitions on unlinked issues are tested in `api_native_issues.rs` and `api_external_issues.rs` (GitHub mock), its refusal without a primary issue in `issue_writes_without_a_reference_need_a_primary_issue`. Tests: `api_external_issues.rs` `a_person_closes_and_reopens_a_linked_issue_as_themselves`, `g5-scm` `issue_state_update_is_classified`, `WorkspaceIssuesSection.test.tsx`, `WorkspacePageAlignment.test.tsx`. Live GitHub and the browser flow are not exercised.
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.external-issues.edit

- human-verified
- Children: [assignees](#g5.external-issues.edit.assignees), [outcome](#g5.external-issues.edit.outcome).
- Source: [issue #808](https://github.com/G5Labs/G5-Lean/issues/808) (updates, assignment, clear outcomes, never reporting an unapplied change); listeners out of scope and edits on any readable issue are the owner's 2026-10-29 answers in the [chat intent](../../chat_intent/issue-808-g5d-issue-edit/request.md). The single write, kept marker, pre-write assignee check and response check are agent decisions in the [design](../../pr_ontology_diff_design/issue-808-g5d-issue-edit/ontology-plan.md).
- Implemented: `POST /api/internal/workspace-agent/:id/issue-edit` (`g5d issue edit`) chooses the issue with `agent_write_target` (shared with `comment` and `close|reopen`) and calls `edit_issue_as`, which dispatches to `native_issues::change_issue` or to `admit_person_write(Edit)`, `validate_edit`, `domain_login_assignable` and one `g5-scm` `update_issue`. Tests: `api_native_issues.rs` `the_workspace_agent_edits_closes_and_reopens_any_g5_issue_as_its_owner`, `api_external_issues.rs` `the_workspace_agent_edits_and_closes_an_unlinked_github_issue_as_its_owner` (GitHub mock), `g5-scm` `issue_edit_sends_only_the_edited_fields_and_checks_assignees`, `issue_creations` `edits_name_a_change_and_keep_the_creation_marker`, and the `g5d` parsing and rendering tests. Live GitHub is not exercised; GitHub's `PATCH` stays last-writer-wins.

### g5.external-issues.edit.assignees

- human-verified
- Expands: [edit](#g5.external-issues.edit). Assignee rules from [issue #808](https://github.com/G5Labs/G5-Lean/issues/808); the pre-write assignable and conflict checks are agent decisions in the [design](../../pr_ontology_diff_design/issue-808-g5d-issue-edit/ontology-plan.md).

### g5.external-issues.edit.outcome

- human-verified
- Expands: [edit](#g5.external-issues.edit). Clear outcomes and never reporting an unapplied change are from [issue #808](https://github.com/G5Labs/G5-Lean/issues/808); last-writer-wins on GitHub and the field comparison are agent decisions in the [design](../../pr_ontology_diff_design/issue-808-g5d-issue-edit/ontology-plan.md).

### g5.external-issues.cli

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md); `view` of any issue, `search`, `unlink` and the gh surface from the owner's [issue #558](https://github.com/G5Labs/G5-Lean/issues/558), whose proposal is the bare-number order. G5 fields as opt-in, distinctly named fields were confirmed by the owner in chat (2026-10-15).
- Unlinking the primary, `--json` alone keeping the full record, `-R/--repo` and refusing reserved query qualifiers are agent decisions in the [#558 design](../../pr_ontology_diff_design/issue-558-g5d-issue-reads/ontology-plan.md). The old linked-issue read route stays for older `g5d` binaries.
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).
- `comment [REF]` on any readable issue: the owner's instruction of 2026-10-22 in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md) ("We should support commenting on any issue"). `post_issue_comment_internal` resolves the argument with `choose_agent_issue` (shared with `view`); `publish_issue_comment` dispatches by provider. Tests: `api_native_issues.rs` `the_workspace_agent_creates_reads_links_and_comments_on_any_g5_issue_as_its_owner`, `g5d` `comment_names_any_issue_as_view_does`, and `api_external_issues.rs` `the_workspace_agent_comments_on_an_unlinked_github_issue_as_its_owner` (GitHub mock).
- `fork --issue`: the owner's 2026-10-26 request in the [G5-Lean#710 workspace](../../chat_intent/issue-710-workspace-agent-parity/request.md#decisions-in-this-workspace) to fork a workspace with a primary issue from the command line, replacing "CLI Create & Fork is outside this feature". Reusing the fork's own link, the primary default (as in the dialog) and the bare-number rule of `link` are agent choices ([design](../../pr_ontology_diff_design/issue-710-workspace-agent-parity/ontology-plan.md#increment-6-fork-with-an-issue-and-member-lookup)).

### g5.external-issues.cli.reads

- human-verified
- Source: the owner's [issue #558](https://github.com/G5Labs/G5-Lean/issues/558): reads of any issue in the configured domains on the installation token, the out-of-scope domains, the owner as viewer and the untrusted-content concern.
- Membership through the owner and the finding that only the current owner can prompt the agent are the agent's, in the [#558 design](../../pr_ontology_diff_design/issue-558-g5d-issue-reads/ontology-plan.md#who-can-drive-the-agent): the incarnation fence rechecks the current owner, and the database keeps a live workspace's owner a member. Live GitHub is not exercised.
- Reason for reading the domains rather than only linked issues: `link` could already name any issue in those domains and left unwanted links behind, while reading beyond the domains would need another credential and a policy decision.

### g5.external-issues.cli.search-budget

- human-verified
- Source: the owner's [issue #558](https://github.com/G5Labs/G5-Lean/issues/558) ("bound agent search so it cannot starve people's searches"); the window sizes and failing closed are agent decisions in the [#558 design](../../pr_ontology_diff_design/issue-558-g5d-issue-reads/ontology-plan.md#search-budget).
- 6 searches a minute per workspace and `max(1, 15 / configured domains)` per organization, so every search in a window shares one limit. Above 15 domains one search still exceeds 15 requests; the window sizes are not checked against live GitHub limits.

### g5.workbench.issues

- agent
- Defines the `g5.workbench.issues` ID level: the workbench's in-place issue surfaces (Home's Issues tile and a workspace's Issues & tasks rail), a child of the `g5.workbench` concept in [ui.g5.md](ui.g5.md). Children: [create](#g5.workbench.issues.create), [arrival](#g5.workbench.issues.arrival).

### g5.workbench.issues.create

- human-verified
- Source: [human request and decisions](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/chat_intent/issue-478-create-issue/request.md).
- The retained interpretation reuses the existing Base-based Start dialog, not a new source picker. Done/Reopen are [hand transitions](#g5.external-issues.transition) from the rail's Done box and the issue page, disabled with the reason on a review copy, without a GitHub connection or while the issue cannot be read. Field validation and parent partial success use the same creation owners.
- Native tracker wording from [issue #681](https://github.com/G5Labs/G5-Lean/issues/681); the owner reversed decision 14 for the native tracker in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md).

### g5.workbench.issues.arrival

- human-provided
- The owner's 2026-10-22 request for a small animation on the issue sidebar when an attached task is created, narrowed by the UI-skills research recorded in the [meeting-task-buttons plan](../../pr_ontology_diff_design/meeting-task-buttons/ontology-plan.md) (change blindness, spotlight over flash, one or two moving elements, WCAG 2.3.1, reduced motion).
- Implemented in `WorkspaceIssuesSection.tsx` (row spotlight) and `TreeNode.tsx` (rail badge and pulse); keyframes in `index.css`; tests in `WorkspaceIssuesSection.test.tsx`.

### g5.attachments

- human-provided, agent
- Source: the owner's [issue #1013](https://github.com/G5Labs/G5-Lean/issues/1013) (inspectable attachments through a supported, bounded path; follow repository permissions; expose no long-lived credential; persist no signed URL; report unavailability before relying on visual fidelity) and its assignee's comment that VMs have no GitHub credentials, so G5 must shim the read. The owner's [chat direction](../../chat_intent/issue-1013-issue-images/request.md) (2026-11-01) asked for one attachment abstraction for remote issues and pull requests, named so that video needs no new name, reusing one fetch mechanism. Placing the operations in the issue and pull-request adapters, deferring pull requests until G5-Lean#830 lands, and a local adapter for G5 issues are the owner's direction of 2026-11-01 in the same chat intent; the owner asked for `g5d pr attachments` on 2026-11-03, after #830 landed. The record, the handle, streaming and the bounds are agent decisions in the [design](../../pr_ontology_diff_design/issue-1013-issue-images/ontology-plan.md), which also recommends the G5-issue adapter.
- Jira Cloud is from its [attachment API](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-attachments/) and community reports that ADF media ids are not attachment ids; Jira support itself is not built.
- Implemented: `services/git/external_issues/attachments.rs` (`list_issue_attachments`, `open_issue_attachment`, per-domain dispatch, opaque handles), the shared kinds and content recognition in `g5-contracts` `attachments`, and `GET /api/internal/workspaces/:id/issues/attachments` (+ `/content`, streamed with `nosniff`) chosen with `choose_agent_issue` as for `view`. Tests: `api_external_issues.rs` `the_workspace_agent_receives_a_github_issue_s_private_attachments_through_g5` (GitHub mock), `api_native_issues.rs` `members_attach_images_to_g5_issues_and_the_agent_reads_them`, the handle tests in the service module. Pull requests: see [g5.attachments.cli](#g5.attachments.cli).

### g5.attachments.adapters

- agent
- Expands: [attachments](#g5.attachments).
- Per-provider dispatch for attachments: the shared GitHub adapter, G5 issues' own store, refused local pull requests, and planned Jira. Same sources as [attachments](#g5.attachments) ([issue #1013](https://github.com/G5Labs/G5-Lean/issues/1013)).

### g5.attachments.github

- agent
- Evidence: GitHub's [private attachments change](https://github.blog/changelog/2023-05-08-more-secure-private-attachments) (images and videos need sign-in); a third party's [token experiment](https://github.com/AdvanGrid/beghu/issues/62) (2026-10-24: installation and `ghu_` tokens get 404 on the raw asset link, while `body_html` carries anonymously loadable links valid 300 s); [file links stay unsigned](https://github.com/cbusillo/codex-skills/issues/2021) and need a classic user token. Videos being signed like images, and GitHub Enterprise Server's hosts, are not verified; live GitHub is not exercised.
- Implemented: `g5-scm` `providers/issues/attachments.rs` (rendered reads, `<img>`/`<video>`/upload-link finding, `open_attachment`) over `GithubHttp::open_hosted` (credential-free, redirects checked by hand, length and deadline bounded). Tests: `issue_attachments_come_from_github_s_rendering_without_a_credential`, `an_attachment_without_a_declared_length_is_cut_off_at_its_limit`, `an_attachment_comment_must_be_on_the_issue` and the module's parsing and host tests.

### g5.attachments.cli

- agent
- Design in the [plan](../../pr_ontology_diff_design/issue-1013-issue-images/ontology-plan.md#cli).
- Implemented: `g5d` `cli/issue_attachments.rs` (list, streamed saving with length check and removal of partial files; one implementation for issues and pull requests), the `view` hint in `issue_links.rs`, and the agent instructions in `.agents/AGENTS.md`. Tests: the module's naming and report tests, `attachments_names_the_issue_as_view_does`, `view_points_at_attachments_only_when_the_shown_text_carries_them`.
- `g5d pr attachments` (2026-11-03, owner request in the [chat intent](../../chat_intent/issue-1013-issue-images/request.md#pull-request-attachments-and-agent-uploads-2026-11-03); refusing G5 PRs is the agent's choice in the [plan](../../pr_ontology_diff_design/issue-1013-issue-images/ontology-plan.md#pull-requests)): `g5-scm` `providers/pull_attachments.rs` (description, conversation comments, reviews and review comments in `full+json`, one text read again for a fetch), `services/git/workspace_pull_request_attachments.rs` (selection and the owner's read grant shared with `view` through `github_pull_request_access`, handles per text kind), internal routes `…/pull-requests/github/attachments[/content]`. Tests: `pull_request_attachments_come_from_every_text_in_github_s_rendering`, `handles_name_each_kind_of_text_and_refuse_anything_else`, `the_workspace_agent_reads_a_github_pull_request_s_attachments_through_g5`, `pr_attachments_names_a_github_pull_request_as_view_does`.

### g5.attachments.cli.attach

- human-provided, agent
- Source: the owner's request for "command line support for --attach" (2026-11-03, [chat intent](../../chat_intent/issue-1013-issue-images/request.md#pull-request-attachments-and-agent-uploads-2026-11-03)); the commands, keys, sequencing and bounds are agent design in the [plan](../../pr_ontology_diff_design/issue-1013-issue-images/ontology-plan.md#agent-uploads).
- Implemented: internal `POST …/issues/attachments` (`upload_issue_attachment_internal`: the agent's write target, GitHub refused before decoding, key from issue, name and digest, origin `agent`), `attachments: true` on the agent's create, `expectedBody` on the agent's edit (`NativeIssueChange::expected_body`, checked against the read the write is conditional on; refused for GitHub), `g5d` `cli/issue_attach.rs` and the create, comment and edit handlers. Tests: `the_agent_attaches_images_to_g5_issues_once_and_shows_them_conditionally`, `the_agent_cannot_attach_images_where_issues_are_on_github`, `only_bounded_regular_image_files_are_read`, `attach_is_repeatable_on_create_comment_and_edit`.

### g5.issue.native

- human-provided
- Source: the owner's [issue #681](https://github.com/G5Labs/G5-Lean/issues/681) ("Done when"); reversing decision 14 for the native tracker only was approved by the owner in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md) (question `reverse-decision-14`).
- [Design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md); [validation](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#validation).

### g5.issue.native.scope

- human-provided
- Source: [issue #681](https://github.com/G5Labs/G5-Lean/issues/681), "Scope is deliberately the adapter's common envelope" and "Guard the scope rule against growing into a full tracker".

### g5.issue.native.adapter

- agent
- Source: [issue #681](https://github.com/G5Labs/G5-Lean/issues/681) asks for a provider behind the existing service; the split of guards is the [design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#ownership-and-the-adapter-seam)'s. Reason for one service and one code path: the issue asks for one experience without spreading GitHub's publication protocol into native writes.
- Implemented: `IssueBackends`/`NativeBackend` and per-domain dispatch in `services/git/external_issues.rs` (search, resolve, detail, link summaries, timeline); writes dispatch in `routes/external_issues.rs` (`create_tracker_issue`, `publish_issue_comment`, `change_linked_issue_state`). The instance feature `issues` is always offered; `github_client` is required only on GitHub paths. Tests: `api_native_issues.rs` (no GitHub App configured).

### g5.issue.native.attachments

- human-provided, agent
- Source: [G5-Lean#726](https://github.com/G5Labs/G5-Lean/issues/726) ("I can't post images in issue creation"); the owner accepted the recommended local adapter and asked for it in this PR, reversing decision 14's retirement of attachments for images, on 2026-11-01 in the [chat intent](../../chat_intent/issue-1013-issue-images/request.md). The Postgres record (bytes moved to the [attachment store](#g5.issue.native.attachment-store) on 2026-11-03), issue ownership, derived places, bounds, removal and the creation sequence are agent decisions in the [design](../../pr_ontology_diff_design/issue-1013-issue-images/ontology-plan.md#local-adapter).
- Implemented: migration `147_native_issue_attachments.sql` (tenant row-level security, the record and its store id, type and size checks), `native_issues/attachments.rs` (idempotent upload under the issue's row lock, derived places, removal, export rows), routes `/api/v1/orgs/:orgId/issues/g5/:number/attachments` (upload, list) and `/:attachmentId` (read; remove by the uploader or an organization Admin). Test: `members_attach_images_to_g5_issues_and_the_agent_reads_them`.
- Placement: the owner's 2026-11-04 [instruction](../../chat_intent/issue-1013-issue-images/request.md#where-an-attached-image-goes-2026-11-04), after an image chosen without a caret landed at the top of a description ("bottom is better … if not, then leave it out of the description entirely if I never clicked"); a click or keystroke places the caret, focus alone does not.
- Browser: `NativeAttachments.tsx` (authenticated image rendering through object URLs, paste/drop/button uploads with retry under the same key, `useCaretPlacement`, the issue page's attachments list with Remove for the uploader or an organization Admin), `useNativeAttachments.ts`, `lib/nativeAttachments.ts`, wired into `ExternalIssueDetailPanel`, `IssuePageControls`, `ActivityCommentComposer` (issue page only), `WorkspaceActivityPanel` and `WorkspaceTaskSection` (rendering: the workspace task and linked-issue reader) and `CreateIssueDialog` (placeholders, or a removable list for images only attached, then upload and one conditional description edit, retried once on `issue_changed`). Text from a domain marked untrusted keeps showing images as text; another organization's address is never fetched. Tests: `NativeAttachments.test.tsx`, `nativeAttachments.test.ts`, `WorkspaceLanding.test.tsx`. Workspace Activity's composer takes no uploads.

### g5.issue.native.attachments.upload

- human-provided, agent
- Expands: [native attachments](#g5.issue.native.attachments). Upload surfaces, idempotency key and the creation-form sequence are agent decisions in the [design](../../pr_ontology_diff_design/issue-1013-issue-images/ontology-plan.md#local-adapter).

### g5.issue.native.attachments.placement

- human-provided
- Expands: [native attachments](#g5.issue.native.attachments). Placement is the owner's 2026-11-04 [instruction](../../chat_intent/issue-1013-issue-images/request.md#where-an-attached-image-goes-2026-11-04): bottom when no caret, out of the description entirely when never clicked; a click or keystroke places the caret, focus alone does not.

### g5.issue.native.attachment-store

- human-provided, agent
- Source: the owner's [2026-11-03 direction](../../chat_intent/issue-1013-issue-images/request.md#attachment-store-2026-11-03) (Filestore instead of database bytes, a second mount, the organization's id in the name for a path guard, backups left to #977, no erasure until organization deletion exists, shaped for a later object store). Choices and the rejected alternatives are in the [design](../../pr_ontology_diff_design/issue-1013-issue-images/ontology-plan.md#attachment-store).
- Implemented: `g5_storage::AttachmentStore` with `DirectoryAttachmentStore` (temporary file, sync, rename; names built only from typed ids), `IssueAttachmentStore` in `native_issues/attachments.rs` (the scoped-organization check; `RemovedAttachment` deletes bytes only after commit), `G5_ATTACHMENT_STORE_DIR`, migration `147_native_issue_attachments.sql` (`storage_id`, no bytes). Helm: the API mounts subPath `attachments` at `/mnt/g5-attachments`; API and daemon mount subPath `repos` at the repository base; the init container creates both directories on the share root.
- Assumption: nothing on the hosted share outside `repos/` is used by the repository store; narrowing the mount relies on it (`FilestoreRepoStorage` reads and writes only `{mount}/{prefix}`).

### g5.issue.native.store

- agent
- [Design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#native-store). Backup, tenant authority and erasure are the issue's listed considerations.
- Implemented: migration `112_native_issues.sql`, `services/git/native_issues.rs`. Member profiles and memberships come from the system pool by organization, as other member listings do; issue tables only through the tenant scope. Test: `g5_issues_stay_in_their_organization_and_viewers_only_read`.

### g5.issue.native.domain

- agent
- Coexistence with external domains is the owner's answer in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md) (question `availability`).
- [Design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#settings-and-gating).
- Implemented: `create_native_domain`, `delete_domain` refusal (`domain_has_issues`), the comment switch refused for native domains; UI `OrgIssuesSettingsSection.tsx` and `IssueDomainSettingsSection.tsx` (Use G5 issues, Export). Tests: `api_native_issues.rs`, `OrgAdminPageContent.test.tsx`.
- "enable internal issue tracking by default if no external issue tracking is enabled" ([chat intent](../../chat_intent/davide-ui-assorted/request.md), 2026-10-29): `org_external_issues_enabled` reads the switch as on unless `false`; `OrgService::create_org` adds the native domain for tenant organizations; migration `137_native_issue_domain_default.sql` adds it to existing organizations without a domain; the client flag `isOrgExternalIssuesEnabled` matches. Tests: `api_native_issues.rs` (default switch and new-organization domain), `OrgAdminPageContent.test.tsx`.

### g5.issue.native.writes

- agent
- [Design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#native-writes). The idempotency, conditional-edit and no-screen choices follow from no write leaving G5. Deriving a state change from G5's own operations waits for the completion mapping ([#682](https://github.com/G5Labs/G5-Lean/issues/682)).
- Implemented: `native_issues::{create_issue, add_comment, edit_comment, set_state, edit_issue, change_issue}`; `change_issue` is the agent's edit by added and removed people, conditional on the version it reads. Tests: replay and changed-key refusal, parent rules, conditional edits, one event per actual change, agent origin (`api_native_issues.rs`).

### g5.issue.native.search

- human-verified, agent
- [Design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#reads-search-and-references).
- Implemented: `native_issues::search` (GIN `tsvector` plus title fragment and key/number). Test: `an_organization_without_integrations_creates_searches_and_reads_g5_issues`; the agent search reserves no budget for native-only searches.
- The words-match-people-and-state rule and the `assignee:`/`author:`/`no:assignee` qualifiers come from the owner in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md#later-instruction-2026-10-23-search) ("inject the assignee and state information so keyword search of an assignee will turn up their bugs"); matching at query time and refusing other qualifiers are agent decisions in the [design](../../pr_ontology_diff_design/issue-681-native-g5-issue-tracker/ontology-plan.md#structured-native-search-and-one-search-for-linking). Implemented: `native_issue_query.rs` (grammar, person matching) and `SearchPeople`/`SearchSql` in `native_issues.rs`; tests: the parser's unit tests and the qualifier table in `an_organization_without_integrations_creates_searches_and_reads_g5_issues`.

### g5.issue.native.page

- human-verified
- Found while designing (native issues have no other tracker UI); the owner approved issue-page comment, close/reopen and edits in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md) (question `native-page-actions`).
- Implemented: `/api/v1/orgs/:orgId/issues/g5/:number` routes and `IssuePageControls.tsx` (shared by both trackers) on the issue page; an issue opened as a workspace tab stays read-only with a link to its page. Tests: `api_native_issues.rs`, `IssuePage.test.tsx`, `ExternalIssueDetailPanel.test.tsx`.
- GitHub issues: the owner's 2026-10-29 requirement ("a user shouldn't be forced to the remote website to do operations that are supported by the issue adapter") and approval of the proposal in the [#808 chat intent](../../chat_intent/issue-808-g5d-issue-edit/request.md#later-instruction-2026-10-29-external-issues-edited-in-the-platform); the field comparison, kept review step and labels on the tracker are the proposal's defaults ([design](../../pr_ontology_diff_design/issue-808-g5d-issue-edit/ontology-plan.md#increment-2-github-issues-edited-on-their-g5-page-stateless-issue-comments)). Implemented: `/api/v1/orgs/:orgId/issues/github/:repositoryId/:number` `write-target`, `PATCH`, `state`, `comments` and `comments/:commentId`, each through `github_page_target` and the operations the agent uses; the page's GitHub controls and composer in `IssuePageControls.tsx` and `ActivityCommentComposer.tsx`. Tests: `a_person_writes_a_github_issue_from_its_page_as_themselves` (GitHub mock), `IssuePage.test.tsx` ("IssuePage — GitHub issue writes"). The link back to the primary workspace is the owner's request of 2026-10-30 ("add a 'back to workspace' button if the issue is the primary issue for a workspace"), tested in `IssuePage.test.tsx`. No browser run against the live stack or live GitHub.

### g5.issue.native.migration

- human-verified
- Source: [issue #681](https://github.com/G5Labs/G5-Lean/issues/681) ("Plan this from the start so connecting later isn't a dead end"); the owner approved coexistence plus export, with the move tool later, in [chat](../../chat_intent/issue-681-native-g5-issue-tracker/request.md) (question `migration`).
- Implemented: `GET /api/v1/orgs/:orgId/issue-domains/:domainId/export` (admin; streamed by `NativeExport::write`, so a failure after the first byte ends the body early; an attachment whose bytes are gone, such as one removed during the export, is exported with `contentBase64: null`), Export button in issue settings. The move tool is not built.

### g5.issue.labels

- human-provided
- user-facing-behavior
- test-coverage
- Source: the owner's [issue #1055](https://github.com/G5Labs/G5-Lean/issues/1055) ("Scope"), which reverses the native scope's exclusion of labels. [Design](../../pr_ontology_diff_design/issue-labels/ontology-plan.md).

### g5.issue.labels.contract

- human-provided, agent
- architecture-decision
- test-coverage
- Expands: [labels](#g5.issue.labels).
- Source: [issue #1055](https://github.com/G5Labs/G5-Lean/issues/1055) ("Boundary and operations"); the operations, deltas, central validation, pagination and reconciliation are the owner's, and the limits (50-character names, 100-character descriptions, 20 labels a change, 100 a page, matching GitHub's) and the combined-edit order are agent choices in the [design](../../pr_ontology_diff_design/issue-labels/ontology-plan.md#design).
- Implemented: shapes and central validation in `services/git/issue_labels.rs`; the shared operations `list_labels_as`, `create_label_as`, `edit_label_as`, `delete_label_as` and `change_issue_labels_as` in `routes/issue_labels.rs`, called by the people's routes (`/api/v1/orgs/:orgId/issue-domains/:domainId/labels[/:labelId]`, `/issues/{github/:repositoryId,g5}/:number/labels`) and the agent's (`/api/internal/workspace-agent/:id/issue-labels[/edit|/delete]`, `issue-edit` with `addLabels`/`removeLabels`, `issues` with `labels`). Tests: `issue_labels` unit tests; `api_native_issue_labels.rs`; the GitHub tests named under [g5.issue.labels.github](#g5.issue.labels.github).

### g5.issue.labels.native

- human-provided, agent
- architecture-decision
- security
- test-coverage
- Expands: [labels](#g5.issue.labels).
- Source: [issue #1055](https://github.com/G5Labs/G5-Lean/issues/1055) ("Ownership, transactions and failures"); the store, keys, transactions, events, deletion and export are the owner's, and recording removals when a catalog label is deleted and the labels-before-issue lock order are agent decisions in the [design](../../pr_ontology_diff_design/issue-labels/ontology-plan.md#design).
- Implemented: migration `146_native_issue_labels.sql`; `services/git/native_issues/labels.rs` (catalog, `change_issue_labels`, locking), creation, reads, `label:`/`no:label` search, Activity events and export in `native_issues.rs` and `native_issue_query.rs`. Tests (`api_native_issue_labels.rs`, no GitHub App): `a_g5_domain_keeps_a_label_catalog`, `g5_issues_carry_labels_through_creation_tagging_search_and_export`, `g5_labels_stay_in_their_organization_and_viewers_only_read`, `the_workspace_agent_keeps_labels_as_its_owner`, `racing_label_changes_and_deletions_keep_every_other_change`.

### g5.issue.labels.github

- human-provided, agent
- architecture-decision
- security
- test-coverage
- Expands: [labels](#g5.issue.labels).
- Source: [issue #1055](https://github.com/G5Labs/G5-Lean/issues/1055) ("Ownership, transactions and failures"), which asked to confirm GitHub's behaviour; the authority, guards, partial outcomes and creation drops are the owner's, and the existence check before an add, the stop-at-first-failure sequence and the absent-label reading of a 404 follow from GitHub's REST label API as recorded in the [design](../../pr_ontology_diff_design/issue-labels/ontology-plan.md#design). Live GitHub is not exercised.
- Implemented: `g5-scm` `providers/issues/labels.rs` (catalog pages, by-name reads, label writes, issue label add/remove, no redirects) and `create_issue` with `labels`. Tests: `g5-scm` `issue_labels_read_the_catalog_and_change_labels_by_name` (mocked transport: pagination, 404 as absent, 301 refused, 403/502 classified); `api_external_issues.rs` `the_workspace_agent_changes_github_labels_as_its_owner` and `github_issues_take_labels_at_creation_and_from_their_page` (GitHub mock).

### g5.issue.labels.surfaces

- human-provided, agent
- user-facing-behavior
- test-coverage
- Expands: [labels](#g5.issue.labels).
- Source: [issue #1055](https://github.com/G5Labs/G5-Lean/issues/1055) ("Experience and acceptance"); `search --label` and `delete --yes` are agent additions matching gh.
- Implemented (CLI): `g5d issue label` in `cli/issue_label.rs`; `--add-label/--remove-label`, `create --label`, `search --label/--no-label` in `cli/issue.rs`; agent guidance in `.agents/AGENTS.md`. Tests: `g5d` `label_commands_send_gh_style_requests_and_delete_needs_yes`, `label_lists_and_changes_print_what_happened` and the extended `issue` parsing tests.
