# Claude Code task secret env manual test

This runbook verifies that runtime-config secrets resolved for a task are visible
inside the Claude Code provided-agent environment, without printing real secret
values. Runtime Config secrets intended for the agent use the exact environment
variable names the agent should see.

Use disposable test values. Do not ask the agent to print raw credentials.

## Expected behavior

| Input configured in Runtime Config | Expected Claude Code env |
| --- | --- |
| Custom secret `ANTHROPOC_IMPORTANT_KEY` | `process.env.ANTHROPOC_IMPORTANT_KEY` is present with the exact value. |
| Secret `ANTHROPIC_API_KEY` | `process.env.ANTHROPIC_API_KEY` is present. |
| Secret `ANTHROPIC_AUTH_TOKEN` | `process.env.ANTHROPIC_AUTH_TOKEN` is present. |
| Secret `ANTHROPIC_BASE_URL` | `process.env.ANTHROPIC_BASE_URL` is present. |
| Secret `ANTHROPIC_MODEL` | `ANTHROPIC_MODEL` is present and the Claude Code primary model is updated. |
| Secret `CLAUDE_CODE_SUBAGENT_MODEL` | `CLAUDE_CODE_SUBAGENT_MODEL` is present. |

Keys with `G5_`/`G5D_` prefixes and dangerous process/server names such as
`PATH`, `HOME`, `DATABASE_URL`, and JWT/private-key variables are reserved for
G5 internals and must not be exported from user Runtime Config. Dotted aliases
such as `llm.apiKey` are not environment variable names and are not exported.
For OpenRouter-compatible Claude Code tasks, `ANTHROPIC_AUTH_TOKEN` remains the
user-configured secret; the worker may add an empty `ANTHROPIC_API_KEY` on the
explicit sidecar bridge as a Claude SDK compatibility sentinel.

Task-scoped values should override provider-config defaults for the new task.
Existing already-running tasks do not refresh their assignment secrets; always
start a new Claude Code task after saving Runtime Config changes.

## 0. Start a local stack with Claude Code available

From `generation/`:

```bash
cd generation
cp .env.example .env   # if needed
```

Provider API keys for this test are added through Runtime Config, not as
worker-managed keys. Keep only the normal Runtime Config encryption key and
Claude Code profile settings in `.env`:

**Historical configuration:** the fixture key shown below is rejected by current builds. Follow [current stack credential and recovery guidance](../../local-stack-secrets.md); the earlier paths and startup commands in this archived runbook are also superseded.

```dotenv
G5_SCM_ENCRYPTION_KEY=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
G5_CLAUDE_CODE_PROFILE=full
```

`G5_SCM_ENCRYPTION_KEY` is required for Runtime Config secret storage. The dev
compose stack provides the shown local-only default, but keep this explicit when
using a custom override file or a non-compose API process.

Start and seed:

```bash
just dev-up
just test-seed
source .env.test
./scripts/init-worker-volume.sh "$G5_E2E_TEST_WORKSPACE_ID"
docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo up -d --build --scale g5d-worker=1 g5d-worker
```

Confirm the worker advertises Claude Code:

```bash
docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo logs --tail=160 g5d-worker | rg "agent_provider:claude_code|Claude Code"
```

Open <http://localhost:3000>, log in with the local dev login, and open the
seeded `e2e-test` workspace.

## 1. Prepare a safe custom secret probe

Generate a unique sentinel and its short hash locally:

```bash
export G5_ENV_SECRET_PROBE="probe-$(date -u +%Y%m%dT%H%M%SZ)-$(uuidgen | tr '[:upper:]' '[:lower:]')"
export G5_ENV_SECRET_PROBE_SHA16="$(printf "%s" "$G5_ENV_SECRET_PROBE" | shasum -a 256 | awk '{print substr($1,1,16)}')"
printf 'probe sha16: %s\n' "$G5_ENV_SECRET_PROBE_SHA16"
```

This value is intentionally fake. It is safe to compare by hash.

## 2. Add the custom env secret through the UI

In the browser:

1. Open Settings.
2. Go to Runtime Config.
3. Use Workspace Overrides for this test, so cleanup is scoped to the seeded
   workspace.
4. In Secret Keys, enter key `ANTHROPOC_IMPORTANT_KEY`.
5. Enter the generated `$G5_ENV_SECRET_PROBE` value.
6. Click Queue.
7. Click Save Scope.

Expected UI state:

- The secret value disappears after save.
- `ANTHROPOC_IMPORTANT_KEY` appears as a secret key.
- It also appears in the effective secret key count/list for the workspace.

Optional API confirmation:

```bash
TOKEN="<copy localStorage.getItem('g5_auth_token') from the browser console>"
curl -sf \
  -H "Authorization: Bearer $TOKEN" \
  "http://localhost:7500/api/v1/workspaces/${G5_E2E_TEST_WORKSPACE_ID}/config?orgId=${G5_E2E_TEST_ORG_ID}" \
  | jq '.data.secretKeys, .data.effectiveSecretKeys'
```

The API must show the key name only, never the secret value.

## 3. Verify the custom secret inside a new Claude Code task

Start a new chat/task using Claude Code as the provider. Do not reuse an
already-running task. Send this prompt:

```text
Verify G5 runtime environment injection without revealing any secret values.
Run exactly this Node command in the workspace and report only the command output.
Do not print raw environment values.

node - <<'NODE'
const crypto = require("crypto");
const keys = [
  "ANTHROPOC_IMPORTANT_KEY",
  "ANTHROPIC_API_KEY",
  "ANTHROPIC_AUTH_TOKEN",
  "ANTHROPIC_BASE_URL",
  "ANTHROPIC_MODEL",
  "CLAUDE_CODE_SUBAGENT_MODEL",
];

for (const key of keys) {
  const value = process.env[key] || "";
  const digest = value
    ? crypto.createHash("sha256").update(value).digest("hex").slice(0, 16)
    : "";
  const secretLike = /KEY|TOKEN|SECRET|PASSWORD/.test(key);
  const detail = secretLike ? `sha16=${digest}` : `value=${JSON.stringify(value)}`;
  console.log(`${key} present=${value.length > 0} ${detail}`);
}
NODE
```

Expected result:

- `ANTHROPOC_IMPORTANT_KEY present=true sha16=<probe sha16>`
- The hash exactly equals `$G5_ENV_SECRET_PROBE_SHA16`.
- The raw probe value does not appear in the chat transcript.

Some Claude Code profiles may refuse to print hashes or other derived data from
secret-like environment variables. If that happens, rerun with a presence-only
prompt that does not emit a value or hash:

```text
Run a presence-only environment check. Do not print any environment values or hashes.
Run exactly this command and report only the command output:

node -e 'console.log("ANTHROPOC_IMPORTANT_KEY_PRESENT=" + Boolean(process.env.ANTHROPOC_IMPORTANT_KEY))'
```

Expected fallback result:

- `ANTHROPOC_IMPORTANT_KEY_PRESENT=true`
- The raw probe value does not appear in the chat transcript or worker logs.

If this fails, inspect the worker logs and confirm the task was created after
the Runtime Config save.

## 4. Verify provider env names

This step proves provider credentials are passed by their standard env names.
Use a disposable provider key if possible. If you use a real key, validate only
presence/hash and never paste the raw value into chat or logs.

### Anthropic env

In Workspace Overrides, add these secret keys, queue them, and save the scope:

- `ANTHROPIC_API_KEY` with the Anthropic key
- `ANTHROPIC_BASE_URL` with `https://api.anthropic.com`
- `ANTHROPIC_MODEL` with `claude-sonnet-4-20250514`
- `CLAUDE_CODE_SUBAGENT_MODEL` with `claude-sonnet-4-20250514`

Start a new Claude Code task and run the verification prompt from step 3.

Expected result:

- `ANTHROPIC_API_KEY present=true`
- `ANTHROPIC_AUTH_TOKEN present=false`
- `ANTHROPIC_BASE_URL value="https://api.anthropic.com"`
- `ANTHROPIC_MODEL value="claude-sonnet-4-20250514"`
- `CLAUDE_CODE_SUBAGENT_MODEL value="claude-sonnet-4-20250514"`

### OpenRouter env

In Workspace Overrides, add these secret keys, queue them, and save the scope:

- `ANTHROPIC_AUTH_TOKEN` with the OpenRouter key
- `ANTHROPIC_BASE_URL` with `https://openrouter.ai/api`
- `ANTHROPIC_MODEL` with `anthropic/claude-opus-4.7`
- `CLAUDE_CODE_SUBAGENT_MODEL` with `anthropic/claude-opus-4.7`

Start a new Claude Code task and run the verification prompt from step 3.

Expected result:

- `ANTHROPIC_AUTH_TOKEN present=true`
- `ANTHROPIC_API_KEY present=false`
- `ANTHROPIC_BASE_URL value="https://openrouter.ai/api"`
- `ANTHROPIC_MODEL value="anthropic/claude-opus-4.7"`
- `CLAUDE_CODE_SUBAGENT_MODEL value="anthropic/claude-opus-4.7"`

Run one of the Anthropic or OpenRouter mappings for the normal manual signoff.
Run both when changing provider routing logic.

## 5. Verify raw secrets did not leak

Search worker logs for the custom probe value:

```bash
docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo logs g5d-worker \
  | rg -F "$G5_ENV_SECRET_PROBE"
```

Expected result: no matches.

Also inspect the chat transcript. It should contain only the hash/presence
lines, not the raw probe or provider key.

## 6. Cleanup

Remove the test keys from Workspace Overrides:

- `ANTHROPOC_IMPORTANT_KEY`
- Any provider keys added for this test, such as `ANTHROPIC_API_KEY`,
  `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_MODEL`, and
  `CLAUDE_CODE_SUBAGENT_MODEL`

Clear or restore any test Settings JSON if you changed unrelated settings.

Optional API cleanup:

```bash
curl -sf -X PUT \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  "http://localhost:7500/api/v1/workspaces/${G5_E2E_TEST_WORKSPACE_ID}/config?orgId=${G5_E2E_TEST_ORG_ID}" \
  -d '{"removeSecretKeys":["ANTHROPOC_IMPORTANT_KEY","ANTHROPIC_API_KEY","ANTHROPIC_AUTH_TOKEN","ANTHROPIC_BASE_URL","ANTHROPIC_MODEL","CLAUDE_CODE_SUBAGENT_MODEL"]}' \
  | jq '.data.secretKeys, .data.effectiveSecretKeys'
```

## Signoff record

Copy this block into the PR or validation notes:

```text
Date:
Branch/commit:
Workspace:
Provider path tested: custom only | anthropic | openrouter | both
Probe key: ANTHROPOC_IMPORTANT_KEY
Expected probe sha16:
Observed probe sha16:
Presence-only fallback result:
Runtime config key visible in UI/API: yes/no
Claude Code task started after save: yes/no
Raw probe absent from worker logs: yes/no
Raw provider key not printed: yes/no
Result:
Notes:
```
