# Just commands in G5 Lean

Audited on 2026-09-21 after the workspace-VM cutover. The root Justfile
contains 101 recipes. They have all been checked for rendering, shell syntax,
concrete script entry points and explicitly named Rust test targets. This is not
a claim that every integration suite or cloud operation has been executed.

## Local application

The replacement for the old `cd generation` sequence is:

```sh
cd /Users/zliu/Workspace/G5-Lean
just dev-down
just dev-up
```

The application listens at <http://localhost:3000> and the API at
<http://localhost:7500>. Settings are loaded from the ignored root `.env`.
`dev-down` retains its original `down -v` behavior: it removes the development
Lean volumes and their local data. For normal code updates while keeping that data,
use `just dev-restart`, `just dev-restart-backend`, or
`just dev-restart-frontend`.

Lean uses `g5-lean_pgdata_dev`, `g5-lean_api_repos`, and
`g5-lean_worker_workspace` for Docker storage. The existing `g5-dev_pgdata_dev`
and `g5-dev_api_repos` volumes were preserved. The existing database rejected
migration 294 because its stored checksum differs from both Lean and the
byte-identical original-reference migration, so Lean starts with fresh storage.
Seed helpers and local E2E consumers use the same new volume names.

The first build after relocation rebuilds the common Rust dependency cache.
Linux microVM development requires KVM and the pinned Cloud Hypervisor platform;
run `just worker-host-setup` to build and stage the checkout's runtime. Apple Silicon
uses explicit local process mode: set `G5D_ISOLATION_MODE=process` and
`G5D_ALLOW_PROCESS_ISOLATION=1` for both setup and `dev-up`. Provider-backed tasks
still require the corresponding credentials. See [local setup](reference/SETUP.md).

## Repairs

- Corrected common-base Dockerfile discovery and fingerprinting of relocated
  crate manifests, build scripts and test targets. Restored Dockerfile frontend
  image references (`docker/dockerfile:1`); these are registry names, not folders.
- Aligned `ci-docker` workspace, Cargo registry and target-cache mounts with its
  actual CI-builder image. Cache versions and image-input hashes now cover the
  relocated workspace. CI logs default to `outputs/ci-logs` under this checkout.
- `e2e-up` now builds the worker base and explicitly rebuilds the worker from
  current source. Previously it could fail on a fresh Docker daemon or reuse a
  stale worker image.
- Fixed doubled `cargo test` in `test-all-docker` and `test-isolation-docker`:
  the test image already has a Cargo test entry point. This bug predates Lean.
- Replaced the stale Rust 1.93 image in `test-cross` with the shared Rust base,
  which supplies the repository's pinned toolchain.
- Fixed `ar-images` shell substitution. Replaced the obsolete `g5-ci` trigger
  with the declared Rust and E2E lanes, exposed individually as `cb-ci-rust`
  and `cb-ci-e2e`. `cb-ci` dispatches both. Cloud Build commands specify their
  region, overridable with `G5_CLOUD_BUILD_REGION`.
- `helm-lint` and `helm-template` use installed Helm or a Docker fallback matching
  the deploy pipeline's Helm 3.21.4. Their default worker image is an explicit
  local preview placeholder because deployment values require pipeline injection.
  Pass a real image reference as the second lint argument or third template
  argument when needed. Rendered defaults are for inspection, not deployment.
- `db-migrate`, `db-prepare` and `scripts/run-integration.sh` load root `.env` and
  otherwise default to the Compose dev stack's own port (Postgres 5432),
  which also matches the g5d-g5dev guest's native test-DB helper. A laptop that shifts
  the stack (as `.env.example` does with `G5_PORT_POSTGRES=5433`) must keep that value
  in `.env`, since without it the stack itself publishes 5432. Manual E2E instructions
  use API 7500 and application 3000.
- **Destructive migrations need the app stopped first.** `002_remove_workspace_search.sql`
  drops tables and the pgvector extension, so a prior-version `g5-api` or
  `g5-daemon` container left running against the upgraded schema would
  keep querying objects that no longer exist. On an existing local stack,
  upgrade in this order:

  ```sh
  docker compose -p g5-dev -f infra/docker/docker-compose.dev.yml stop g5-api g5-daemon
  docker compose -p g5-dev -f infra/docker/docker-compose.dev.yml up -d --no-build --no-deps --no-recreate --remove-orphans postgres
  just db-migrate
  just dev-restart-backend  # restarts g5-api and g5-daemon on the new schema
  ```

  Orphan cleanup removes a retained `g5-api-indexer` container before migration;
  it keeps Postgres available while the API and current daemon stay stopped.
  A fresh `just dev-up` already migrates before starting the services, and the
  self-hosted `just upgrade` already calls `stop_app_services` before
  `db-migrate`, so both paths satisfy this ordering without extra steps.
  Deployment itself is out of scope for this change.
- Fixed the value-loop catalog path and platform-issue Terraform path.
  `fmt` now covers both Rust and the frontend, matching `fmt-check`.
- `demo-codex` validates its source repository and required commits before any
  teardown. Set `G5_CODEX_DEMO_SOURCE_REPO` to supply that external repository.

## Seed this Lean checkout into localhost

With the dev stack already running:

```sh
just seed-lean-workspace
```

Creates **G5-Lean** in **g5labs**, owned by the existing `demo@g5labs.ai` user.
Imports the local committed `HEAD` and its ancestry into an independent code repo,
including `.g5/ontology`, plus an empty docs repo. Uncommitted edits and untracked
files are excluded. No push, stack reset or VM start is performed. Browse at
`http://localhost:3000/g5labs/G5-Lean`; select **Start** when compute is needed.

The command refuses an existing name. To deliberately delete a previous seed and
create a fresh one, use `just seed-lean-workspace --replace WORKSPACE_UUID`; this
accepts only the exact owner-held workspace created by this seeder. Normal G5
busy/deletion rules apply. Replacement is not atomic: a failure after deletion
may require a normal create retry. Old data follows ordinary product retention.
Use `--name OTHER_NAME` for a separate workspace without replacement.

`--source PATH` selects another local committed checkout; `--org`, `--email` and
`--api-url` select an existing local identity/API. Stack identity follows `.env`
and exported `G5_COMPOSE_*`/port overrides through the shared helper. Requires
Python 3, Git, Docker and dev test authentication; shallow sources and submodules
are rejected. The target source must contain `.g5/ontology/root.g5.md`.

The seed creates exactly one ordinary standalone workspace with dedicated repository
UUIDs, no seed-source parent and no special Base status. You can fork G5-Lean
normally; those forks can compare/rebase/merge against G5-Lean. The standalone
root itself has no parent comparison or merge destination. It publishes DB membership
only after Git installation, and verifies committed ontology through the API.
If installation/publication fails, printed repository/workspace IDs identify the
partial attempt; no success is claimed and uncertain imported data is retained.
Existing seed commands are unchanged.

## Retired behavior and remaining exceptions

| Command or behavior | Finding and disposition |
| --- | --- |
| The old `g5-ci` trigger used by `cb-ci` | Stale in the original Justfile. Replaced with `g5-ci-rust` and `g5-ci-e2e`, matching Terraform. |
| `cb-deploy staging` / `cb-deploy prod` | Referenced nonexistent branch-deploy triggers in the checked-in infrastructure. These forms now fail with a migration message before contacting GCP. Use the dev deployment and the existing `g5-promote-staging` / `g5-promote-prod` pipeline with immutable image references and its approval process. |
| `surface-sync` | Retired with the shipped agent profiles. Workspace agents now receive the repository-root `AGENTS.md` and `.agents/skills`. |
| `demo-codex` | Requires the external `codex_g5` repository and historical commits `7373aff` / `0934045`; absent from both checkouts. Keep only if this specific historical demo is still wanted. It now reports the missing input before resetting the stack. |
| `worker-process`, `telemetry-shim-check`, `test-live-cache`, `crdt-smoke`, `demo-redeploy-ui`, `demo-redeploy-ui-auto`, `test-live-llm` | Retired with per-task workers, the build telemetry shim, CRDT and Native/ReAct. Use the workspace host, retained unit/integration slices, explicit Save tests, `dev-restart-frontend`, and `test-live-e2e` as applicable. |
| `tinydb-seed` | Requires `external_repos/tinydb/tinydb_src`, absent from both checkouts. Supply that source checkout to the seed script, or use `tinydb-commit0-seed` for the different, retained commit-zero fixture. |
| `prepare-g5-workspace` / `seed-g5-workspace` | The default self-hosting fixture expects the separate `g5_docs` repository and G5 ontology/preview metadata removed from Lean. Preparation supports external source repositories; use `seed-lean-workspace` for the current Lean checkout. |
| `seed-compliance-workspace`, `seed-timsystem-workspace`, `seed-bankworkflow-new-ontology-workspace` | Require external customer fixture repositories, absent at the default locations in both checkouts. Their source-path arguments/environment overrides remain supported; absence of private fixtures does not make the utilities inherently obsolete. |
| `fmt-check`, `ci` | The relocation audit found frontend formatting differences in 148 inherited files whose contents matched the original checkout. These checks are valid; formatting debt is not a reason to retire them. The file changed by relocation was formatted. Docker CI separately checks Rust formatting and frontend ESLint; it does not run frontend Prettier. See [the retirement validation report](dead-code-retirement.md) for current Docker CI results. |

Cloud recipes still require the existing GCP projects, configured authentication,
Terraform backend and Kubernetes context. Trigger names were checked against the
retained Terraform definitions; no live cloud trigger or infrastructure change
was made. This local checkout has no configured Git remote. Cloud Build triggers
act on their configured repository and branch; they do not upload this local,
uncommitted Lean code. Publishing Lean to the cloud requires a Lean remote and
corresponding trigger configuration. These recipes also retain the original G5
Terraform backend; they do not provision a separate G5-Lean cloud environment.

## Validation

- All 101 recipes render and pass Bash syntax checks; directly referenced
  script entry points exist and named Rust test targets resolve.
- The E2E Rust crate passes `cargo check -p g5-e2e --tests --offline` after the
  development-volume changes.
- Local layout check passes for 26 Cargo members, 474 embedded Rust inputs and
  253 Docker inputs, including Dockerfile frontend references.
- Ten command-dispatch regression tests pass and cover Cloud Build, Artifact Registry,
  Terraform backend selection, Docker test arguments, demo preflight, development
  teardown order, fresh E2E worker builds, isolated Lean storage and warm-base
  invalidation. External operations are stubbed.
  Five checks were confirmed to fail against the pre-fix implementation.
- The full Python unittest suite passes: 214 tests. The frontend user-guide
  tests pass: eight tests.
- Common-base lock tests pass. Worker-provider and development build-profile checks pass.
- Helm lint and template pass for all three environments using preview values.
- Live `just dev-up` completed successfully with the final Dockerfiles and Lean
  volumes. Postgres, API, edge and OAuth are healthy; the daemon is running.
  The fresh database has 289 successful migrations, through version 298.
- Headless Chromium loaded localhost:3000 with HTTP 200 and no page errors;
  the API health endpoint at localhost:7500 returned HTTP 200. This verifies the
  public landing page, not a full authenticated user workflow.
- Native `worker-host-setup` completed, including a real microVM smoke boot.
  The final guest binary was refreshed and the native host registered with the
  local API. Provider-backed task execution was not exercised.
- `dev-down` dispatch was tested with stubs. A real Compose dry run confirms
  teardown selects Lean volumes and excludes the preserved original volumes;
  the running stack was left up.

Useful repeatable checks:

```sh
just commands-check
just layout-check
python3 -m unittest discover -s scripts -p test_just_commands.py -v
just dev-build-profile-check
just helm-lint
just helm-template
```

## Complete recipe inventory

Use `just --list` for arguments and `just --show <recipe>` for the exact behavior.
Live suites require their documented services, seeded data and provider keys;
virtualization suites additionally require the appropriate host capabilities.

| Group | Recipes |
| --- | --- |
| Local development | `dev`, `dev-up`, `dev-stack-up`, `dev-down`, `dev-rebuild`, `dev-restart`, `dev-restart-backend`, `dev-restart-frontend`, `eval-end-user-fresh` |
| Workspace hosts | `worker-host`, `worker-host-setup`, `worker-host-start`, `worker-host-stop` |
| Build and setup | `setup`, `build`, `build-debug`, `check`, `docker-build`, `docker-build-api`, `docker-build-base`, `docker-build-common-base`, `docker-build-edge`, `docker-build-g5d` |
| Code and repository checks | `default`, `layout-check`, `commands-check`, `fmt`, `fmt-check`, `lint`, `ci`, `ci-fmt`, `worker-provider-env-check`, `dev-build-profile-check`, `gen-model-catalog` |
| Native Rust tests | `test`, `test-compile`, `test-crate`, `test-doc`, `test-integration`, `test-one`, `test-unit`, `test-isolation-native`, `test-live-e2e` |
| Docker and CI tests | `test-all-docker`, `test-cross`, `test-g5d-docker`, `test-isolation-docker`, `ci-docker`, `ci-docker-5`, `ci-docker-6`, `ci-docker-7`, `ci-docker-nfs-init`, `value-loop-matrix` |
| System and browser tests | `api-smoke`, `dev-e2e`, `dev-playwright`, `e2e-down`, `e2e-live`, `e2e-live-repo-hint-gate`, `e2e-run`, `e2e-up`, `test-e2e`, `test-playwright`, `provided-agents-manual-e2e` |
| Demo and fixture management | `demo-codex`, `demo-down`, `demo-seed`, `demo-start`, `demo-up`, `prepare-g5-workspace`, `seed-bankworkflow-new-ontology-workspace`, `seed-compliance-workspace`, `seed-g5-workspace`, `seed-timsystem-workspace`, `test-seed`, `tinydb-commit0-seed`, `tinydb-seed` |
| Database | `db-migrate`, `db-prepare` |
| Infrastructure and charts | `helm-lint`, `helm-template`, `gke-auth`, `deploy-status` |
| Cloud Build and operations | `cb-ci`, `cb-ci-rust`, `cb-ci-e2e`, `cb-deploy`, `cb-list`, `cb-logs`, `ar-images`, `reset-dev-environment` |

`docker-ci` is an alias of `ci-docker`; phase 5b runs Claude/Codex protocol tests, including `g5d-agent`.
