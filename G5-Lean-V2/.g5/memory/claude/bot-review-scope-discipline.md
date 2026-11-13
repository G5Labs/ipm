---
name: bot-review-scope-discipline
description: "Do not implement bot review findings (Codex, CodeRabbit) for provider behaviour that cannot occur; hold the issue's scope and answer \"not applicable, out of scope\"."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 76dc5ac6-7458-40a9-b67f-65145ee97d34
  modified: 2026-10-14T02:28:16.257Z
---

On PR #507 (issue #503, 2026-10-14) a simple fix (raise the provider line
ceiling, bound recorded events, count recorded bytes) grew from about 200 to
about 640 production lines over eight bot-review rounds. Each round fixed a
Codex or CodeRabbit finding about cases no pinned provider produces
(megabyte ids, hundreds of 16 MiB pending requests, thousands of
near-ceiling replies, 8 MiB stderr lines), and every fix gave the next round
new code to critique. The owner (zcl) had all of it reverted and said I was
over-engineering and getting distracted by reviewers.

**Why:** most bot findings are not realistically reachable; implementing them
adds code, commits, CI cycles and ontology bloat without user value.

**How to apply:**
- Before changing code for a bot finding, ask whether the pinned provider
  version can actually produce the frame or state. If the finding does not
  name a provider version and the frame that produces it, reply "not
  applicable, out of scope for #N" and move on. Owner's explicit rule.
- Implement only what the user required (they asked for P0/P1 and
  Critical/Major); P2/Minor are not obligatory, and even P1s need the
  realism check above.
- Do not keep posting `@codex review` after every push. In G5-Lean, Codex
  reviews only when asked, so each request starts a new round.
- Keep ontology blocks as statements of intent; mechanism goes in the design.
- If the diff keeps growing round over round, stop and re-check against the
  issue's scope before pushing again.

Related: [[g5d-pr-comments-and-body]].
