---
name: full-stack-preview-claude-pin
description: "The full-stack preview fails in a VM whose image Claude CLI is older than scripts/claude-code-cli-version, and its dev-up first kills any dev-up host sharing /work/code/.dev/g5d-host."
metadata:
  node_type: memory
  type: project
  originSessionId: 6eb68bfc-6bca-4f59-a1fd-43dda2d9fa97
  modified: 2026-10-26T05:43:01.372Z
---

Seen 2026-10-26 (#823, PR #828): the `full-stack` preview profile failed at `worker-host-start`
with `process mode needs pinned claude at /opt/g5/bin/claude: expected '2.1.308 (Claude Code)',
got '2.1.285 (Claude Code)'`. The containers were healthy; only the host stage refused. The
repo pin moved to 2.1.308 on main (5dc4530a1) ahead of the QA guest image. A `dev-up` with
`G5_CLAUDE_CODE_FAKE=1` skips the check, which is why an agent's fake-provider stack worked.

**Why:** the preview's output is not readable from the VM (no g5d log command, and the
cleanup removes the containers). Reproduce by running the profile's `command` with its
`env` exported, logging to a file, then `just dev-stack-down` with the same env.

**How to apply:**
- The host dir `.dev/g5d-host` is not scoped by `G5_COMPOSE_PROJECT`. A preview started
  while an agent's own `just dev-up` runs from /work/code stops that agent's worker host
  (fingerprint mismatch) before failing. Do not keep an agent dev stack running when the
  owner wants a full-stack preview.
- The fix needs the pinned CLI first on PATH (`/usr/local/bin` precedes `/opt/g5/bin`),
  which changes `claude` for every PATH lookup in the VM. Ask the owner first.
- 2026-10-26 the owner approved it: installed claude 2.1.308 (native `bin/claude.exe` from
  `npm install @anthropic-ai/claude-code@2.1.308`) to /usr/local/bin/claude, and codex
  0.164.0 (the codex pin was also ahead of the image's 0.159.3) as a node wrapper at
  /usr/local/bin/codex over `npm install -g --prefix /usr/local/lib/g5-codex-0.164.0`.
  Lost on VM reset; re-check `claude --version`/`codex --version` against scripts/*-cli-version.
  `check-codex-cli-flags.sh` needs an existing CODEX_HOME or it errors.
- `lsof` is not installed, so `g5_worker_host_reap_strays` never matches by open files here.
- The personal-workspace E2E under a process-isolation host writes
  `reference-source-*.txt` (`REFERENCE_ONLY`) into every real `/work/*` repo; delete them
  before any `g5d pr create/update`, which saves dirty files.

Related: [[in-vm-preview-runs-dev-up]], [[full-stack-chat-e2e-in-vm]], [[pkill-f-kills-own-shell]].
