---
name: g5d-pr-comments-and-body
description: g5d pr view -c reads PR discussion comments and review threads; g5d pr update -b/-F/-t now republishes body and title (verify with g5d pr view <n> --json body).
metadata:
  node_type: memory
  type: reference
  originSessionId: 696c299f-b4d3-466f-9422-ad8ea5f52629
  modified: 2026-10-08T16:32:48.529Z
---

Reading PR review feedback in a G5 workspace, where `gh` has no credentials and
the repo is private (WebFetch 404s):

- **`g5d pr view <number> -c`** returns the PR's GitHub *discussion* comments
  (first 100). This is the way in — do not conclude the comments are
  unreachable.
- The JSON field set **differs between the stored G5 PR and a live GitHub PR**.
  `g5d pr view --json` with no number lists the stored-PR fields (no comments);
  `g5d pr view <number> --json` lists live-PR fields including `comments`,
  `commentCount` and `reviewCommentCount`. Checking only the stored list is how
  I wrongly concluded comments were not exposed (2026-10-08, PR #362).
- Review threads: AGENTS.md (2026-10) documents `g5d pr view N -c` to read
  threads and `g5d pr comment --reply-to THREAD` / `g5d pr resolve THREAD` to
  answer and resolve them — check `--help` and use these before asking the
  human to paste `gh api repos/<owner>/<repo>/pulls/<n>/comments --paginate`.
- `g5d pr create` (and `update`) auto-commits ANY pending working-tree change
  as "Save workspace changes for pull request" (authored as the user) — even
  another session's edits you deliberately left uncommitted. Inspect
  `git status` first. A `500 INTERNAL_ERROR` from `create` can be transient:
  the save commit still happens; retry once and re-verify `headRefOid`.
- CodeRabbit's substantive findings live in its one discussion comment: the
  `Merge Risk` block and the `Pre-merge checks` table, not in inline threads.

**Update 2026-10-14 (PR #493):** `g5d pr view <n> -c` now also prints review
threads with their `PRRT_…` ids, which `g5d pr comment --reply-to` and
`g5d pr resolve` take. `g5d pr update -t … -F body.md` printed
"title applied · body applied" and `g5d pr view <n> --json body` showed the new
text, so the body problem below is fixed; still verify after updating.

**Historical (2026-10-08): `g5d pr update` could silently fail to publish, for the body AND for commits.**
It printed "Updated G5 pull request" and exited 0 either way.

- Body: stored as the G5 PR's `descriptionOverride` (`g5d pr view --json body`
  reflects it) while `g5d pr view <number> --json body`, the provider copy,
  keeps the text from the original `g5d pr create`.
- Commits: observed 2026-10-08 on #362 — `sourceHeads[].headCommit` and the
  proxy's `refs/g5-tracking/<branch>/baseline` both advanced to the new commit,
  but `headRefOid` (the GitHub `g5/<branch>` mirror) stayed several commits
  behind through repeated `g5d pr update` calls and a 3-minute wait, with
  `localMergeStatus: ready` and no `blockingMessage`. Reviewers and CI were
  looking at stale code.

So after every push, verify with
`g5d pr view --json headRefOid,sourceHeads` and compare `headRefOid` against
`git rev-parse HEAD`. If they differ, say so plainly instead of reporting the
work as published — a green check or a bot review may be describing an older
head.

Related: [[g5-vm-resets-lose-toolchains]].

Right after `git push`, `g5d pr update` can report "GitHub publication is incomplete or
pending" and `g5d pr view --json headRefOid` still shows the previous head, so a
`g5d pr checks --watch --head` armed from that value watches a stale commit and replays its
old result. Loop `g5d pr update` until `headRefOid` equals `git rev-parse HEAD` before arming
the watch (seen twice on #711, 2026-10-22).
