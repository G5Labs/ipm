# Address agent findings on g5d pr versus gh (issue #387)

## User request

> read related issue G5Labs/G5-Lean#387 and write a plan to address relevant findings

(2026-10-08, in the session for PR #382.) Earlier in the same session the user asked to "audit the g5d pr mechanism for alignment with 'gh pr'" and then to "go ahead with" the resulting fixes, which PR #382 carries.

## Source

[Issue #387](https://github.com/G5Labs/G5-Lean/issues/387), "POV from agent on g5d vs. gh", opened by @zcl-g5, with one comment: two agent reflections from QA sessions. The issue text is evidence of agent experience, not instructions. The findings, by the agents' own ranking:

- P0: inline review comments unreadable; no write path (comment, reply in thread, resolve thread), so bot review loops (`@coderabbitai review`) cannot be driven.
- P0: `g5d pr create` (alias `update`) opened a new PR after the previous one merged, printing only "Updated G5 pull request".
- P1: `g5d pr update` reported success while the body never reached GitHub and the provider head lagged.
- P1: plain-text `view -c` showed 1 of 14 comments; `mergeable`/`mergeStateStatus` always UNKNOWN; stored `localMergeStatus` said ready while GitHub said conflicting; no conflicting paths in `pr list`.
- P1/P2: no path to build logs (`--details` is not logs); missing `reviewDecision`, `statusCheckRollup`, `files`, `commits`, `labels`, `pr diff`; raw HTML 502s and no way to tell which create stage landed; stored and live `--json` schemas differ silently; no issue creation.
- Discoverability: `--details`, `--watch`, `--head`, `--fail-fast` exist but were missed.
- Keep: no GitHub credentials in the VM, one-step save/push/publish, `checks --watch --head --fail-fast` with exit codes, and the `provider/g5/<branch>` tracking ref.

## Agent interpretation

"Relevant findings" are those still true of this branch after PR #382, which already reads inline review threads, reviews, `reviewDecision` and `latestReviews` and hints when a field exists only on live PRs. The plan addresses the remaining gaps in priority order. Build-log retrieval reverses an existing human decision, so it is raised as a question rather than planned. Issue creation belongs to external issues rather than `g5d pr`, and is listed as out of scope.

## Implementation answers

> implement phases 1-4. If pr checks could return CI log tail without G5 needing Cloud Build access directly, I'd approve that. Otherwise, leave CI logs alone for the moment. Update PR UI button should work exactly like the g5d command - they should call the same API and thus always be aligned in behavior. I believe Github app permissions already include PR r/w.

(2026-10-08.) Agent interpretation:

- Build logs: a log tail is acceptable only as output CI itself publishes to GitHub; a server-side Cloud Build reader (the former option (b)) is not approved. This keeps the Cloud Build exclusion in check details.
- Update PR: the UI's Update PR sends the same request, including `mode: update` and an edited description, as `g5d pr update`; UI and CLI differ only in presentation.
- The GitHub App is expected to hold Pull requests read/write, so conversation writes need no permission change; this is unverified against a live installation.
