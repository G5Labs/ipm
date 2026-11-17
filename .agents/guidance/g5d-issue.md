# `g5d issue`: the organization's trackers

Read this for issue work beyond viewing one issue: searching, creating, commenting, editing, labelling, linking, closing or reading attachments. `g5d issue <command> --help` lists every flag; this page gathers the behaviour, limits and exit codes. Writes are made as the workspace owner: on GitHub under their connected account, on G5 as them. Commands below use the `g5d issue` prefix unless they name another.

## Contents

- [References](#references)
- [Read](#read): `view`, `search`, `list`, `attachments`
- [Write](#write): `create`, `comment`, `edit`, `close`, `reopen`, `--attach`
- [Workspace links](#workspace-links): `link`, `unlink`
- [Labels](#labels)
- [Assignees](#assignees): `g5d user`
- [Exit codes](#exit-codes)

## References

References are GitHub `owner/name#12` or G5 `ACME-12`. `-R owner/name`/`-R ACME` scopes a number; bare numbers select a linked issue first, then the creation destination.

## Read

- `view [REF]` (alias `show`): any configured issue domain; others refused. Defaults to the primary issue. Comments: `-c`/`--json comments`.
- `search [QUERY]`: `--state open|closed`, `--assignee @me`, `--no-assignee`, `--label NAME`, `--no-label`, `-R`, `-L` (maximum 30). GitHub qualifiers such as `label:bug` go in QUERY. G5 searches title, description, state and assignees' names; its only qualifiers are `assignee:`, `author:`, `label:`, `no:assignee`, `no:label`. Search budget is shared with other agents: search sparingly; exit 3 means wait as instructed.
- For view/search, `--json a,b` selects gh fields plus `workspaceLink`, `workspaceLinks`, `primaryWorkspace`, `contentTrusted`; bare `--json` prints the full G5 record.
- `list`: this workspace's links.
- `attachments [REF] [-c] [--dir DIR] [--list]`: save the attachments of an issue view can read (images, videos and files; with `-c`, its comments' too) as files to inspect, since this VM cannot download them directly; `--list` only lists. Exit 1: some were not retrieved, each named with its reason; expect a failure only where the output reports one. For visual work against mocks, compare with the files; if retrieval fails, say a comparison cannot be made before relying on them.
- `g5d pr attachments NUMBER|URL [-R OWNER/REPO] [-c]` does the same for a GitHub PR (`-c`: comments, reviews and review comments); G5 PRs have none of their own.

## Write

- `create -t TITLE -b BODY`: create as owner at the organization's creation destination; print URL without prompting or linking. Alternatives/options: `-F FILE`, `-a LOGIN`/`@me` (email for G5), `-l LABEL` (existing labels), `--parent ISSUE` (same-domain parent). Exit 7: GitHub did not confirm issue/parent; rerun with the printed `--idempotency-key` before creating again. Exit 1 after a URL: created, but not under the parent or without some labels.
- `comment [REF] -b TEXT`: comment as owner on the primary issue by default, or any issue view can read. GitHub requires a non-public repository in a domain allowing G5 comments. Exit 7: unconfirmed; rerun with the printed `--idempotency-key` (G5 looks for it first).
- `edit [REF]`: `-t TITLE`, `-b`/`-F` (replaces description), `--add-assignee`/`--remove-assignee` (comma-separated; `@me`), `--add-label`/`--remove-label` (existing labels; replace a priority by removing the old and adding the new); same issues, account and GitHub requirements as `comment`. No milestones or listeners. Unassignable people and unknown labels are refused before any change. Exit 7: unconfirmed; read the issue, then rerun. Exit 1 with "not every ... change": partly applied; the rest is listed.
- `close [REF]`/`reopen [REF]`: change state as owner, using their connected GitHub account for GitHub, or as them on G5; primary issue by default, or any issue view can read. Exit 7: GitHub outcome unconfirmed; rerun.
- `--attach FILE` (repeatable, at most 10) on `create`, `comment` and `edit` (with `-b`/`-F`): add local files (any type, 10 MiB each) to a G5 issue as owner, shown at the end of the text (images inline, other files as links); GitHub issues are refused before anything is sent. Reruns add no file twice.

## Workspace links

- `link REF [--primary]`: related by default.
- `unlink REF`: remove one workspace link, including primary, without changing the issue.

## Labels

`label list|create|edit|delete [--domain owner/name|KEY]`: a domain's label catalog (the creation domain by default), as owner; `create NAME -c COLOR -d TEXT`, `edit NAME -n NEW`, `delete NAME --yes` (removes it from every issue). Issues take only labels the domain already has. Labels classify only; `priority:p0`–`p3` and `source:formal-verification` are conventions, not evidence.

## Assignees

Resolve assignees with `g5d user search [WORDS]` (`--include-deactivated`, `-L`) or `g5d user view USER|@me`: members' emails and connected accounts (`github:LOGIN`). G5 takes email; GitHub takes login. Bare `--json` returns the full record (view: object; search: array); `--json email,accounts` selects those fields.

## Exit codes

- 1 after a `create` URL: created, but not under the parent or without some labels. 1 from `edit` with "not every ... change": partly applied. 1 from `attachments`: some were not retrieved.
- 3 from `search`: wait as instructed; the search budget is shared with other agents.
- 7 from a write: unconfirmed. Rerun with the printed `--idempotency-key` (`create`, `comment`), read the issue then rerun (`edit`), or rerun (`close`, `reopen`).
