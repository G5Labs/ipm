# Attachments of remote issues and pull requests (G5-Lean#1013)

Governing blocks: [g5.attachments](../../ontology/features/issues.g5.md#g5.attachments), [g5.attachments.github](../../ontology/features/issues.g5.md#g5.attachments.github), [g5.attachments.cli](../../ontology/features/issues.g5.md#g5.attachments.cli), [g5.issue.native.attachments](../../ontology/features/issues.g5.md#g5.issue.native.attachments), within [g5.external-issues.cli.reads](../../ontology/features/issues.g5.md#g5.external-issues.cli.reads) and the [issue adapter seam](../../ontology/features/issues.g5.md#g5.issue.native.adapter); for pull requests, `g5.git.pr-adapter` on the [G5-Lean#830](https://github.com/G5Labs/G5-Lean/issues/830) branch. Intent: [chat](../../chat_intent/issue-1013-issue-images/request.md).

## Problem

`g5d issue view` returns Markdown whose private uploads (`github.com/user-attachments/...`) answer 404 without a GitHub browser session. The VM has no GitHub credential and must not get one, so the agent cannot see mocks, recordings or logs it is asked to use. Pull requests carry the same uploads. G5's own issues could not take images at all ([G5-Lean#726](https://github.com/G5Labs/G5-Lean/issues/726)). The first increment, `g5d issue images`, solved only GitHub images on issues; the owner asked for one abstraction across kinds and item families, inside the adapter model, with a local adapter built in the same PR.

## What the providers do

- **GitHub** has no attachment API. Uploads are links in an item's texts: `user-attachments/assets/<uuid>` for images and videos, `user-attachments/files/<id>/<name>` for other files (PDF, text, logs, archives, Office, audio). Since [May 2023](https://github.blog/changelog/2023-05-08-more-secure-private-attachments) private-repository images and videos need sign-in. The REST `full+json` (or GraphQL `bodyHTML`) rendering replaces an image's link with `private-user-images.githubusercontent.com/...?jwt=`, which loads without a credential for 300 s; installation and GitHub App user (`ghu_`) tokens get 404 on the raw link ([experiment](https://github.com/AdvanGrid/beghu/issues/62)). File links stay unsigned in the rendering and are served only to a classic user token or a browser session ([report](https://github.com/cbusillo/codex-skills/issues/2021), [discussion](https://github.com/orgs/community/discussions/162417)). Upload limits: images 10 MB, videos 10 MB (free) or 100 MB (paid), other files 25 MB ([docs](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/attaching-files)). GitHub's API cannot upload attachments either. Not verified: that videos are signed like images; Enterprise Server's hosts.
- **Jira Cloud** keeps attachments on the issue (`fields.attachment`: id, filename, mimeType, size, author, created), fetched with `GET /rest/api/3/attachment/content/{id}` under the site credential (`redirect=false` returns the bytes; otherwise a 303 to Atlassian's media host). Description and comments reference files by ADF media UUIDs that are not attachment ids, so a file's place is the issue, not a comment.
- **G5 issues** had none: attachments were retired with the old local issues (decision 14). The owner reversed that for images in this PR (2026-11-01).

## Adapter alignment

The issue service and the pull-request service each own admission, item selection and the provider-neutral record; each sends two content operations to the adapter serving the item:

| Adapter | `list attachments` | `fetch attachment` |
|---|---|---|
| GitHub (issues and PRs) | `<img>`, `<video>` and upload links in GitHub's rendering of the item's texts | signed or public link, no credential, GitHub's hosts only |
| G5 issues (native) | the issue's uploaded images, places derived from its texts | from G5's own store |
| Local repository PR | refused like its conversation (description is G5's own) | n/a |
| Jira Cloud (planned) | the issue's attachment field | by id under the site credential |

The GitHub fetch mechanism lives with GitHub's provider client in `g5-scm` (credential-free fetch with checked redirects, content checks, per-kind limits), so the GitHub issue adapter and the GitHub PR adapter call the same operations with their own grant: the issue domain's installation token narrowed to Issues read, or the pull request's read credential. Callers never reconstruct admission or provider rules, and no surface branches on the provider.

The record: place (item, description, comment, review or review comment, with author and time), kind (image, video, file), name or alt, type and size when the adapter knows them, retrievable or the reason, and an opaque handle. GitHub's handle is text + position + text version (stateless; a fetch re-reads the text and refuses a changed one); Jira's and G5's would be the attachment id. Bytes stream through G5, since a video can be 100 MiB. At most 50 attachments per listing (truncation reported); each GitHub fetch costs one read of its text.

## Pull requests

Built 2026-11-03 at the owner's request, after #830 landed ([intent](../../chat_intent/issue-1013-issue-images/request.md#pull-request-attachments-and-agent-uploads-2026-11-03)); governs [g5.attachments.cli](../../ontology/features/issues.g5.md#g5.attachments.cli) and the operation list of [g5.git.pr-adapter](../../ontology/software_architecture/git.g5.md#g5.git.pr-adapter).

- **Owner and admission.** The pull-request service selects and admits exactly as for `g5d pr view NUMBER -c` (selection, readable repositories, the owner's `ReadPullRequests` credential) and sends list and fetch to the GitHub adapter. A G5 PR name, or a number in a repository not on GitHub, is refused before any read with the same reason as conversation reads (`g5.git.pr-local-conversation`): the local adapter would have nothing to list, and the description of a G5 PR is G5's own. Rejected: expanding a G5 PR name to its GitHub repository PRs, which would make one command span several repositories' texts while `view -c` does not.
- **Texts.** GitHub's REST `full+json` rendering of the description (the existing description read), conversation comments, reviews and review comments, first page each (100), with `-c`; without it only the description. Reviews and review comments are read over REST, not the GraphQL review connection, so every text has `body_html` from the same renderer the issue path relies on.
- **Handles** extend the issue handle with the PR's kinds: `description:P:V`, `comment:ID:P:V`, `review:ID:P:V`, `review-comment:ID:P:V`. A fetch re-reads that one text (and checks it belongs to the PR) and refuses a changed version, as for issues.
- **Shared mechanism.** `g5-scm`'s rendering parser and credential-free `open_attachment` serve both families; opening now also accepts a pull-request read grant, since it sends no credential.
- **CLI.** One implementation for both commands; the PR header names the PR (`owner/name#N`) and places include `review` and `review comment` (with the file path).

## Agent uploads

Built 2026-11-03 at the owner's request; governs [g5.attachments.cli.attach](../../ontology/features/issues.g5.md#g5.attachments.cli.attach).

- **One server operation:** the agent's upload (`POST …/issues/attachments`) chooses the issue as the agent's other writes do (`agent_write_target`), refuses a GitHub issue before decoding the bytes, and adds the image through the native module with origin `agent`. The key is derived on the server from the issue, the file name and the content digest, so the CLI carries no upload keys and retries never duplicate. Rejected: inline attachments in the create and comment requests, which would make those routes carry up to tens of MiB and duplicate the upload rules in each.
- **Sequencing in `g5d`**, as the composers and creation form already do: `comment` and `edit` upload, then send the text with the images' Markdown appended. `create` asks the server to refuse a GitHub destination before creating (`attachments: true`), creates, uploads, then edits the description with `expectedBody` set to what it created, so a concurrent person's edit is never overwritten. A refused or unanswered edit is settled by reading the description again: already showing the images is success, still the created one means a rerun with the same idempotency key finishes (the server's create replay accepts only the description it created), and anything else is the concurrent edit.
- **Bounds** are the native ones: the CLI refuses a non-regular file or one over 10 MiB before reading it, and at most 10 files per command, which keeps each request bounded; the server recognizes the type.

## Local adapter

[G5-Lean#726](https://github.com/G5Labs/G5-Lean/issues/726) asks for images in G5 issues. Recommended on 2026-11-01, accepted and built in this PR:

- **Store:** the record in a native attachments table in Postgres under tenant row-level security, beside the native issue tables; the bytes in the [attachment store](#attachment-store) (amended 2026-11-03; the first build kept them inline). Rejected: reusing `chat_attachments` (owned by a chat session, with its scan and mount lifecycle and 5 MiB limit); the docs Git repository (a workspace's lifecycle).
- **Attachments belong to the issue**, as on Jira, recording uploader, origin (person or agent), time, file name, type recognized from content, size and digest. Text embeds one by its G5 URL; the listing derives where each is referenced from the description and comments rather than storing a place, and an unreferenced upload is listed at the issue, so nothing is orphaned.
- **Upload** from the issue page's composers and the creation form (paste, drop, button) before the text is posted, which fixes #726; the creation form creates the issue, uploads, then edits the description. Uploads never move the issue's edit version, so that edit is not refused. Agent uploads (`--attach`) are [below](#agent-uploads). Each upload carries the caller's idempotency key, exactly-once like other [native writes](../../ontology/features/issues.g5.md#g5.issue.native.writes).
- **Bounds:** images only at first (PNG, JPEG, GIF, WebP by content, 10 MiB, matching GitHub), and a per-issue count; other files and video are a later decision.
- **Serving:** the page reads them through an authenticated route that sends images inline with `nosniff` and anything else as a download; SVG is refused. The agent uses the same adapter operations as for GitHub.
- **Removal:** the uploader or an administrator deletes an attachment, which removes its bytes; references then show it as removed. Organization deletion, when it exists, removes all.
- **Consequences:** the native tracker's retirement of attachments (decision 14) is reversed for images. The native export includes them with their bytes, and the planned move tool cannot upload them to GitHub (its API accepts no uploads), so a moved issue links back to G5.

The same GitHub fetch could also back a browser route that shows private GitHub images in the issue timeline ([G5-Lean#992](https://github.com/G5Labs/G5-Lean/issues/992)) under the reading person's authority; that is a separate change.

## Attachment store

Amended 2026-11-03 at the owner's [direction](../../chat_intent/issue-1013-issue-images/request.md#attachment-store-2026-11-03); governs [g5.issue.native.attachment-store](../../ontology/features/issues.g5.md#g5.issue.native.attachment-store).

- **Why not Postgres:** bytes inflate the database, its backups, point-in-time recovery and restores, and a read loads the whole value. Its one advantage, the SQL backup boundary, the owner set aside: Filestore backups are being added (#977), and losing Git would be the larger loss.
- **Owner and interface.** `g5_storage::AttachmentStore` is byte-level: write a new object, stream one, delete one, by `(organization id, random id)`. Nothing returns a path, so a bucket backend is one more implementation and a configuration switch: the names are already object keys under per-organization prefixes, and a copy keeps them. The existing review-artifact store is the counter-example: it hands out a directory and callers use the filesystem. The tenancy check sits above any backend, in the native attachments module, which alone holds the store: it builds a name only for an organization the caller's tenant transaction is scoped to (`ScopedDb::org_ids`). Ids are typed, so no request text reaches a name. `RemovedAttachment`, produced only by a removal, is the one way to delete bytes, after commit.
- **Ordering.** An upload writes the bytes (temporary file, sync, rename) before inserting the record, and drops them if the insert loses an idempotency race or fails; a removal deletes the record, commits, then deletes the bytes. A record therefore never names missing bytes. A crash or failed commit between the steps leaves unnamed bytes, which nothing can read; no sweep is built (the owner: no machinery before it is needed).
- **Mounts.** Same Filestore share, two `subPath` mounts: `attachments` for the API only, `repos` (the tenant prefix) for the API and daemon at the unchanged repository base, so repository configuration and the CI boot check stay the same. The root-run init container mounts the share root to create both directories with the service user's ownership before the subPaths are mounted. A separate Filestore instance was rejected for now: about double the cost for separate capacity and snapshots, and the subdirectory can move to one later.
- **Configuration.** `G5_ATTACHMENT_STORE_DIR`; unset, the local provider uses `.attachments` under its base directory (development, tests, self-hosted) and the Filestore provider has no store, so attachment operations answer unavailable instead of writing to a container's disk. The Helm chart sets it and refuses an API without it.
- **Erasure.** G5 has no organization deletion yet; when it does, it removes `<root>/<org>/` (or the bucket prefix). Native issues cannot be deleted either.
- **Export** reads each attachment through the same check and still embeds its bytes.

## Alternatives rejected

- Per-kind commands (`images`, `videos`): each new kind would need a new name and route.
- A fetch-any-URL command: a private link cannot be fetched without the rendering of the text holding it, and fetching caller-supplied URLs from the host would make it a general fetcher.
- Handing signed links to the VM: they would land in the persisted transcript, the VM's network would need the content hosts, and bounds would become the client's.
- Fetching GitHub file uploads under the owner's connected account: G5 holds GitHub App user tokens, which GitHub refuses for these links, and asking for classic tokens would widen what G5 holds.

## CLI

`g5d issue attachments [REF] [-c] [--dir DIR] [--list]` and `g5d pr attachments [PR] [-R OWNER/NAME] [-c] [--dir DIR] [--list]`, sharing one implementation; `--attach FILE` on `g5d issue create`, `comment` and `edit` ([agent uploads](#agent-uploads)). Files are new (never overwritten) and named for their place; each is printed with where it appears. Exit 1 when any listed attachment was not retrieved, after saving the rest. `view` points at the command when the item's text carries attachments; the agent instructions say to compare against the files or report that the comparison cannot be made.

## Acceptance (this PR)

- A member pastes, drops or chooses an image on a G5 issue's page or creation form and it shows in the text; a retry does not duplicate it; non-images, oversized images and a 51st attachment are refused; the uploader or an administrator removes it; another organization sees none; the export carries it; the agent lists and fetches it.

- The two private mocks of [#1010](https://github.com/G5Labs/G5-Lean/issues/1010) reach the agent as files without a chat upload (live; mocked until deployed).
- A video is saved; an oversized one, an unrecognized image or video, a foreign host or a redirect off GitHub's hosts is refused; no request to a content host carries a credential.
- A private file upload is listed with GitHub's limitation and fails the command; a public one is saved.
- An issue outside the domains, another workspace's credential, and a changed text are refused; a G5 issue without uploads lists none.
- `g5d pr attachments` lists a GitHub PR's attachments by text (description; with `-c` comments, reviews, review comments) and saves them; a changed text and a G5 PR name are refused; no content request carries a credential.
- `--attach` adds images to a G5 issue as the agent's, once per file and issue; a GitHub issue or GitHub creation destination is refused before anything is sent; `create` shows the images only in the description it created.
