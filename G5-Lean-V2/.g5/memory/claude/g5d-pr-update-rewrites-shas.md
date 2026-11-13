---
name: g5d-pr-update-rewrites-shas
description: "g5d pr create/update re-author the branch's commits as the workspace owner, so local SHAs change on every push; read HEAD again (or omit --head) before watching checks."
metadata:
  node_type: memory
  type: reference
  originSessionId: a5249e53-7356-4b88-a7f7-a5c0aae6489a
  modified: 2026-10-22T07:16:47.286Z
---

`g5d pr update` and `g5d pr create` "save and push" by re-committing the branch's
commits under the owner's Git identity (same messages, same trees), then fast-forward
the local branch to the rewritten head. Every push therefore changes the SHAs you
printed before it (seen on #709, 2026-10-22: `2e1c2e05` became `d40a42fa`,
`c6206f19` became `4673e2f0`).

**How to apply:** capture `git rev-parse HEAD` *after* `g5d pr update` returns, not
before, when starting `g5d pr checks --watch --head …` or any SHA-keyed watch; a watch
on the pre-push SHA reports `none`/pending forever. Commit attribution lines still
land in the message. Related: [[background-watches-die-with-session]].
