---
name: background-watches-die-with-session
description: "A CI poll watch missed completion twice over: its g5d probe failed every iteration (short SHA) and the '?' error guard kept waiting instead of failing loudly; g5d pr checks exits 8 while pending, so test the parsed output, not the exit code; killed sessions also end watches silently."
metadata:
  node_type: memory
  type: feedback
  originSessionId: cd9ca33f-4417-4104-a26f-c451a1174fcd
  modified: 2026-10-20T00:32:43.353Z
---

A background watch polling `g5d pr checks` on PR #636 never reported CI completion
(2026-10-20). Two causes, both to avoid:

1. **The probe failed on every iteration and the loop treated that as "still waiting".**
   `g5d pr checks --head` requires the FULL 40-character commit SHA; a short SHA exits 1
   with nothing on stdout. The loop parsed the empty output into `"?"` counts and its
   conditions explicitly skipped `"?"` (meant for transient blips), so neither the
   settled nor the failed branch could ever fire — the watch was blind from iteration 1
   while CI had long finished.
2. **The session then ended and killed the still-spinning shell** (output shows only
   `[killed]`, no notification), so even the timeout message never surfaced.

**Why:** a watch whose probe errors must fail loudly; mapping probe failure to "keep
waiting" converts permanent errors into silent hangs, and background shells do not
survive session teardown.

**How to apply:** run the probe once in the foreground and confirm it parses before
arming a loop on it; in the loop, treat repeated probe errors (2–3 consecutive) as a
failure exit, never as "pending"; always pass `"$(git rev-parse HEAD)"` (full SHA) to
`g5d pr checks --head`; keep watch horizons short and re-arm; re-check watched state
fresh at session start instead of trusting a prior watch.

**Exit code 8 is not a probe failure** (PR #728, 2026-10-22): `g5d pr checks --json …`
exits 8 whenever any check is still pending, with valid JSON on stdout. A loop that
used `out=$(…) || errors++` counted every pending poll as an error and quit after 3
("PROBE FAILED") while CI was healthy. Decide probe health by whether the output
parses, not by the exit status.
