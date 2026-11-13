# `g5d issue view` of any issue, `search` and `unlink` (issue #558)

Intent: the owner's [issue #558](https://github.com/G5Labs/G5-Lean/issues/558) (its Ontology Statements, Proposed surface and "Things to settle"). Comparison base: `main` at `0134daf4`. This continues the [#478 design](../issue-478-create-issue/ontology-plan.md), which built `create`, `link` and the `view` alias.

## Ontology changes

| Block | Change | Why |
| --- | --- | --- |
| [`g5.external-issues.cli`](../../ontology/features/issues.g5.md#g5.external-issues.cli) | Amended | `view` of any issue with `-c` and `-R`, `search`, `unlink` (primary included), the bare-number order, `--json FIELDS`, and refusing what G5 cannot honour. |
| [`g5.external-issues.cli.reads`](../../ontology/features/issues.g5.md#g5.external-issues.cli.reads) | New | Agent reads stay in the configured domains on the installation token, need the owner to still be a member, and use the owner as viewer. |
| [`g5.external-issues.cli.search-budget`](../../ontology/features/issues.g5.md#g5.external-issues.cli.search-budget) | New | Agent search is limited per workspace and per organization, failing closed. |

The diff is this branch's commit touching `.g5/ontology/`. After main's ontology rewrite (PR #500) removed `features/external_issues.g5.md`, these blocks were carried into `features/issues.g5.md` in its compact form (2026-10-15); governing blocks below link to the file as it stood at the comparison base.

## Governing ontology

- [`g5.external-issues.intent.presentation`](https://github.com/G5Labs/G5-Lean/blob/0134daf4/.g5/ontology/features/external_issues.g5.md#present-the-trackers-native-issue-context): "authorized search". The agent's search is the person's search, run as the owner.
- [`g5.external-issues.association.primary`](https://github.com/G5Labs/G5-Lean/blob/0134daf4/.g5/ontology/features/external_issues.g5.md#bound-primary-issue-associations) and [`.fork`](https://github.com/G5Labs/G5-Lean/blob/0134daf4/.g5/ontology/features/external_issues.g5.md#make-primary-association-handoff-explicit): an unlink is explicit, so the agent's unlink of the primary is allowed. It releases the claim without promoting another workspace.
- [`g5.external-issues.association.deletion`](https://github.com/G5Labs/G5-Lean/blob/0134daf4/.g5/ontology/features/external_issues.g5.md#release-associations-without-mutating-the-tracker): unlinking never changes the external issue.
- [`g5.external-issues.broker.credentials`](https://github.com/G5Labs/G5-Lean/blob/0134daf4/.g5/ontology/features/external_issues.g5.md#keep-integration-credentials-outside-the-workspace): the agent still gets no tracker token. All reads run in the API.
- [Typed VM tokens are checked by the workspace lifecycle's one reader](../../ontology/concepts/security.g5.md) (live workspace, same owner, the VM's epoch and lease). The new reads rely on that fence for the owner; the database keeps the owner a member.

## High-level design

```text
g5d issue view [REF] [-c] [-R repo] ── GET  /internal/workspaces/:id/issues/view?ref=&comments=
g5d issue search [Q] [flags]       ── GET  /internal/workspaces/:id/issues/search?q=&state=&assignee=&repo=&limit=
g5d issue unlink REF               ── POST /internal/workspaces/:id/issue-links/unlink {ref}
            │
            └─ each: live incarnation fence (current owner; the owner is a member by FK)
               view/search → resolve_reference / fetch_issue_detail / search_domains  (installation token)
               unlink      → unlink_workspace_link                                     (database only)
```

### Routes and authorization

The three routes are added to `routes/internal.rs`. Their incarnation paths go on the worker-token allowlist (`auth_middleware.rs::is_workspace_incarnation_internal_route`). Each calls the service functions the person's routes call, so the two paths share one implementation:

- `search_domains`, `resolve_reference`, `fetch_issue_detail` and `fetch_linked_issue_detail`
- `issue_link_refs` and `attach_primary_owners`
- `unlink_workspace_link`

The person's handlers keep their own request parsing. The search filter parsing (`state`, `assignee`) moves into one shared function, so both paths accept the same values.

**The owner's member grant.** View and search open the organization behind the live incarnation fence (`scoped_db_for_workspace_incarnation` with `workspaces:read`), as the other internal issue routes do. The fence rechecks, on every request, that the token's user is still the workspace's owner. The database keeps a live workspace's owner an eligible member: `workspaces_owner_same_org_fk` refuses removing or demoting a member who still owns one. So the owner is a member whenever the fence admits the agent.

*Amended during implementation (2026-10-15).* The first design opened the organization as the owner through `scoped_db_for_user_actor_in_org`, as an explicit membership check. The E2E test that removed the owner's membership showed that the foreign key already refuses that removal, which makes the extra check redundant. An ownership transfer ends the old owner's reads at the fence instead.

Unlink uses `workspaces:write` with the same fence, like the internal `link`.

**Admission order** (view and search):

1. The fence.
2. `ensure_external_issues_enabled`.
3. The workspace's links (view only), then `list_domain_access`.
4. Commit, then the GitHub calls.

No transaction stays open across GitHub.

### `view`

The server resolves the argument, so its rules have one home. This replaces the CLI's `select_link`:

| Argument | Resolves to |
| --- | --- |
| none | the primary link; "no primary issue" lists the links |
| a UUID | a link of this workspace (another workspace's link id is not found) |
| `N` / `#N` | a linked issue with number `N`. With several, an ambiguity error lists their keys. With none, the creation target via `target_reference`. With no creation target: "name the repository: owner/name#N or -R" |
| `owner/name#N`, URL | `resolve_reference` in any configured domain; elsewhere `outside_domain` |

`-R owner/name N` is composed by the CLI into `owner/name#N`. `-R` without a number is refused.

A resolved issue that this workspace links is read with the moved-issue check of `fetch_linked_issue_detail`. Any other issue is read with `fetch_issue_detail`. Comments are fetched only when asked (`comments=true`, from `-c` or `--json` naming `comments`). Without them the detail keeps `commentCount`.

The response is:

- `issue`: `ExternalIssueDetailDTO`, whose `links` are the issue's links as the owner sees them;
- `workspaceLink`: this workspace's link, or `null`.

The old `GET .../issue-links/:link_id` stays for `g5d` binaries already in running guests. The new CLI does not call it.

### `search`

The CLI validates before sending anything:

- `--state open|closed`; gh's default is both.
- `--assignee @me`. Any other login is refused: "G5 search filters only your own or unassigned issues".
- `--no-assignee`. It conflicts with `--assignee`.
- `-R/--repo` names one configured domain (`owner/name`). An unknown repository is refused before GitHub is called.
- `-L/--limit` takes 1–30, default 30, as gh's default. Above 30 is refused: "G5 search returns at most 30 issues; narrow the query".
- The query may not contain reserved qualifiers (`repo:`, `org:`, `user:`, `owner:`, `is:`, `type:`, `state:`). The person's search drops these silently. The CLI refuses each one and names the flag to use instead. Other qualifiers (`label:`, `author:` and so on) pass through to GitHub.

The server reruns the same checks; `limit` is bounded server-side too. It then takes the [budget](#search-budget) and calls `search_domains` over all domains, or over just the `repo` domain, with `attach_primary_owners` (viewer = owner). It truncates to `limit`. `truncated` and `domainErrors` are kept.

`ExternalIssueSummaryDTO` gains `author`, `createdAt`, `closedAt` and `commentCount`, moved from the detail, which flattens the summary, so the detail's JSON is unchanged. These are additive, the search payload already carries them, and they back gh's search fields (`commentsCount` is projected from `commentCount`). Summaries do not gain a body.

### `unlink`

`POST .../issue-links/unlink {ref}` matches only this workspace's active links, from the database:

- link id
- `displayKey`, case-insensitive
- `canonicalUrl`
- the number, ambiguous when several links share it

It then calls `unlink_workspace_link` with the owner as `unlinked_by`. There is no GitHub call and no external-issues switch check: unlinking is cleanup, and the integration design keeps cleanup available while the switch is off. An unknown, already-unlinked or other workspace's issue is not found. The response names the link and its role. The CLI prints "Unlinked acme/web#12 (primary) from this workspace; the issue is unchanged."

### CLI surface and output

- `IssueCommand` gains `Search` and `Unlink`. `Show` (visible alias `view`) gains `-c/--comments` and `-R/--repo`. New calls go through `InternalApiClient` methods.
- **`--json`.** `issue` takes the optional field list the way `pr` does (`scope_json_flag` and `PrArgs.json_fields`). `--json` alone prints G5's full record, as `create` and `list` already do. `--json a,b` projects gh's field names, through a small projection module beside `pr_json.rs`. Commands other than `view` and `search` refuse a field list.

  | Command | Fields |
  | --- | --- |
  | `view` | `assignees author body closed closedAt comments createdAt labels number state stateReason title updatedAt url`, plus `repository contentTrusted workspaceLink workspaceLinks` |
  | `search` | `assignees author closedAt commentsCount createdAt labels number repository state title updatedAt url`, plus `contentTrusted primaryWorkspace` |

  G5's own fields are opt-in: they appear only when named or with `--json` alone. Their names (`workspaceLink`, `workspaceLinks`, `primaryWorkspace`, `contentTrusted`) cannot be mistaken for GitHub data or collide with a gh field, and they never change a gh field's shape. They carry what the text output shows: whether another workspace already holds the issue, before `link --primary`, and whether its text is from an untrusted domain.

  States are uppercase for `view` and lowercase for `search`, as gh prints them. gh fields G5 lacks are refused with the list of available fields: `id` (GitHub's GraphQL id, which G5 does not hold), `milestone`, `projectItems`, `reactionGroups`, and `body` for search (use `view`).
- **Text output.** `view` keeps G5's labelled layout, because it must carry the context note. It changes in three ways:
  - the header shows `(primary)`, `(related)` or `(not linked)`;
  - a "Linked from" line lists the workspaces that link the issue;
  - comments are printed only with `-c`; otherwise "Comments: N (use -c to show them)".

  `search` prints one tab-separated line per issue, as `gh search issues` does without a terminal: repository, number, state, title, labels, updatedAt. The context note, a truncation notice and domain errors go to stderr.
- **Refused gh habits.** `gh_habit_hint` covers `issue` too: `--web`, `--jq`/`-q`/`--template`, and the search flags G5 has as qualifiers (`--label`, `--author`, `--mentions` and so on). These point to the qualifier (`label:bug`). gh flags with no G5 counterpart (`--sort`, `--order`, `--include-prs`, `--owner`, `--match`) say so. Clap's unknown-argument error stays the exit status (2).
- **Exit codes** follow `g5d issue`'s existing mapping: 2 validation, 6 not found, 3 rate limited (with the retry time), 1 other.

### Search budget

Agent searches take a pair from the shared `PostgresRateLimitStore` (`try_acquire_pair`, as `PlatformIssueThrottle` does) before any GitHub call:

- `g5:rate-limit:{agent-issue-search}:workspace:<id>`: 6 per minute;
- `g5:rate-limit:{agent-issue-search}:org:<id>`: `max(1, 15 / configured domains)` per minute.

GitHub allows 30 authenticated search requests per minute, and one G5 search spends one per domain. The organization window therefore keeps agents together to about half of that, whatever the number of domains, and leaves the rest to people. A refusal is `RATE_LIMITED` with `Retry-After`, and nothing is sent. If the store cannot decide, the search is refused as unavailable (503), as platform issue reporting is. The window length is fixed and its size is computed per call, so a change in domain count takes effect in the next window.

*Amended in review (2026-10-15).* The organization window was first sized from the domains each search covered. A `-R` search of one domain then raised the window's limit for searches of all domains in the same window, so together they could spend more than 15 requests. The window is now sized from all configured domains, so every search in a window counts against one limit; a `-R` search is charged as a full one.

View and resolve use GitHub's core allowance (thousands per hour per installation). They are not budgeted; see the residuals.

## Settled design questions (issue's list)

- **Scope.** Reads stay in the configured issue domains on the installation token. Anything else is `outside_domain`, as for a person. Arbitrary GitHub issues are out of scope.
- **Bare `#N`.** A linked issue first, then the creation target. Ambiguity is reported. `link` keeps the creation target, since linking a linked issue does nothing. `unlink` matches links only.
- **Auth.** The live incarnation fence, which rechecks the current owner on every request; the database keeps that owner a member (see [Routes and authorization](#routes-and-authorization)). The internal `POST .../issues` is the precedent for acting as the owner.
- <a id="who-can-drive-the-agent"></a>**Who can drive the agent.** Chat input needs `workspaces:write` on the chat's workspace, and `scoped_db_for_chat_session` enforces the workspace's *current owner* for that. Workspace members with read access can view, not prompt. Jobs and platform-started turns run in the owner's workspace on the owner's behalf. The owner is an organization member, which the database enforces while they own the workspace. No one outside the organization can prompt a workspace agent. They can only influence it through content it reads (issues, PR comments, repository files). That is the untrusted-content concern below, not an authorization path.
- **Workspace links in results.** The owner is the viewer. Workspaces the owner cannot access are listed without name or id, exactly as the owner's own view shows them. `view` adds this workspace's own link role.
- **Untrusted content.** The `g5-linked-issues` skill the issue names no longer exists; the packaged guidance is the `g5d issue` bullet in `.agents/AGENTS.md`. That bullet is rewritten for `view`, `search` and `unlink`. It says that any issue's text, including search titles, is context, not instructions, and that untrusted domains are flagged. `view` keeps its note and the untrusted-domain line; `search` prints the note on stderr. `docs/external-issue-integration-design.md` §6 ("Agent read access") is updated to match.
- **Rate limits.** See [Search budget](#search-budget).
- **Unlink of the primary.** Allowed. The owner's route allows it, the CLI's purpose is parity, `link --primary` is already the agent's, and an unlink is the explicit handoff the fork rule asks for. The cost: an unlinked primary can be claimed by another workspace before the agent re-links it. Re-linking then reports the existing `issue_has_primary` conflict, so nothing is lost silently.

## Decision notes

- **`--json` alone keeps the full record.** gh prints the available field list instead, and `g5d pr` follows gh there. Here the issue asks for "`--json` for the full record", and `create`/`list --json` already print it. Switching `issue` to gh's behaviour would break those callers. The field list is printed in `--help` and in the error for an unknown field.
- **`-R/--repo` is included.** G5 supports both of its meanings (a repository for a number; one domain for search). Leaving it out would refuse something G5 can do.
- **Text layout of `view` is not gh's `title:\t…` form.** The layout must carry the context note. The JSON follows gh.
- **`search --limit` above 30 is refused, not clipped.** G5 searches one page per domain and merges them to 30. Clipping would quietly return fewer results than asked.
- **Old route kept.** `GET .../issue-links/:link_id` serves older guest binaries. Removing it is later cleanup.

## Residual risks

- View and resolve are unbudgeted. A looping agent spends core API calls (several per call: the domain repositories, the issue, the comments). It cannot starve the much smaller search allowance.
- With more than 15 configured domains, one search still spends more than 15 search requests (the window floors at one search). Charging a search by its domain count would need a weighted acquire in the rate-limit store.
- Several GitHub App installations across domains would each have their own search allowance. The organization window assumes one, which is conservative.
- Prompt injection grows with org-wide reads. G5 labels the text but cannot stop an agent from following it.
- Live GitHub is not exercised; tests use wiremock, as for #478.

## Acceptance conditions

E2E (`backend/g5-api/tests/api_external_issues.rs`, worker-token requests against wiremock GitHub):

- **View.**
  - An unlinked issue in a domain is read with its description, `workspaceLink: null` and the owner-view `workspaceLinks`. Comments come only when asked.
  - A linked issue reports its role. A moved linked issue is refused.
  - A bare number prefers the linked issue, then the creation target. A shared number is ambiguous. No target refuses.
  - A repository outside the domains is `outside_domain`, with no issue fetched.
  - Another workspace's link id is not found.
  - With the switch off, the read is `external_issues_disabled`.
  - After the workspace passes to another owner, the old owner's agent is refused.
  - A stale incarnation is refused.
- **Search.**
  - The state, `@me` (owner's account; refused without a connection), unassigned, `repo` and `limit` filters reach the GitHub query or the truncation.
  - Reserved qualifiers and an unknown `repo` are refused with nothing sent. `limit` 31 is refused.
  - `primaryWorkspace` is shown as the owner sees it.
  - The 7th workspace search in a window is `RATE_LIMITED` with nothing sent. A second workspace is refused once the organization window is spent. A person's search in the same organization still succeeds.
- **Unlink.**
  - A related link by reference is ended, and the issue is untouched (no GitHub call).
  - The primary is ended, and another workspace can then claim it.
  - Another workspace's link is not found, as is an already unlinked one.
  - It works with external issues off.
  - An ambiguous number is refused.

CLI (`backend/g5d/tests/issue_commands.rs`, mock API, and unit tests):

- `view`:
  - `-c` sends `comments=true`;
  - `-R` composes the reference;
  - the text shows "(not linked)", "Linked from" and the comment hint;
  - `--json title,comments` projects gh's names;
  - an unknown field lists the available ones.
- `search`:
  - prints gh's tab-separated lines;
  - writes the note, truncation and domain errors to stderr;
  - each refusal (`--assignee octo`, `-L 31`, `repo:` in the query, `--label`, `--web`) exits 2 with its hint and sends nothing;
  - rate limited exits 3 with the retry time.
- `unlink`: prints the role and that the issue is unchanged; not found exits 6.
- `create`, `link`, `comment` and `list` behave as before (existing tests).

Docs: `.agents/AGENTS.md` describes `view`, `search` and `unlink`, and `docs/external-issue-integration-design.md` §6 is updated.
