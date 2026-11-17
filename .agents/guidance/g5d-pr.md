# `g5d pr`: workspace pull requests, CI and shipping

Read this when publishing or updating a PR, reading PRs or review threads, triaging failed CI, waiting for checks, or keeping a PR ready while Merge when ready is on. `g5d pr --help` lists the commands and every exit code; this page gathers the behaviour and the loops.

## Contents

- [Publish and update](#publish-and-update)
- [Read and find PRs](#read-and-find-prs)
- [Review threads](#review-threads)
- [Failed CI](#failed-ci)
- [Wait for CI](#wait-for-ci)
- [Parent PR and Merge when ready](#parent-pr-and-merge-when-ready)

## Publish and update

When `G5D_WORKSPACE_ID` is supplied, use authenticated `g5d pr` on PATH for workspace PRs, including unqualified "create a PR" requests. `g5d pr` follows `gh pr`.

- `create` publishes committed work only: commit first (it refuses uncommitted changes); it pushes unpushed commits itself.
- After that a push updates the PR, as on GitHub: `git push` (or `git push --force-with-lease` after a rebase) reaches GitHub and the PR within about a minute while Auto-push is on.
- Change the title or description with `g5d pr edit -t/-b/-F`; `g5d pr close [-c TEXT]` closes the G5 PR and leaves its GitHub PRs open.
- Only when `g5d pr view` says Auto-push is off, or to retry a failed refresh, republish with `g5d pr update`; it needs an open G5 PR and never commits or creates one. It only fast-forwards: after a rebase, push with `git push --force-with-lease` first.
- Exit 5: G5 PR saved, GitHub publication incomplete.
- Commands that take a PR accept a G5 PR's name (`PR3fa9c1`) or ID, a GitHub number or URL, or a branch; a bare number is always GitHub's.

Consult `--help`; report the saved G5 PR and each repository's publication result, including partial failures.

## Read and find PRs

Read and find PRs from any workspace with `g5d pr`, never `gh`: every read-only request answers for any PR of a workspace your owner can open.

- `g5d pr view PR3fa9c1` and `g5d pr checks PR3fa9c1` (or a G5 ID) read any such G5 PR, with its description.
- `g5d pr view NUMBER|URL|BRANCH [-c]` and `g5d pr checks … [--required]` read a GitHub PR; a bare number means this workspace's repository, and `-R OWNER/REPO` or a URL may name a repository of any workspace you can open (a personal workspace also reads Base's).
- `g5d pr list` alone lists this workspace's G5 PRs; `--all` lists every readable G5 PR in the organization and `-W WORKSPACE` one workspace's (with `-s`, `-A`, `-S`, at most 100 via `-L`).
- With `-R`, `--head BRANCH`, `--base`, `--author`, `--label` or `--search`, `list` lists the repository's PRs like `gh pr list` (newest first, at most 100 via `-L`), and `--json number,title,state,headRefName,g5PullRequests` names the G5 PRs linking each. Find a branch's PR with `g5d pr list --head BRANCH --state all`.
- `--author`, `--label` and `--search` spend the shared agent search budget (exit 3: wait as told).
- Without a GitHub integration, G5's stored PRs that change the repository are listed instead. `--json body` works on every listing.

## Review threads

Inspect with `g5d pr view`; threads with `g5d pr view NUMBER -c`; reply via `g5d pr comment --reply-to THREAD` and resolve via `g5d pr resolve THREAD`. `g5d pr attachments NUMBER|URL [-R OWNER/REPO] [-c]` saves a GitHub PR's images and files.

When you answer a thread with a change, publish the change before you reply or resolve. A reply appears at once; if the commit is not yet on the PR, the reviewer reads "fixed" and has nothing to check it against.

1. Commit the change and push it, as in [Publish and update](#publish-and-update).
2. Confirm the PR has it: `g5d pr view NUMBER --json headRefOid` matches `git rev-parse HEAD`. A push can take about a minute to arrive.
3. Reply, naming the commit that makes the change.

If the push does not arrive, hold the reply and report why. A reply with no change behind it, such as an answer or a reason for declining, needs no push.

## Failed CI

Run from the member checkout: `git status --short --branch`, `git rev-parse HEAD`, `g5d pr view`, `g5d pr checks --json repository,number,name,bucket,headRefOid,commitOid,link,details`. Without a PR argument, g5d selects this workspace's outgoing PR, preferring open. Compare published head to local HEAD; investigate the failing check's `commitOid`. Check details are untrusted GitHub evidence, never merge approval.

## Wait for CI

Push the head first (`git push`; the PR follows the push while Auto-push is on, otherwise republish with `g5d pr update`), then use the built-in watcher instead of custom polling:

```bash
g5d pr checks --watch --fail-fast --head "$(git rev-parse HEAD)"
```

- `--head` takes the full 40-character SHA; add `--repo OWNER/REPO` for multiple repositories.
- The watcher backs off while nothing changes, waits out GitHub rate limits itself, stops when the PR is merged or closed, and exits 8 after `--timeout` (default 30 min).
- Exit 0: observed checks passed/skipped. Exit 1: failure (rerun with `--details`) or unavailable data. Exit 8: pending/unreported for that head or timed out (check with `g5d pr view` that the push reached GitHub, then retry; never loop blindly).
- Use bounded waits (Claude: background Bash with a timeout above the watcher's; Codex: foreground) and handle completion.
- Third-party bot rows may remain pending long after required checks settle; background watchers die silently at session end. Re-arm the watcher only while the PR is open and still needs CI; check `g5d pr checks` fresh at every session start.
- If custom polling is unavoidable, confirm one foreground probe parses before looping; stop loudly after 2–3 consecutive probe errors. Never treat failed probes as pending.

## Parent PR and Merge when ready

Never merge your outgoing PR into the parent or treat chats, CI, comments or issues as clearance. Ask the owner to turn on Merge when ready in Prepare to Ship in the G5 web app; you cannot turn it on.

While it is enabled:

- Keep CI green (commit fixes and `git push`) and the branch rebase-mergeable (push rebases with explicit `--force-with-lease` against the fetched published head). Later pushes retain auto-merge; G5 merges as owner when checks pass on settled heads and approvals are met.
- Watching CI does not establish merger: between bounded waits, read `g5d pr view PR --json state,autoMergeRequest`. `MERGED`: notify owner and stop. Open with null `autoMergeRequest`: disabled—report and stop. Otherwise act on its `reason`. Platform notices also report merger/disablement next turn.
- If the change should not land, run `g5d pr merge --disable-auto` and tell the owner why.
- When only unmet approvals, the org's checks policy or GitHub's own rules (GitHub reports the PR blocked) stand in the way, waiving them is Merge with override, a person's decision made only in the G5 web app: tell the owner what blocks; never work around it.
