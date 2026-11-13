# Native G5 issue tracker

Intent: the owner's [issue #681](https://github.com/G5Labs/G5-Lean/issues/681) (and its
comment linking #682). Organizations without an external integration lose G5's issue
loop. Add a native "G5" provider behind the existing issue service, with issues stored in
Postgres and limited to the common envelope, so that every existing issue surface works.

Status: design decided. On 2026-10-22 the owner approved the recommended option for all four
questions ([chat intent](../../chat_intent/issue-681-native-g5-issue-tracker/request.md)).
Implemented on the workspace branch after the owner's go-ahead; see [Validation](#validation).
During implementation the owner added `g5d issue comment` on any readable issue
([chat intent](../../chat_intent/issue-681-native-g5-issue-tracker/request.md#later-instruction-2026-10-22-during-implementation)),
recorded in `g5.external-issues.cli`.

## Owner questions

[Q1. Reverse decision 14]

```g5-question-ref
{"path": "docs/8e8564d2-0481-43b7-b2f6-f339bd3d0feb/questions/native-issue-tracker.g5q.json", "id": "reverse-decision-14"}
```

[Q2. Availability beside external domains]

```g5-question-ref
{"path": "docs/8e8564d2-0481-43b7-b2f6-f339bd3d0feb/questions/native-issue-tracker.g5q.json", "id": "availability"}
```

[Q3. Connecting GitHub or Jira later]

```g5-question-ref
{"path": "docs/8e8564d2-0481-43b7-b2f6-f339bd3d0feb/questions/native-issue-tracker.g5q.json", "id": "migration"}
```

[Q4. Where native issues change]

```g5-question-ref
{"path": "docs/8e8564d2-0481-43b7-b2f6-f339bd3d0feb/questions/native-issue-tracker.g5q.json", "id": "native-page-actions"}
```

Answers:

1. Reverse decision 14 for the native tracker only.
2. Native coexists with external domains.
3. Coexistence plus an admin JSON export; the move tool comes later.
4. The issue page can comment, close/reopen and edit.

## Ontology changes

All in [features/issues.g5.md](../../ontology/features/issues.g5.md).

| Block | Change | Summary |
|---|---|---|
| `g5.issue.providers`, `.domains`, `.domain-policy`, `.identity`, `.live-read`, `.read-failure` | Amended | Name "its tracker" instead of GitHub; the native tracker is one domain. |
| `g5.issue.comment-identity`, `.comment-authority`, `.comment-recovery` | Amended | Identity is the G5 account on native issues; the audience and publication-record rules govern external publication. |
| `g5.external-issues.creation`, `.transition`, `.cli`, `g5.workbench.issues.create` | Amended | Native creation and state changes follow the native write rules; "creation repository" becomes "creation domain". |
| `g5.issue.native`, `.scope` | New I | The tracker and its scope guard, from the issue. |
| `g5.issue.native.adapter`, `.store`, `.domain`, `.writes`, `.search`, `.page` | New D | The decisions below. |
| `g5.issue.native.migration` | New D | Q3: coexistence plus export; the move tool comes later. |

`docs/external-issue-integration-design.md` decision 14 carries a superseding note.

## Governing ontology

- [`g5.issue.primary`](../../ontology/features/issues.g5.md#g5.issue.primary),
  [`g5.issue.base-links`](../../ontology/features/issues.g5.md#g5.issue.base-links),
  [`g5.issue.identity`](../../ontology/features/issues.g5.md#g5.issue.identity) and
  [`g5.issue.fork-primary-race`](../../ontology/features/issues.g5.md#g5.issue.fork-primary-race)
  are unchanged. Native issues reuse `workspace_issue_links` and its partial unique indexes
  as they are.
- [`g5.issue.platform-bug`](../../ontology/features/issues.g5.md#g5.issue.platform-bug) is
  separate and unchanged.
- [`g5.repo.durability`](../../ontology/features/repositories.g5.md#g5.repo.durability):
  native content is SQL. BackupBoundary already captures SQL and Git together.
- [ScmProvider ownership](../../ontology/software_architecture/git.g5.md) (line 281): GitHub
  issue protocol stays in the GitHub client. The native adapter is not a repository provider.
- [`g5.workspace.issue`](../../ontology/features/workspaces.g5.md#g5.workspace.issue): "the
  platform helps maintain this linkage". G5-managed closure stays deferred to #682. A native
  tracker removes the provider race that made it hard.

## Ownership and the adapter seam

Today there is no seam. Routes and `external_issues.rs` call `github_issues`/`g5-scm`
directly. Every handler starts with `github_client(&state)?`, and the instance feature
`external_issues` exists only when the GitHub App is configured.

```text
UI / g5d ──► issue routes ──► IssueService (provider-neutral)
                                 │  switch, caller authority, domain lookup,
                                 │  links + primary/Base rules, fork linking
                                 ▼
                       adapter for domain.provider
                     ┌───────────┴────────────┐
               GithubIssues                NativeIssues
     publication guards, user/installation   Postgres store,
     tokens, markers, comment ledger,        idempotency keys,
     search budget, no-redirect client       conditional edits, events
```

- **IssueService** owns what is true for any tracker: the organization switch, workspace
  and organization permissions, the review-copy refusal, domain configuration, link
  creation and release, the primary-claim invariants, and fork-time resolution and
  linking. It resolves the domain, then calls the adapter.
- **Adapters** own content operations and their protocol: `resolve`, `search`, `get`,
  `activity`, `create` (with parent), `comment`, `edit_comment`, `set_state`,
  `assignable_users`, plus native `edit` (Q4). Both adapters return the same issue shape. The
  shape already exists (`ExternalIssueSummaryDTO` and friends); `provider` gains `"g5"`,
  users gain an optional G5 user id and display name, and the GitHub-only `locator`
  becomes provider-tagged.
- **Publication guards move inside the GitHub adapter's writes.** These are
  `admit_person_write`'s comment switch, audience check and connected account; the secret
  screen; the marker-based recovery; `issue_comment_publications`; and the redirect rule.
  They exist because content leaves G5 for an audience G5 does not control. A native write
  does not leave G5, so applying them there would only add the "Connect GitHub"
  requirement this issue exists to remove.
- The adapter is chosen by `issue_domains.provider`. Enum dispatch is enough for two
  providers. Jira later adds a variant.

Rejected: a separate native issue service with its own routes and UI. The issue explicitly
asks for one code path. A second experience would duplicate links, Activity, fork and CLI
behavior and let them drift.

## Native store

New tables, each with `org_id`, the `set_org_id_from_session` trigger, `ENABLE`/`FORCE`
RLS and a `tenant_isolation` policy. There is no system-pool policy because no background
worker reads them.

- `native_issues`: `id bigint identity` (the stable `external_issue_id`, so the existing
  `^[1-9][0-9]*$` link and fork checks hold unchanged). It also holds `domain_id`,
  per-domain `number`, `title`, `body`, `state` (`open`/`closed`), `state_reason`,
  `parent_id` (same domain), `author_user_id`, `origin` (`person`/`agent`),
  `idempotency_key` (unique per `(org_id, author_user_id, idempotency_key)`), `created_at`,
  `updated_at` (the edit version), `closed_at`, `closed_by`, and a generated `tsvector`
  over title and body with a GIN index.
- `native_issue_assignees (issue_id, user_id)`: assignees must be organization members
  who can write.
- `native_issue_comments`: `id bigint identity`, `issue_id`, `author_user_id`, `origin`,
  `body`, timestamps, and an idempotency key unique per author.
- `native_issue_events`: append-only. Records `closed`, `reopened`, `assigned` and
  `unassigned` with actor and time.

Schema additions to shared tables: `issue_trackers.provider` and
`issue_domains.container_kind` checks gain `'g5'`/`'g5_project'`. The native tracker's
`instance_key` is `g5` and its URL columns hold no deployment address. A partial unique
index allows one native domain per organization. Its `display_key` is the issue key
(`ACME`, uppercase, fixed at creation), and its `external_container_id` is `1`.
The creating transaction locks the domain's `issue_domains` row `FOR UPDATE` and takes the
next number as `max(number) + 1` (no counter table). That serializes creation within an
organization, which is ample for human and agent rates.

**State and events.** `native_issues.state` is the current fact, used for filtering and
conditional updates. Events are history written in the same transaction when, and only
when, the state or assignees actually change. Deriving state from the latest event
instead would make every search filter a lateral join, for no added guarantee.

**URLs.** Summaries build the absolute URL (`<external_url>/<org slug>/issues/g5/<number>`)
when read, from the deployment's configured external URL and the organization's slug. A link
row keeps the URL it was made with, exactly as GitHub links keep theirs; surfaces open native
issues through the issue page route, so a stale stored URL is cosmetic. (Revised from
storing a path: the link and fork tables already treat `canonical_url` as routing metadata of
link time, and a second URL convention would have spread into every link consumer.)

**Retention.** Rows are deleted with the organization (FK cascade). Member deactivation or
removal keeps authorship, matching how chats and workspaces retain history. There is no
user-erasure contract in the ontology today; if one is added, native content falls under
it like other user-authored SQL content.

## Native writes

| Operation | Entry points (existing unless noted) | Rule |
|---|---|---|
| Create | Home Create / Create & Fork, rail quick-add, `g5d issue create` | One transaction: allocate the number, insert the issue, assignees and optional parent. An idempotency-key replay returns the first row; the same key with different content returns 409. |
| Comment | Activity composer on the primary issue, `g5d issue comment [REF]` (any readable issue), issue page (Q4) | Insert keyed by author and idempotency key. Response shape matches `CommentPublicationDTO` with state `published`, `provider: "g5"` and the comment itself. |
| Edit comment | Activity (author only) | `UPDATE … WHERE id = $1 AND author = $me AND updated_at = $seen`. Zero rows means "changed since you opened it". |
| Close / reopen | Rail Done box, issue page | `UPDATE … SET state = $s WHERE state <> $s RETURNING`, plus an event only on a returned row. Repeating it is harmless. The agent has no transition. |
| Edit title, description, assignees | Issue page (Q4) | Conditional on `updated_at`, like comment edits. Assignment changes append events. |
| Link / unlink / Create & Fork | unchanged | IssueService, provider-neutral. |

Authority: organization members with write permission, matching today's `comment_db` and
creation rules; viewers read. The agent acts as the workspace owner with `origin = agent`,
under the existing incarnation fence. Agent idempotency keys keep their `agent:` prefix.

Not applied to native writes: secret screening, rendered-payload confirmation, hidden
markers, the domain's "G5 comments" switch and visibility checks. Native content stays in
the organization like chats and docs, which G5 does not screen either. The domain's
`content_trusted` still controls Markdown rendering and the agent's "not trusted" label.

Not in scope: notifications (G5 sends none), G5-derived state changes (#682), labels,
templates, reactions, comment deletion and issue deletion.

## Reads, search and references

- **Reads and Activity.** The native adapter reads the issue, comments (oldest first) and
  events from Postgres under the caller's tenant session. Activity entries use
  `g5-issue:`/`g5-comment:`/`g5-event:` keys and a `g5` source. A database failure is
  reported as the issue error, as a GitHub failure is today.
- **Search.** `websearch_to_tsquery` over title and body, plus an exact match on
  `KEY-n`/number. Filters cover state and assignee (`@me` or a member's email). The native
  domain joins federated search and merges results with the existing completeness and
  truncation rules. The agent budget's per-organization share divides by the number of
  external domains only, and a native-only search reserves nothing.
- **References.** The native adapter accepts `KEY-n`, its G5 issue-page URL, and bare `n`
  or `#n` when it is the creation domain, under the existing "linked first, then creation
  domain, ambiguity reported" order. `-R KEY` names the native domain the way `-R owner/name`
  names a GitHub one. `KEY-n` cannot collide with GitHub's `owner/name#n`.
- **Users.** The issue user shape gives `login` as the member's email for gh-compatible
  `--json` (`assignees[].login`), and `displayName`/`avatarUrl` for the UI.
  `g5d issue create -a` accepts an email or `@me`.

## Settings and gating

- The instance feature that gates issue surfaces (`useExternalIssuesFeature`,
  `instance.rs`) becomes `issues`, always offered. `github_app` keeps meaning GitHub
  domains can be added. `github_client(&state)?` moves from the top of every handler into
  the GitHub adapter, so a deployment without the App serves native issues.
- The organization switch keeps its stored key `external_issues_enabled` (no data
  migration) and is relabeled "Issues". It and `IssueDomainSettingsSection` also appear in
  an Issues section of the organization's Admin page, reachable without the App (the GitHub
  App page keeps them too). That section offers
  "Use G5 issues" (key input, default from the organization slug), and GitHub
  repositories when the App is bound. Default creation domain, content trust and removal
  apply to either kind. Removing a native domain that still has issues is refused, since
  its issues would become unreachable; while the switch is off they are retained.
- UI copy names the tracker. For example: "G5 issue" or "GitHub issue"; "Post comment"
  or "Post to GitHub"; the Connect GitHub prompt, the "as @login" text and the
  hidden-line notes appear only for GitHub destinations.
- `.agents/AGENTS.md` (shipped as the harness instructions) describes `KEY-n` references,
  `-R KEY` and email assignees beside the GitHub forms.

## Alternatives considered

- **Markdown files in the docs repository** were rejected in the issue: concurrent state
  changes become Git conflicts, identity and primary uniqueness still need a platform
  index, and permissions are all-or-nothing.
- **Reusing `issue_comment_publications` for native comments.** That table is a delivery
  ledger (pending/unknown/reconcile, GitHub-numeric ids, `github_login NOT NULL`, shared
  with PR comments). A native comment is the content itself, written once in a
  transaction. Reusing the ledger would add a state machine with nothing to reconcile.
- **A `native_issue_events`-only design with state derived from events.** Rejected above:
  state filtering on every search, for no added guarantee.
- **Restoring the dropped local-issue schema.** That schema is gone (migration 025) and
  carried hotlists, labels, attachments and inference jobs that are out of scope. The
  envelope tables are smaller and fit the adapter shape.

## Capacity and limits

- Number allocation serializes creation per organization (one row lock, held for a few
  milliseconds).
- Search is Postgres text search on one organization's issues through a GIN index. It is
  adequate into the hundreds of thousands of issues per organization. Comments are not
  indexed.
- Activity reads all comments and events of one issue. The GitHub path caps at 100
  events; native applies the same cap with the existing truncation notice.

## Acceptance and tests

Done when an organization with no GitHub App binding (and a deployment with no App at all)
can do all of the following through the UI and `g5d issue`:

- create (Home, quick-add, CLI), search, view;
- link primary and related, unlink;
- comment and edit comments from Activity;
- close and reopen;
- Create & Fork;

all with the primary, Base and fork-race rules intact.

- **API integration** (`api_external_issues.rs`, sharing its provider-neutral helpers):
  - native create/replay/conflicting key; parent same-domain and refused elsewhere;
  - comment/edit with stale refusal; state change idempotence and events;
  - search filters, completeness with a failing GitHub domain beside native;
  - references (`KEY-n`, bare number ordering, ambiguity);
  - primary claim races and fork-primary race with native ids;
  - Base refusal;
  - switch off: reads refused and cleanup allowed;
  - RLS isolation across two organizations;
  - agent routes (origin agent, no transition, no budget spend);
  - and a deployment with `github_app = None`.
- **Regression:** the existing GitHub suite passes unchanged except for the renamed
  feature gate.
- **Frontend unit:** provider-aware copy in the composer, rail, issue page and create
  dialog; no Connect prompt for native.
- **Browser E2E** (native stack, no App): create → Create & Fork → comment → Done →
  reopen.
- **CLI:** `g5d issue` text and `--json` output for native issues.

## Implementation slices

1. Migration and native store with RLS; the adapter enum; GitHub code moved behind it with
   no behavior change (the existing suite proves this).
2. Native reads, search, references and Activity; the instance feature and the settings move.
3. Native writes: create, comment, edit, state, and the agent routes.
4. UI copy, issue-page controls (Q4), `g5d` and `AGENTS.md`.
5. Export: an administrator JSON download of the native domain's issues, comments and
   events.

## Validation

Run on 2026-10-22 against the workspace branch, local Postgres (pgvector/pg17) and no
GitHub App unless a test mounts its mock:

- `api_native_issues.rs` (4 tests, deployment without the App): domain admin, keyed
  creation and checks, parents, assignee refusal, search filters and key/number matching,
  references, issue page; links with primary/Base rules; Activity entries, composer,
  comments and conditional edits; state changes with one event per change; issue page
  writes; export; domain removal refusal; organization isolation and FORCE RLS; viewers
  read-only; switch off; the agent's create, view, link, comment (primary and any issue)
  and search without spending the GitHub budget.
- Regression: `api_external_issues`, `api_workspace_timeline`, `local_issue_removal_guard`,
  `api_org_admin_authority`, `service_context_layering` and `route_auth_static` pass; the
  fork-without-App test now enables issues first, since a fork can name a G5 issue without
  the App. Full `g5-api` integration run: 1659 of 1662 passed. Two failures were fixed and
  rerun green: the mixed-tracker test's own setup, and the provider-dispatch guard, which
  now permits parsing a domain request's provider in `NewDomainProvider::parse`.
  `api_intent_log::test_intent_log_git_fallback_participates_in_actor_filter_and_default_sort`
  also fails on `origin/main` in this VM, so it is not caused by this change.
- `api_external_issues.rs` adds `the_workspace_agent_comments_on_an_unlinked_github_issue_as_its_owner`
  and `g5_issues_and_github_domains_search_together_and_report_failures_separately`.
- `g5d` unit tests (`cli::issue*`), `cargo clippy` (both CI invocations, `-D warnings`),
  `cargo fmt --check`, migration checksum check.
- Frontend: `tsc --noEmit` clean; the 13 touched test files (263 tests) pass; the agent's
  full `vitest` run passed (316 files).

- Browser smoke (local native stack, no GitHub App, Chromium, an uncommitted Playwright
  script): Admin page turns Issues on and adds G5 issues (`ACME`), Home's Create makes
  `ACME-1` and opens `/<org>/issues/g5/1`, a comment posts and the issue closes. No committed
  E2E spec was added; CI's browser lanes do not cover the native flow yet.

## Open assumptions

- Native content defaults to trusted. If the owner prefers untrusted-by-default, only the
  domain default changes.

## Structured native search and one search for linking

Owner request (2026-10-23, [chat intent](../../chat_intent/issue-681-native-g5-issue-tracker/request.md#later-instruction-2026-10-23-search)):
the workspace link form failed on plain words without the GitHub App and should use the
same search as everything else; native search should find an assignee's issues by name and
know issue state. Ontology: [`g5.issue.search.one-path`](../../ontology/features/issues.g5.md#g5.issue.search.one-path)
(new) and [`g5.issue.native.search`](../../ontology/features/issues.g5.md#g5.issue.native.search) (amended).

- **One search.** The link form calls the organization search
  (`GET /orgs/:orgId/issues/search`, state All) and links the chosen result by its display
  key through the existing link route. Home search, the Issues tile and Import task already
  use it. The link route keeps resolving references, because `g5d issue link` and
  fork-linking name issues that way; it is not a second search. Reason: the form's failure
  came from the resolver being its only path, and a GitHub-only resolver fallback is exactly
  the second mechanism the owner wants gone.
- **References through search.** A GitHub domain given a query that is a GitHub issue URL or
  `owner/name#n` naming its own repository fetches that issue instead of sending the text to
  GitHub search. A reference to another repository finds nothing there. Native search already
  matched `KEY-n`, `#n` and its issue URL. The State and assignee filters still apply, so a
  pasted reference behaves like any other query. Without this, pasting a GitHub URL into the
  link form would lose what the resolver gave it.
- **Native query grammar** (parsed in Rust, evaluated in one SQL query per domain):
  - Words and quoted phrases must all match. `-` negates, and `OR` between two terms
    accepts either.
  - A word matches if it is in the stemmed title and description, if it is the issue's
    state (`open`, `closed`), or if it matches an assignee's name or email. A name matches
    on equality or on a word prefix of two or more characters; an email matches whole or on
    its local part.
  - `assignee:WHO` and `author:WHO` take `@me` (the caller, or the agent's owner), a name or
    an email. `no:assignee` keeps unassigned issues. All three can be negated.
  - `is:`, `state:`, `repo:` and the other reserved qualifiers are dropped, as on GitHub
    domains: the State control owns state.
  - Other GitHub qualifiers (`label:`, `milestone:`, `in:` …) are refused with what G5
    issues support. A refusal is a domain error under search completeness. Refusing follows
    the agent rule that an unhonoured qualifier is refused, never ignored; native issues have
    no labels by [scope](../../ontology/features/issues.g5.md#g5.issue.native.scope).
  - The title-fragment and key/number alternatives still apply to the free text, and
    qualifiers always filter.
- **No stored people text.** People are matched at query time: the domain's assignees (and
  authors, only for `author:`) are read under the tenant session, their current profiles from
  the system pool, and matching ids are passed to SQL. Considered and rejected: adding
  assignee names to the stored search vector. It needs a write on every assignment, a
  cross-owner refresh when a person is renamed (profiles are system data that native issues
  cannot read), and goes stale without that refresh. State needs no stored copy either,
  since the row has it.
- **Capacity.** Matching people costs one extra query plus a profile read, bounded by the
  people ever assigned in the domain, and runs only when the query has words or person
  qualifiers. A word that matches a person or state word is checked per row, in recency
  order, until a page fills. A search that matches nothing scans the domain's issues, which
  is adequate for tens of thousands of issues. Pure-text searches keep the GIN index.
