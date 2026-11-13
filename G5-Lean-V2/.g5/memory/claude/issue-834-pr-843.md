---
name: issue-834-pr-843
description: "Markdown owner email (#834) shipped as PR #843; design choices the owner made, scope limits, and the CI/review loop state."
metadata:
  node_type: memory
  type: project
  originSessionId: 334369b3-b688-4c95-a35e-bdf2722bfcfd
  modified: 2026-10-26T19:56:57.974Z
---

Issue #834 (agent email to owner renders a sanitized Markdown subset) is PR #843 (G5 PR 3a349ed8), opened 2026-10-26, rebased onto main 280aaa157 (which carries #840's zstd 0.14 fix, so the PR's own zstd commit was dropped).

Owner decisions (2026-10-26): keep in-paragraph line breaks like GitHub comments; links may target every Git-provider repo of the org, not just the workspace's. Agent decisions recorded in `.g5/pr_ontology_diff_design/issue-834/ontology-plan.md`: reference-style link definitions are refused (pulldown-cmark consumes them, first-label-only map); G5's own links clickable only when the public URL is https; notice-first applies to plain-text bodies too.

**Why:** the owner asked to rebase, open the PR, monitor CI and address review comments.
**How to apply:** on resume, re-check `g5d pr checks` fresh (watches die with the session, see [[background-watches-die-with-session]]), read review threads with `g5d pr view 843 -c`, and follow [[bot-review-scope-discipline]]. The test layering map (`service_context_layering.rs` HOMES) must list every new email module file.
