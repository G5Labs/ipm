# G5 Lean

G5 Lean starts from the G5 platform with a simpler repository layout. Product
features and Rust crate boundaries are retained during this relocation. The
feature keep/remove list and replacement backend design will be supplied
separately.

## Layout

| Path | Responsibility |
| --- | --- |
| `frontend/` | React/Vite application, frontend unit tests and browser E2E tests |
| `backend/` | Existing Rust application and library crates |
| `AGENTS.md` | Repository development instructions |
| `.agents/AGENTS.md`, `.agents/skills/` | Workspace agent instructions and the maintained skills packaged into the worker runtime |
| `assets/branding/` | Product branding assets |
| `infra/docker/` | Images and Compose stacks; build context is the repository root |
| `infra/qa/`, `infra/terraform/modules/` | QA Terraform roots and the modules they source |
| `infra/helm/`, `infra/packer/` | Kubernetes charts and worker image definitions |
| `infra/ci/` | Cloud Build configurations |
| `infra/self-hosted/`, `infra/guest/` | Host installers and pinned guest kernel/init assets |
| `scripts/`, `tools/` | Task automation and standalone helper programs |
| `tests/e2e/`, `tests/fixtures/` | Rust system-test crate and shared fixtures |
| `docs/` | Lean decisions and current documentation |
| `docs/reference/` | Reference designs and local setup documentation |

The repository root is the Cargo workspace and Docker build context. Crate-local
tests and database migrations remain with their current owners. SQLx's offline
query cache is in `.sqlx/`.

## Local commands

Run these from the repository root:

```sh
just                         # List the retained commands
just dev-up                  # Start the app at localhost:3000
just commands-check          # Audit every recipe without running services
python3 scripts/check-layout.py
SQLX_OFFLINE=true cargo check --workspace --all-targets --offline
SQLX_OFFLINE=true cargo test -p <crate> --lib
npm --prefix frontend ci
npm --prefix frontend run build
npm --prefix frontend test
```

For focused PR validation on a laptop, start with `just validation-preflight` and `just validation-recommend main`. Use `just ci-docker-prepare` to retain a candidate stack for Phase 5/6/7 reruns, and `just ci-docker-down` when finished. See [local validation](docs/local-validation.md) for receipts, reuse checks and retention controls.

`just dev-up` starts the development stack with dedicated `g5-lean_*` Docker
volumes, preserving the original G5 local storage. Local settings live in
the ignored root `.env`; `.env.example` documents the available settings.
The local native worker host starts with cloud provisioning disabled by default. Set `ENABLE_CLOUD_FEATURES=true` only when testing GCP provisioning with a configured cloud provider.
`just dev-up` also resolves a digest-pinned workspace image. The API assigns it to new organizations and adopts it for existing organizations with no image policy, so their workspaces can start without adding image settings to `.env`.
The native worker host starts without an LLM credential, so workspace boot and the terminal work without adding one to `.env`. Agent turns receive provider credentials from the API's Runtime Config or provider account settings when they run. For local agent testing without LLM calls, set `G5_CLAUDE_CODE_FAKE=1` for that command. Startup does not open an authentication flow or write to `.env`.
MicroVM execution requires Linux x86_64/KVM and the pinned Cloud Hypervisor platform artifacts. macOS development requires explicit process isolation: `G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 just dev-up`. See [local setup](docs/reference/SETUP.md).
Live E2E tests require the corresponding stack and seeded test data.

See [preview isolation](docs/preview-isolation.md) for the per-preview host requirements, what happens without a preview origin, and the remaining deployment isolation work.

The `full-stack` and `full-stack-no-worker` previews seed two workspaces in
G5 Labs: `preview-issue-only` (a primary issue without a PR) and
`preview-issue-and-pr` (a primary issue with an open G5 PR). Both include
Alex/Jordan issue conversations, acceptance criteria and review checklists.
They use G5's tracker, so no GitHub connection is needed. To add the examples
to an already running dev stack, run `python3 scripts/seed-preview-workspaces.py`
(`--api-url http://localhost:17500` for the preview stack). Repeating the seed
reuses its workspaces, issues, comments and PRs without resetting review work.

For an image build, use a root context, for example:

```sh
docker build -f infra/docker/Dockerfile.edge .
docker compose -f infra/docker/docker-compose.dev.yml config --quiet
```

For frontend-only commands, working directly in `frontend/` is also supported.
Historical references to `cd generation` no longer apply to this checkout.

See [the Just command audit](docs/just-commands.md) for repaired commands,
prerequisites and retirement candidates.

See [the relocation decision](docs/decisions/0001-repository-layout.md) for the
path mapping and [validation notes](docs/relocation-validation.md) for checks
and remaining limitations. The original checkout is retained separately at
`/Users/zliu/Workspace/G5-original-reference` as a restoration reference.

## Workspace review UX prototype

The isolated [review prototype](docs/review-flow.md) uses the current frontend workbench
components with sample data. From `frontend/`, run `npm ci` then `npm run dev:review-mock`
and open `http://127.0.0.1:8792/review-mock.html`. Demo scenarios switch owner/reviewer/viewer;
the workspace tabs provide submitted and pre-PR examples. Reload resets local data.

`npm run build:review-mock` produces `frontend/review-mock-dist/`, a static preview that
can be served independently. It does not mount production routes or call QA APIs.
Run focused tests with `npm test -- src/prototypes/review`.
