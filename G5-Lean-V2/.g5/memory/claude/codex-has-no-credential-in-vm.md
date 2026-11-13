---
name: codex-has-no-credential-in-vm
description: "Codex CLI credential in the G5 MicroVM varies by VM: 2026-10-21 it had none (401); 2026-10-26 /opt/g5/bin/codex 0.164.0 reported Logged in using ChatGPT and `codex review --base main -c model=\"gpt-6-astra\"` ran. Check `codex login status` first; report a block only when it fails."
metadata:
  node_type: memory
  type: reference
  originSessionId: f8c3539e-99fa-49d9-ae5a-b613972307c3
  modified: 2026-10-26T23:09:39.000Z
---

In a G5 workspace MicroVM (checked 2026-10-21), `codex exec` (`/opt/g5/bin/codex`, a
wrapper around the npm CLI) fails with `401 Unauthorized: Missing bearer or basic
authentication` from `api.openai.com`. `G5D_SECRET_ENV_NAMES` lists only
`CLAUDE_CODE_OAUTH_TOKEN` and `QA_G5_CLOUDBUILD_READER_KEY_JSON`; no `OPENAI_API_KEY`
is exported and `CODEX_HOME` (`/home/agent/.codex`) has no `auth.json` (the directory
does not even exist until created, which the wrapper warns about).

**Why:** Only when `codex login status` reports no credential (as in the 2026-10-21 VM)
can requests like "run an independent Codex review loop until APPROVE" not be satisfied
from inside the VM; AGENTS.md then says to report the block and give the owner the
command to run in the workspace terminal rather than work around it. When it reports a
login (as on 2026-10-26), run the review in the VM.

**How to apply:** Probe first with a one-line prompt (`codex exec -s read-only
--skip-git-repo-check -o out.md - <<< 'Reply OK'`); if it 401s, keep the prepared
prompt and helper script in the job tmp dir, record the block in the plan, and hand the
owner the command. Do not try to route Codex through `ANTHROPIC_BASE_URL`.

**Update 2026-10-26:** in the `preview-ux-improvement` VM, `codex login status` printed
`Logged in using ChatGPT` and `codex review --base main -c 'model="gpt-6-astra"'` ran
(`review` takes no `-m`; use `-c model=...`, and it refuses a custom prompt together
with `--base`; `codex exec` takes a prompt). So check `codex login status` before
reporting a Codex request as blocked.
