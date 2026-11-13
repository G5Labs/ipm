---
name: pkill-f-kills-own-shell
description: "pkill -f \"<pattern>\" inside a Bash tool command matches the tool's own shell command line and kills it (exit 144); stop dev servers by PID or port instead."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 697991eb-2b46-46a9-84dc-7065ec7560b4
  modified: 2026-10-23T08:46:04.208Z
---

`pkill -f "vite.*15174"` run inside a Bash tool call killed that call's own `/bin/sh -c` (its command line contains the pattern), exiting 144 and silently skipping every later step in the same command (a commit, once).

**Why:** the Bash tool runs the whole command string as one shell process whose argv includes the pattern text.

**How to apply:** stop background servers with a PID saved at start, or select by executable and check argv: `for p in $(pgrep -x node); do tr '\0' ' ' < /proc/$p/cmdline | grep -q -- '--port 5191' && kill $p; done`. A `pgrep -f <pattern>` loop also matches the tool's own shell and killed it again on 2026-10-26 (`fuser` is not installed). Never chain such a kill before important steps such as commit or push in the same call.
