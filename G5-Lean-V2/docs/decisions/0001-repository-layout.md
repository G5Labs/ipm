# Repository layout

Accepted by the user on 2026-09-12. This change relocates existing code and its
build inputs while retaining product functionality and crate identities.

| Previous location | Current location |
| --- | --- |
| `generation/Cargo.toml`, `Cargo.lock`, `Justfile` | Repository root |
| `generation/.cargo/`, `.config/`, `.sqlx/` | Repository root |
| `generation/g5*/` | `backend/g5*/` |
| `generation/client/` | `frontend/` |
| `generation/g5-api/provided_agent_assets/` | `assets/agents/` |
| `generation/docker/` | `infra/docker/` |
| `generation/infra/` Terraform inputs | `infra/terraform/` |
| `generation/infra/helm/`, `packer/`, `runbooks/` | `infra/helm/`, `packer/`, `runbooks/` |
| `generation/cloudbuild-*.yaml` | `infra/ci/` |
| `generation/self-hosted/`, `libkrunfw/` | `infra/self-hosted/`, `libkrunfw/` |
| `generation/local-setup-artifacts/` | `infra/local-setup/` |
| `generation/scripts/` | Merged into `scripts/` |
| `generation/e2e/` | `tests/e2e/` |
| `generation/test-fixtures/` | `tests/fixtures/` |
| `generation/designs/`, `docs/`, historical setup/design files | `docs/reference/` |
| `G5-logo.png` | `assets/branding/G5-logo.png` |

The root is the build context and task working directory. Product assets are
separate from development instructions. Historical documents are retained as
reference, with their original content rather than presenting old designs as
the new architecture.

Tests that previously embedded deleted development-profile skills now inspect
the retained shipped profiles. Two route-authorization JSON fixtures were
restored from the original checkout into `tests/fixtures/security/`. A minimal
`.g5/workspace.yaml` retains the local Docker build recipe without the original
repository or external docs-repository bindings.

Feature removal, crate consolidation, database redesign and deployment changes
are deferred until their designs are provided. Existing operational recipes
remain available; this relocation does not deploy them.
