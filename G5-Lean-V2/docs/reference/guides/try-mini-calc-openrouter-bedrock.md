# Try mini-calc with Claude Code

This is the quick path for developers who want to run the local G5 dev stack,
seed the small **mini-calc** workspace, and try the Provided-Agent **Claude Code**
provider safely.

Claude Code uses local subscription/OAuth auth by default. If you want a Claude
Code task to use provider API keys, configure those keys as task-scoped Runtime
Config secrets by their exact environment variable names. There is no
worker-level Claude Code backend selector for managed OpenRouter or Bedrock
keys because the SDK query environment can be visible to agent tool execution.

> This doc is for local/manual experimentation. Do not commit real API keys.

## 0. Start from `generation/`

```bash
cd generation
cp .env.example .env   # if you do not already have one
```

Optional non-secret model pins can still be set in `.env`.

## 1A. Choose the Claude Code profile

The managed worker bundles one built-in Claude Code profile:

```dotenv
# full
G5_CLAUDE_CODE_PROFILE=full

# Optional exact path override. Leave empty unless you want to provide your own profile assets.
G5_CLAUDE_CODE_ASSETS_DIR=
```

Use `full` for the default, fully featured profile tree with hooks, skills,
examples, and MCP guidance. If `G5_CLAUDE_CODE_ASSETS_DIR` is non-empty, it is
treated as an exact custom path override and the profile selector is not used
for asset resolution.

## 2. Start the dev stack

```bash
just dev-up
```

When it finishes:

- App: <http://localhost:3000>
- API: <http://localhost:7500>
- Worker: one managed Provided-Agent worker

## 3. Seed the mini-calc workspace

```bash
just test-seed
source .env.test
```

This creates an `e2e-test` workspace containing the `mini-calc` code repo and
docs repo.

Refresh the worker volume metadata so the running worker is pointed at the
seeded mini-calc workspace:

```bash
./scripts/init-worker-volume.sh "$G5_E2E_TEST_WORKSPACE_ID"

docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo up -d --build --scale g5d-worker=1 g5d-worker
```

## 4. Sanity-check the worker

```bash
docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo ps

docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo logs --tail=120 g5d-worker
```

In the logs, look for the worker registering and advertising provider
availability. If Claude Code is unavailable, check the profile/assets settings
and restart `g5d-worker`.

## 5. Try it in the UI

1. Open <http://localhost:3000>.
2. Log in via the local OAuth stub.
3. Open the `e2e-test` workspace.
4. Open the mini-calc code repo or chat/task panel.
5. Choose **Claude Code** as the provider if the UI gives you a provider picker.
6. Try one small request.

Good starter prompts:

```text
In the mini-calc repo, add support for the % modulo operator.
Update the relevant .semlift spec first, then update parser/evaluator code and tests.
```

```text
In mini-calc, add a simple square function named square(x).
Keep the change small and update the spec and tests.
```

```text
Inspect mini-calc and explain how parser, engine, and formatter fit together.
Do not modify files yet.
```

## 6. Switching profiles or model pins

Edit `.env`, then recreate the worker. This applies to model pins and profile
changes such as `G5_CLAUDE_CODE_PROFILE=full` or a custom
`G5_CLAUDE_CODE_ASSETS_DIR`:

```bash
docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo up -d --build --scale g5d-worker=1 g5d-worker
```

You do **not** usually need to reseed mini-calc unless you want a clean
workspace.

## 7. Useful cleanup

Stop everything and remove dev volumes:

```bash
just dev-down
```

Start fresh:

```bash
just dev-up
just test-seed
source .env.test
./scripts/init-worker-volume.sh "$G5_E2E_TEST_WORKSPACE_ID"
docker compose -p g5-dev -f docker/docker-compose.dev.yml \
  --profile demo up -d --build --scale g5d-worker=1 g5d-worker
```

## Troubleshooting

### Claude Code does not appear as available

- Make sure `G5_CLAUDE_CODE_PROFILE` is `full`, unless you intentionally set
  `G5_CLAUDE_CODE_ASSETS_DIR` to a custom profile path.
- For API-key-backed Claude Code tasks, add the provider key as a task-scoped
  Runtime Config secret with the exact env name the agent should see.
- Recreate the worker after changing `.env`.

### The UI opens, but I cannot find mini-calc

Run:

```bash
just test-seed
source .env.test
```

Then open the `e2e-test` workspace.

### A task finishes but workspace files do not change

Task/job changes are isolated until published. Use the UI merge/publish flow for
job-to-workspace publication when available.
