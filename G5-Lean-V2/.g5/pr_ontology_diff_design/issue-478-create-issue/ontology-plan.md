# Create Issue from Home, the rail and `g5d issue` (issue #478)

> Ontology rebase note: links to current owners were retargeted. Immutable main-commit links preserve the historical ontology/source context of this record; they are not additional current contracts.


Intent: [request and the owner's 34 answers](../../chat_intent/issue-478-create-issue/request.md). Comparison base: `main` at `ad66367f`.

## Ontology changes

| Block | Change | Why |
| --- | --- | --- |
| [`g5.external-issues.creation`](../../ontology/features/issues.g5.md#g5.external-issues.creation) | New | Who creates what, and where: any member or the agent; title, description, assignees; the default target only; GitHub only. |
| [`g5.external-issues.creation.admission`](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission) | New | Creation passes the comment guards: G5 comments switch, non-public repository, secret screen, the author's own account, and the review step for untrusted domains. |
| [`g5.external-issues.creation.recovery`](../../ontology/features/issues.g5.md#g5.external-issues.creation.recovery) | New | Statelessly: a key-derived marker is looked for before every send and on checks; there is no creation record. |
| [`g5.external-issues.cli`](../../ontology/features/issues.g5.md#g5.external-issues.cli) | New | `g5d issue create` (gh flags), `link` and the `view` alias. |
| [`g5.external-issues.publication.automation`](../../ontology/features/issues.g5.md#g5.external-issues.creation) | Amended | The agent may write its own text, comments and issues, as the owner; only comments record the agent origin. This supersedes the design's "drafts only". |
| [`g5.external-issues.broker`](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission) | Amended | The shared guarded publication path is the broker's first step; status changes still wait for the full broker. |
| [`g5.story.track-workspace-issues`](../../ontology/features/issues.g5.md#g5.workbench.issues.create), [`g5.story.create-issue-from-home`](../../ontology/features/issues.g5.md#g5.workbench.issues.create) | Amended, new | Quick-add joins the rail story; Home creation is a new story. |
| [`g5.workbench.issues`](../../ontology/features/issues.g5.md#g5.workbench.issues.create) | Amended | Quick-add is enabled, creating a related issue. Done/Reopen stay disabled. |
| [`g5.workbench.issues.create`](../../ontology/features/issues.g5.md#g5.workbench.issues.create) | New | Home's Create button, the dialog, the issue page and Create & Fork. |
| [`g5.workbench.meeting`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/workbench.g5.md#review-a-meeting-through-a-question-set), [`g5.workbench.deferred`](../../ontology/features/issues.g5.md#g5.workbench.issues.create) | Amended | Meeting cards create issues through the dialog; quick-add leaves the deferral list. |
| `g5.scope.internal-issues` (metadata) | Amended | The new `g5d issue` commands are external-issue features, not tracker residue. |

The diff is this branch's first commit touching `.g5/ontology/` (`git log -p -- .g5/ontology` on the branch).

## Governing ontology

- [`g5.external-issues.publication`](../../ontology/features/issues.g5.md#g5.issue.comment-authority) and [`.payload`](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission). The non-public check runs at send time. A changed destination needs a new review.
- [`g5.external-issues.setup.recovery`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/external_issues.g5.md#reconcile-uncertain-external-creation). G5 never automatically creates another issue while a creation is uncertain.
- [`g5.external-issues.association.primary`](../../ontology/features/issues.g5.md#g5.issue.primary) and [`.fork`](../../ontology/features/issues.g5.md#g5.issue.fork-primary-race). Create & Fork claims primary through the existing fork link, so its conflict fallback is unchanged.
- [`g5.external-issues.completion`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/external_issues.g5.md#derive-completion-from-the-work-actually-accepted). Creation does not lift the Done/Reopen deferral.

## High-level design

```text
Home Create ─────┐                          ┌─ list the author's issues (installation token):
Rail quick-add ──┼─ POST /orgs/:org/issues ─┐│   marker found → return that issue
                 │                          ├┤
g5d issue create ─ POST /internal/.../issues┘└─ else POST /repositories/{id}/issues (author's token)
                    create_tracker_issue: guards → look → send
```

**One admission function, shared guards.** `create_tracker_issue` sits beside `publish_primary_issue_comment` and reuses its guards:

- the organization switch
- the domain's `comments_enabled`
- `domain_repository_audience`
- `acting_user`
- the secret screen

Only the destination differs: the organization's `is_default_create_target` domain, not a primary issue. It runs this order:

1. Validate the title, body, assignees and key, and resolve `@me` to the author's login.
2. Read the target and its switch, then check the audience on GitHub and resolve the author's grant.
3. Look for the submission's marker. If it is found, return that issue.
4. Otherwise send the request. Created returns the issue; refused returns the typed error; no answer returns `outcome_unknown` with the key.

No transaction stays open across a GitHub request.

**No creation record.** Per the owner's choice of 2026-10-13, G5 stores nothing for a creation. The marker is `<!-- g5-issue <token> -->`, where the token is a SHA-256 digest of organization, author and idempotency key, so a retry with the same key yields the same marker. Agent keys are namespaced `agent:` as for comments.

GitHub's issue search may not index HTML comments, so the lookup lists the target's issues instead: `creator = <login>`, `since` 24 hours ago, newest first, at most 10 pages of 100, read with the installation token. It matches the marker ending the body.

What this gives up:

- A retry with the same key sends again if GitHub has not yet listed the first issue.
- An unconfirmed creation is shown only by the dialog or command that made it.
- Agent origin is not recorded for creations; the issue shows the owner's account.

**API.**

- `GET /orgs/:org/issues/create-target` gives the dialog and the disabled reasons. It returns:
  - `state`: `ready`, `no_target`, `comments_disabled`, `public_destination`, `visibility_unknown`, `connection_required` or `unavailable`
  - the destination: domain, display key and `contentTrusted`
  - the person's GitHub login
  - the repository's assignable users
- `POST /orgs/:org/issues` creates the issue. It requires `workspaces:write` in the organization and binds the request to the `domainId` the person saw. With `checkOnly: true` it only looks for the marker and reports `created` or `not_found`.
- Internally, `POST /internal/workspaces/:id/issues` creates as the incarnation owner. `POST /internal/workspaces/:id/issue-links` links through `create_workspace_link`, also as the owner.

**Frontend.**

- `CreateIssueDialog` holds the title, Markdown description and assignee picker. It shows "Creating in `acme/web` as `@login`". Untrusted domains get a review step with the exact payload. An unknown outcome keeps the form, and offers Check again (look only) and Create anyway (same key, so it looks first).
- Home's tile shows Create (alignment layout only), disabled with the target's reason. Create navigates to the new `/:org/issues/github/:repositoryId/:number` page. That page renders `ExternalIssueDetailPanel` and offers Start workspace while no workspace owns the issue.
- Create & Fork closes the dialog and opens `CreateWorkspaceDialog` with `initialIssueRef` set to the new issue's display key. That dialog already defaults the issue to primary and offers "Start work".
- In the rail, quick-add opens the same dialog without Create & Fork, then links the new issue as related through `useLinkWorkspaceIssue`. It is offered only to a viewer who can link issues to the workspace (its owner, not on a review copy).

**CLI.** `g5d issue create` takes `-t/--title`, `-b/--body` or `-F/--body-file`, `-a/--assignee` (repeatable or comma-separated, gh style) and `--idempotency-key`. Title and body are required, as `gh` requires them without a terminal; it never prompts. On success it prints the URL; `--json` prints the full result. Exit codes:

| Exit | When |
| --- | --- |
| 1 | Failed |
| 7 | Outcome unknown; prints "rerun with `--idempotency-key K`" (the rerun looks before sending), matching `g5d pr comment` |
| 2 | A refusal with its `code`, the existing validation mapping |

`g5d issue link <ref> [--primary]` links an issue, as related by default; `view` aliases `show`. New calls go through `InternalApiClient` methods, as `g5d question` does, not the raw `reqwest` helper in `issue.rs`.

## Decision notes

- **Source workspace for Create & Fork.** This follows "Same as Start workspace today": it forks the organization's base workspace through the existing dialog. If the owner meant a source picker, it belongs to Start workspace too and is a separate change.
- **Meeting decisions.** The contract is recorded, but the cards are not built (`g5.workbench.meeting` 0%). `CreateIssueDialog` takes an initial title and body so the cards can reuse it.
- **Assignees.** Logins are checked for shape (at most 10, `@me` expanded). GitHub decides who can be assigned: it rejects unknown users (`failed`, with GitHub's message) and drops assignees it does not permit.
- **Destination binding.** The person route takes `domainId`. If the default target changed, the request is refused with `destination_changed`, as comments are. The agent route always uses the current target.
- **Residual races.** As with comments, visibility can change between G5's check and GitHub's write. A switch turned off during the GitHub check does not stop a send already admitted.

## Acceptance conditions

- A person creates an issue in an approved non-public target under their own token. The payload carries the key's marker, and the response gives the issue's number and URL.
- Each guard refuses with nothing sent: switch off, external issues off, public repository, unknown visibility, no grant, secret in the title or body, no target, changed destination.
- A 5xx or timeout reports `outcome_unknown`. A check finds the marker and reports the issue without sending; with no marker, a check sends nothing. A retry with the key after the first issue is listed returns it without sending.
- The agent creates as the owner (no origin is recorded; the issue shows the owner's account); `@me` becomes the owner's login. Links made through `g5d issue link` respect the primary bounds.
- The UI covers:
  - Create is disabled with each reason
  - the review step appears only for untrusted domains
  - Create opens the issue page
  - Create & Fork opens the fork dialog with the issue preselected as primary
  - quick-add links the issue as related, and a failed link is reported while the issue is kept

## Follow-up: a parent issue

Intent: the owner's instruction of 2026-10-13 superseding the parent-issue answer, in the [intent record](../../chat_intent/issue-478-create-issue/request.md#parent-issue). It covers the small part only: a parent on creation, not the fork dialog's "Create sub-issue".

### Ontology changes

| Block | Change | Why |
| --- | --- | --- |
| [`g5.external-issues.creation.parent`](../../ontology/features/issues.g5.md#g5.external-issues.creation.parent) | New | A parent in the creation target, the order (create, then attach), read-before-attach on retries, and never moving an issue. |
| [`g5.external-issues.creation`](../../ontology/features/issues.g5.md#g5.external-issues.creation) | Amended | "No parent" is replaced by an optional parent. |
| [`g5.external-issues.cli`](../../ontology/features/issues.g5.md#g5.external-issues.cli) | Amended | `create --parent`. |
| [`g5.story.create-issue-from-home`](../../ontology/features/issues.g5.md#g5.workbench.issues.create), [`g5.workbench.issues.create`](../../ontology/features/issues.g5.md#g5.workbench.issues.create) | Amended | The dialog's parent field and its notice when the issue is not under the parent. |
| [`g5.workbench.issues`](../../ontology/features/issues.g5.md#g5.workbench.issues.create) | Amended | Quick-add starts with the primary issue as parent when it is in the creation target (the owner's follow-up instruction). |

### Governing ontology

- [`g5.external-issues.creation.recovery`](../../ontology/features/issues.g5.md#g5.external-issues.creation.recovery). The stateless marker check carries the parent too: a retry finds the issue, so retrying the attach never creates a second issue. No creation record is added.
- [`g5.external-issues.creation.admission`](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission). Its guards admit the destination. Attaching writes to the parent issue, so the parent must be in that same destination.
- [`g5.external-issues.association.identities`](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/external_issues.g5.md#keep-external-hierarchies-and-roles-distinct). The sub-issue relationship lives in GitHub only. It does not link the issue to a workspace or change any workspace's parentage.

### High-level design

```text
create_tracker_issue: guards → resolve parent → look for marker → send if none
                                                       │
                       issue (found or sent) ──────────┴─ settle_parent
   sent now:  POST /repositories/{id}/issues/{parent}/sub_issues {sub_issue_id}
   found:     GET  .../issues/{n}/parent → same parent: attached
                                         → other parent: other_parent (never replaced)
                                         → none: attach (a check only reports not_attached)
```

- **Resolve first.** `#N` or `N` names an issue in the target. Anything else goes through `resolve_reference` limited to the target domain. A pull request, missing issue, unparseable reference or issue elsewhere is a 422 with `field: "parent"`, and nothing is sent. GitHub being unavailable stays the usual typed error.
- **Attach.** The attach uses the author's user token, as creation does. The read uses the installation token, as the marker lookup does. The attach never sets `replace_parent`, so GitHub refuses rather than moving an issue.
- **Result.** `IssueCreationDTO.parent` is `null` without a parent. Otherwise its `state` is one of:
  - `pending`: the issue was not created;
  - `attached`;
  - `not_attached`: a check found the issue without a parent, or GitHub refused, with its message;
  - `outcome_unknown`: no answer from GitHub, or the issue's parent could not be read;
  - `other_parent`.

  The issue's own `state` stays `created`, and the HTTP status is unchanged.
- **Dialog.** A Parent issue field with a debounced lookup (the fork dialog's pattern). Create is disabled while the parent is unresolved or outside the destination. The parent is part of the submission's fingerprint, so editing it means a new key. The review step shows the parent. If the issue is created but not under the parent, a notice replaces the form, offering Retry adding parent (same request, same key) when the state can still change, and Continue without parent (the normal success path). Rail quick-add passes the primary issue as `initialParent` when its `locator.repositoryId` is the destination's, and nothing otherwise. The task is still linked as related, and its button's tooltip names the parent.
- **CLI.** `--parent ISSUE`. With a requested parent, the URL is printed whenever the issue exists. `outcome_unknown` exits 7 with the key; `not_attached` and `other_parent` exit 1.

### Decision notes

- **Same repository only.** GitHub allows a sub-issue in another repository of the same owner. A parent elsewhere would need that domain's own approval and visibility checks at send time, so it waits for a request.
- **Pre-fill.** Home starts empty (home-create-prefill). Quick-add starts with the primary issue, per the owner's follow-up. A primary issue in another repository is not offered, because it would only block Create.
- **Residual race.** As with creation, someone may attach the issue elsewhere between G5's read and its write. Without `replace_parent`, GitHub is expected to refuse (the issue already has a parent), which reports `not_attached` with GitHub's message rather than moving it. That refusal is not verified against live GitHub.
- **Unverified fact.** GitHub's documentation describes the sub-issue endpoints, but the tests use wiremock, not live GitHub. Whether GitHub refuses a second attach to the same parent, rather than accepting it, is unverified; reading the parent before every retried attach avoids depending on it.

### Acceptance conditions

- Each parent refusal sends nothing: not an issue, not found, outside the destination, invalid reference.
- A parent is attached with the author's token after creation. A replay finds the issue under it and sends nothing.
- A refused attach leaves the issue created and reported `not_attached`. A check only reads. A retry with the key attaches without creating again.
- An issue under another parent is reported `other_parent` and is not moved. No answer reports `outcome_unknown`.
- The agent creates a sub-issue by number. The CLI prints the URL and exits 0, 7 or 1 by parent state.
- The dialog covers:
  - the lookup and the outside-destination block
  - the review row
  - the notice with retry under one key and continue
  - the refusal shown at the field
  - a new key after editing the parent
