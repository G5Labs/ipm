# G5 Development Process

> **Toolchains, dev environment, CI/CD, and testing infrastructure.**
> Cross-references: DESIGN.md §4.2 (two binaries), WORKER_INFRASTRUCTURE.md §1–3 (Kata VMs, base images).

---

## 1. Repository Structure

```
g5/
├── g5-api/                  ← Rust: API server binary
│   ├── Cargo.toml
│   └── src/
├── g5d/                     ← Rust: compute daemon binary (editor + worker modes)
│   ├── Cargo.toml
│   └── src/
├── g5-common/               ← Rust: shared library (types, CRDT, auth, git)
│   ├── Cargo.toml
│   └── src/
├── g5-edge/                 ← Rust: edge gateway (~200 lines)
│   ├── Cargo.toml
│   └── src/
├── g5-analysis/             ← Python: codebase analyzers (semlift lineage)
│   ├── pyproject.toml
│   └── src/
├── client/                  ← TypeScript: React frontend
│   ├── package.json
│   └── src/
├── docker/                  ← Container images
│   ├── Dockerfile.g5-api
│   ├── Dockerfile.g5d
│   ├── Dockerfile.g5d-base  ← worker base image with toolchains
│   └── Dockerfile.edge
├── .devcontainer/           ← VS Code / GitHub Codespaces dev environment
│   ├── devcontainer.json
│   ├── Dockerfile
│   └── docker-compose.yml
├── .github/
│   └── workflows/
│       ├── ci.yml           ← PR checks (lint, test, build)
│       ├── release.yml      ← Tagged release → container images
│       ├── kata-e2e.yml     ← Kata container integration tests
│       └── base-image.yml   ← Weekly worker base image rebuild
├── infra/                   ← Terraform / Pulumi for GKE + GCS + Cloud SQL
│   └── ...
├── Cargo.toml               ← Workspace root (members: g5-api, g5d, g5-common, g5-edge)
├── Cargo.lock
├── rust-toolchain.toml      ← Pinned Rust version
├── deny.toml                ← cargo-deny config (license + advisory audit)
├── .sqlx/                   ← Offline sqlx query cache (checked in)
└── docs/                    ← Design docs (DESIGN.md, AGENTS.md, etc.)
```

### 1.1 Cargo Workspace

The Rust crates form a single Cargo workspace:

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["g5-api", "g5d", "g5-common", "g5-edge"]
resolver = "2"

[workspace.dependencies]
# Shared versions — all crates use workspace = true
tokio = { version = "1", features = ["full"] }
axum = "0.8"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "time", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v7", "serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
clap = { version = "4", features = ["derive", "env"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace", "compression-gzip"] }
jsonwebtoken = "9"
yrs = "0.21"                    # Yjs Rust implementation (Yrs)
reqwest = { version = "0.12", features = ["json"] }
```

### 1.2 Rust Toolchain

```toml
# rust-toolchain.toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy", "rust-analyzer"]
targets = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]
```

Both `x86_64` and `aarch64` targets are included for cross-compilation
of worker images (GKE nodes may be ARM or x86).

### 1.3 Python Analysis Package

The analysis pipeline (codebase analyzers from semlift) is a Python
package managed with `uv`:

```toml
# g5-analysis/pyproject.toml
[project]
name = "g5-analysis"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "tree-sitter>=0.23",
    "tree-sitter-languages>=1.10",
    "pyyaml>=6.0",
    "gitpython>=3.1",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.24",
    "ruff>=0.8",
    "mypy>=1.13",
    "coverage>=7.6",
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.ruff]
line-length = 120
target-version = "py312"

[tool.ruff.lint]
select = ["E", "F", "I", "UP", "B", "SIM", "TCH"]

[tool.mypy]
python_version = "3.12"
strict = true
```

### 1.4 Frontend

React + Vite + TypeScript. See [client/package.json](client/package.json)
for current dependencies. Key additions planned:

| Package | Purpose |
|---------|---------|
| `@tiptap/react`, `@tiptap/pm` | Rich editor (ProseMirror) — FRONTEND_DESIGN.md §11 |
| `y-prosemirror`, `y-monaco` | CRDT bindings for both editor modes |
| `yjs` | CRDT library |
| `monaco-editor` | Code editor + Markdown source mode |
| `markdown-it` + plugins | Canonical Markdown parser |
| `katex` | Math rendering |
| `vitest` | Unit tests |
| `playwright` | E2E browser tests |

---

## 2. Dev Environment

### 2.1 Devcontainer

The primary dev environment is a **devcontainer** (VS Code / GitHub
Codespaces / any devcontainer-compatible tool). This ensures all
contributors have identical toolchains regardless of host OS.

```jsonc
// .devcontainer/devcontainer.json
{
  "name": "g5-dev",
  "dockerComposeFile": "docker-compose.yml",
  "service": "dev",
  "workspaceFolder": "/workspace",

  "features": {
    "ghcr.io/devcontainers/features/rust:1": {
      "version": "latest",
      "profile": "default"
    },
    "ghcr.io/devcontainers/features/python:1": {
      "version": "3.12"
    },
    "ghcr.io/devcontainers/features/node:1": {
      "version": "22"
    }
  },

  "customizations": {
    "vscode": {
      "extensions": [
        "rust-lang.rust-analyzer",
        "ms-python.python",
        "charliermarsh.ruff",
        "ms-python.mypy-type-checker",
        "dbaeumer.vscode-eslint",
        "esbenp.prettier-vscode",
        "tamasfe.even-better-toml",
        "bradlc.vscode-tailwindcss"
      ],
      "settings": {
        "rust-analyzer.cargo.features": "all",
        "rust-analyzer.check.command": "clippy",
        "[rust]": { "editor.formatOnSave": true },
        "[python]": { "editor.formatOnSave": true },
        "[typescript]": { "editor.formatOnSave": true },
        "[typescriptreact]": { "editor.formatOnSave": true }
      }
    }
  },

  "postCreateCommand": "just setup",
  "remoteUser": "vscode"
}
```

### 2.2 Docker Compose (Dev Services)

```yaml
# .devcontainer/docker-compose.yml
services:
  dev:
    build:
      context: ..
      dockerfile: .devcontainer/Dockerfile
    volumes:
      - ..:/workspace:cached
      - cargo-cache:/home/vscode/.cargo/registry
      - target-cache:/workspace/target
    environment:
      DATABASE_URL: postgres://g5:g5@postgres:5432/g5
      REDIS_URL: redis://redis:6379
    depends_on:
      - postgres
      - redis

  postgres:
    image: postgres:17
    environment:
      POSTGRES_USER: g5
      POSTGRES_PASSWORD: g5
      POSTGRES_DB: g5
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"

volumes:
  pgdata:
  cargo-cache:
  target-cache:
```

### 2.3 Justfile (Task Runner)

`just` is the project task runner (like `make` but saner). All common
commands are defined here:

```just
# Justfile — run with `just <recipe>`

# Setup: install tools, run migrations, install deps
setup:
    rustup show                          # ensures rust-toolchain.toml is applied
    cargo install sqlx-cli@0.8.6 cargo-deny cargo-nextest just  # sqlx-cli pinned to the workspace sqlx version
    cd g5-analysis && uv sync --all-extras
    cd client && npm ci
    just db-reset

# === Rust ===
check:
    cargo clippy --workspace --all-targets -- -D warnings

build:
    cargo build --workspace

test:
    cargo nextest run --workspace

test-unit:
    cargo nextest run --workspace --lib

fmt:
    cargo fmt --all -- --check

deny:
    cargo deny check licenses advisories

# === Python (analysis) ===
py-lint:
    cd g5-analysis && uv run ruff check .
    cd g5-analysis && uv run mypy .

py-test:
    cd g5-analysis && uv run pytest -x -q

py-fmt:
    cd g5-analysis && uv run ruff format --check .

# === Frontend ===
ui-lint:
    cd client && npm run lint

ui-build:
    cd client && npm run build

ui-test:
    cd client && npx vitest run

# === Database ===
db-migrate:
    sqlx migrate run --source g5-api/migrations

db-reset:
    sqlx database reset -y --source g5-api/migrations

db-prepare:
    SQLX_OFFLINE=false cargo sqlx prepare --workspace -- --all-targets

# === Full CI check (mirrors GitHub Actions) ===
ci: fmt check deny test py-fmt py-lint py-test ui-lint ui-build ui-test

# === Docker ===
docker-api:
    docker build -f docker/Dockerfile.g5-api -t g5/api:dev .

docker-g5d:
    docker build -f docker/Dockerfile.g5d -t g5/g5d:dev .

docker-base:
    docker build -f docker/Dockerfile.g5d-base -t g5/worker:base .

# === Run locally ===
dev:
    just db-migrate
    cargo run -p g5-api -- --local-workers 2 &
    cd client && npm run dev
```

---

## 3. GitHub Actions CI

### 3.1 PR Checks (`ci.yml`)

Every PR runs the same checks as `just ci`. The workflow uses the
**devcontainer CLI** to build and enter the dev environment, ensuring
CI and local dev are identical.

```yaml
# .github/workflows/ci.yml
name: CI

on:
  pull_request:
  push:
    branches: [main]

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"
  SQLX_OFFLINE: true            # Use checked-in .sqlx/ cache

jobs:
  rust:
    name: Rust (check + test)
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:17
        env:
          POSTGRES_USER: g5
          POSTGRES_PASSWORD: g5
          POSTGRES_DB: g5_test
        ports: ["5432:5432"]
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2
        with:
          cache-on-failure: true

      - name: Format
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: License & advisory audit
        run: |
          cargo install cargo-deny --locked
          cargo deny check licenses advisories

      - name: Test
        run: |
          cargo install cargo-nextest --locked
          cargo nextest run --workspace
        env:
          DATABASE_URL: postgres://g5:g5@localhost:5432/g5_test

      - name: sqlx offline check
        run: |
          cargo install sqlx-cli --version 0.8.6 --locked --no-default-features --features postgres
          SQLX_OFFLINE=false cargo sqlx prepare --check --workspace -- --all-targets

  python:
    name: Python (analysis)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: astral-sh/setup-uv@v4

      - name: Lint
        run: |
          cd g5-analysis
          uv run ruff check .
          uv run ruff format --check .

      - name: Type check
        run: |
          cd g5-analysis
          uv run mypy .

      - name: Test
        run: |
          cd g5-analysis
          uv run pytest -x -q --tb=short

  frontend:
    name: Frontend (lint + build + test)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: client/package-lock.json

      - name: Install
        run: cd client && npm ci

      - name: Lint
        run: cd client && npm run lint

      - name: Type check
        run: cd client && npx tsc --noEmit

      - name: Build
        run: cd client && npm run build

      - name: Test
        run: cd client && npx vitest run

### 3.2 Release (`release.yml`)

Triggered on tags (`v*`). Builds multi-arch container images and pushes
to GCR (or GitHub Container Registry for open-source).

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags: ["v*"]

permissions:
  contents: read
  packages: write

env:
  REGISTRY: ghcr.io
  IMAGE_PREFIX: ghcr.io/${{ github.repository_owner }}/g5

jobs:
  build-and-push:
    name: Build & push images
    runs-on: ubuntu-latest
    strategy:
      matrix:
        include:
          - image: api
            dockerfile: docker/Dockerfile.g5-api
          - image: g5d
            dockerfile: docker/Dockerfile.g5d
          - image: edge
            dockerfile: docker/Dockerfile.edge
          - image: worker-base
            dockerfile: docker/Dockerfile.g5d-base
    steps:
      - uses: actions/checkout@v4

      - uses: docker/setup-qemu-action@v3    # ARM cross-compile
      - uses: docker/setup-buildx-action@v3

      - uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - uses: docker/build-push-action@v6
        with:
          context: .
          file: ${{ matrix.dockerfile }}
          push: true
          platforms: linux/amd64,linux/arm64
          tags: |
            ${{ env.IMAGE_PREFIX }}/${{ matrix.image }}:${{ github.ref_name }}
            ${{ env.IMAGE_PREFIX }}/${{ matrix.image }}:latest
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Sign image (cosign)
        uses: sigstore/cosign-installer@v3
      - run: cosign sign --yes ${{ env.IMAGE_PREFIX }}/${{ matrix.image }}:${{ github.ref_name }}
```

---

## 4. Kata Container Testing

Kata VMs are the production isolation boundary for cloud workers
(WORKER_INFRASTRUCTURE.md §1). Testing Kata-specific behavior in CI
requires **nested virtualization** — the GitHub Actions runner must be
able to create KVM-accelerated VMs inside the CI environment.

### 4.1 The Challenge

Standard GitHub-hosted runners (`ubuntu-latest`) run on shared VMs
with KVM **disabled** — nested virtualization is not available. Kata
containers require a hardware hypervisor (KVM or Cloud Hypervisor),
so they cannot run on default runners.

### 4.2 Solution: Self-Hosted Runners with KVM

We use **self-hosted runners** on bare-metal or nested-virt-enabled VMs
for the Kata E2E test suite:

| Runner Type | KVM Support | Use Case |
|-------------|-------------|----------|
| `ubuntu-latest` (GitHub-hosted) | ❌ No | Rust/Python/frontend unit tests, lint, build |
| `self-hosted` + `kvm` label | ✅ Yes | Kata container E2E, worker lifecycle tests |
| `ubuntu-latest` + `larger` (GitHub Teams/Enterprise) | ✅ Yes (4+ vCPU) | Alternative: GitHub larger runners with nested virt |

**Runner provisioning options:**

1. **GCE instances with nested virtualization:**
   ```bash
   gcloud compute instances create gh-runner-kata \
     --machine-type n2-standard-8 \
     --min-cpu-platform "Intel Haswell" \
     --enable-nested-virtualization \
     --image-family ubuntu-2404-lts \
     --image-project ubuntu-os-cloud
   ```
   Install the GitHub Actions runner agent + Kata Containers runtime.
   Label the runner: `self-hosted`, `linux`, `x64`, `kvm`.

2. **GitHub larger runners (Enterprise):**
   GitHub's larger runners (4+ vCPU, Linux) support `/dev/kvm`. Request
   nested virtualization via the runner group configuration. This avoids
   managing self-hosted infrastructure but requires a GitHub Teams or
   Enterprise plan.

3. **Bare-metal CI runners (Hetzner, Equinix Metal):**
   Cheapest option for dedicated Kata testing hardware. Full KVM support.
   Run the GitHub Actions runner agent as a systemd service.

### 4.3 Kata E2E Workflow (`kata-e2e.yml`)

This workflow exercises the full worker lifecycle inside Kata containers:
pod creation, workspace init, agent execution, CRDT sync, and cleanup.

```yaml
# .github/workflows/kata-e2e.yml
name: Kata E2E

on:
  pull_request:
    paths:
      - "g5d/**"
      - "g5-common/**"
      - "docker/**"
      - "infra/**"
  push:
    branches: [main]
  schedule:
    - cron: "0 6 * * 1"    # Weekly Monday 6am UTC

concurrency:
  group: kata-e2e-${{ github.ref }}
  cancel-in-progress: true

jobs:
  kata-e2e:
    name: Kata container integration tests
    runs-on: [self-hosted, linux, x64, kvm]
    timeout-minutes: 45

    steps:
      - uses: actions/checkout@v4

      - name: Verify KVM access
        run: |
          [ -e /dev/kvm ] || (echo "KVM not available" && exit 1)
          ls -la /dev/kvm

      - name: Install Kata Containers runtime
        run: |
          # Install kata-containers from official packages
          sudo mkdir -p /etc/apt/keyrings
          curl -fsSL https://packages.kata-containers.io/kata-containers.key | \
            sudo gpg --dearmor -o /etc/apt/keyrings/kata-containers.gpg
          echo "deb [signed-by=/etc/apt/keyrings/kata-containers.gpg] \
            https://packages.kata-containers.io/ubuntu $(lsb_release -cs) main" | \
            sudo tee /etc/apt/sources.list.d/kata-containers.list
          sudo apt-get update && sudo apt-get install -y kata-runtime kata-agent
          kata-runtime --version

      - name: Configure containerd for Kata
        run: |
          # Add Kata as a containerd runtime
          sudo mkdir -p /etc/containerd
          cat <<'EOF' | sudo tee /etc/containerd/config.toml
          version = 2
          [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.kata]
            runtime_type = "io.containerd.kata.v2"
            privileged_without_host_devices = true
            [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.kata.options]
              ConfigPath = "/opt/kata/share/defaults/kata-containers/configuration.toml"
          EOF
          sudo systemctl restart containerd

      - name: Set up K3s with Kata RuntimeClass
        run: |
          # K3s: lightweight K8s for CI (uses host containerd)
          curl -sfL https://get.k3s.io | \
            INSTALL_K3S_EXEC="--container-runtime-endpoint unix:///run/containerd/containerd.sock" \
            sh -
          sudo chmod 644 /etc/rancher/k3s/k3s.yaml
          export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

          # Register Kata RuntimeClass
          kubectl apply -f - <<EOF
          apiVersion: node.k8s.io/v1
          kind: RuntimeClass
          metadata:
            name: kata
          handler: kata
          EOF

          # Wait for node ready
          kubectl wait --for=condition=Ready node --all --timeout=120s

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2

      - name: Build g5d binary
        run: cargo build -p g5d --release

      - name: Build worker image
        run: |
          docker build -f docker/Dockerfile.g5d -t g5/g5d:test .
          # Import into K3s containerd
          docker save g5/g5d:test | sudo k3s ctr images import -

      - name: Run Kata E2E tests
        run: |
          export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
          export G5_TEST_IMAGE=g5/g5d:test
          export G5_TEST_RUNTIME_CLASS=kata
          cargo nextest run -p g5d --features kata-e2e -E 'test(kata::)'
        env:
          DATABASE_URL: postgres://g5:g5@localhost:5432/g5_test

      - name: Collect logs on failure
        if: failure()
        run: |
          kubectl get pods -A -o wide
          kubectl describe pods -l app=g5-worker
          kubectl logs -l app=g5-worker --tail=200 || true
          sudo journalctl -u k3s --no-pager --lines=100

      - name: Cleanup
        if: always()
        run: |
          kubectl delete pods -l app=g5-worker --force --grace-period=0 || true
          /usr/local/bin/k3s-uninstall.sh || true
```

### 4.4 What Kata E2E Tests Cover

| Test | What it validates |
|------|-------------------|
| `kata::pod_lifecycle` | Pod creates with `runtimeClass: kata`, starts g5d, responds to health check, shuts down cleanly |
| `kata::workspace_init` | Worker clones repo, hydrates caches, completes workspace init within timeout |
| `kata::agent_execution` | Full ReAct loop: agent receives task, reads files, writes output, calls `transition_task_state` |
| `kata::crdt_sync` | Worker applies CRDT deltas to local filesystem, round-trips changes to mock g5-api |
| `kata::network_isolation` | Worker cannot reach external hosts (only control plane + GCS endpoints) |
| `kata::resource_limits` | Worker respects memory/CPU limits, OOM kills are handled gracefully |
| `kata::nested_containers` | Podman inside Kata VM can pull and run a simple image (DinD validation) |
| `kata::cache_hydration` | Toolchain caches (Rust sccache, npm, pip) are correctly restored from GCS mock |
| `kata::terminal_pty` | Persistent PTY sessions work inside Kata VM (build/test commands execute) |
| `kata::worker_swap` | Task can be migrated between workers: dirty tree preserved via CRDT draft |

### 4.5 Kata Testing Without KVM (Unit-Level)

Not all worker tests require a full Kata VM. The unit test suite mocks
the container runtime and tests g5d logic directly:

```rust
// g5d/src/tests/worker_test.rs — runs on any CI runner, no KVM needed
#[tokio::test]
async fn worker_completes_task() {
    let mock_api = MockG5Api::start().await;
    let workspace = TempWorkspace::new_with_repo("fixtures/small-rust").await;

    let worker = Worker::new(WorkerConfig {
        api_url: mock_api.url(),
        workspace_path: workspace.path(),
        ..Default::default()
    });

    let task = mock_api.assign_task(TaskFixture::uplift_single_file()).await;
    let result = worker.execute_task(task).await.unwrap();

    assert_eq!(result.state, TaskState::Completed);
    assert!(workspace.path().join(".g5/specs").exists());
}
```

These tests run in the standard `cargo nextest run` job on GitHub-hosted
runners. The Kata E2E tests (§4.3) are **additive** — they validate the
same logic inside the real isolation boundary.

---

## 5. Container Images

### 5.1 Image Matrix

| Image | Base | Contents | Size Target |
|-------|------|----------|-------------|
| `g5/api` | `debian:bookworm-slim` | g5-api binary, migrations, CA certs | < 50 MB |
| `g5/g5d` | `g5/worker:base` | g5d binary (editor + worker modes) | < 2 GB (with toolchains) |
| `g5/worker:base` | `ubuntu:24.04` | Language runtimes, compilers, linters, sccache, ripgrep, git | ~1.5 GB |
| `g5/edge` | `debian:bookworm-slim` | Edge gateway binary | < 20 MB |

### 5.2 Build Strategy

All Rust binaries use **multi-stage builds** with a shared builder stage:

```dockerfile
# docker/Dockerfile.g5-api
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY g5-api/ g5-api/
COPY g5-common/ g5-common/
# Dummy build for dependency caching
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build -p g5-api --release && \
    cp target/release/g5-api /usr/local/bin/g5-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/g5-api /usr/local/bin/g5-api
COPY g5-api/migrations/ /app/migrations/
EXPOSE 8080
CMD ["g5-api"]
```

### 5.3 Worker Base Image Pipeline

The worker base image (`g5/worker:base`) is rebuilt weekly or when
the Dockerfile changes. It contains all language runtimes and tools
that user workspaces inherit from:

```yaml
# .github/workflows/base-image.yml
name: Worker Base Image

on:
  push:
    paths: ["docker/Dockerfile.g5d-base"]
  schedule:
    - cron: "0 4 * * 0"    # Weekly Sunday 4am UTC
  workflow_dispatch:

jobs:
  build-base:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/build-push-action@v6
        with:
          context: .
          file: docker/Dockerfile.g5d-base
          push: true
          platforms: linux/amd64,linux/arm64
          tags: |
            ghcr.io/${{ github.repository_owner }}/g5/worker-base:latest
            ghcr.io/${{ github.repository_owner }}/g5/worker-base:${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

---

## 6. Database Migrations

Migrations use **sqlx-cli**. Since P0.2 of the Lean plan there is exactly one
migration file, `backend/g5-api/src/db/migrations/001_lean_baseline.sql`, a
snapshot of the schema that the 289 historical migrations produced. Until the
first production deployment it is edited in place (there is no deployed
data); each PR keeps its edits inside a `-- P<x.y> begin` / `-- P<x.y> end`
section. Normal forward migrations resume after that deployment.

**Offline mode:** sqlx query macros are checked against a cached
`.sqlx/` directory (committed to the repo). CI validates this cache
matches the actual schema:

```bash
SQLX_OFFLINE=false cargo sqlx prepare --check --workspace -- --all-targets
```

When adding a new query or changing the schema:
1. Edit `001_lean_baseline.sql` and recreate the dev database
   (`just dev-down` then `just dev-up`), because the single baseline is
   edited in place rather than appended to.
2. Run `just db-prepare` to regenerate the `.sqlx/` cache.
3. Commit both the baseline file and the updated `.sqlx/` directory.

---

## 7. Testing Strategy

### 7.1 Test Pyramid

```
                    ┌───────────────┐
                    │  Kata E2E     │  ← Self-hosted runners, weekly + path-triggered
                    │  (§4.3)       │
                    ├───────────────┤
                    │  Integration  │  ← GitHub-hosted, every PR
                    │  (API + DB)   │     Postgres service container
                    ├───────────────┤
                    │  Unit tests   │  ← GitHub-hosted, every PR
                    │  (all crates) │     No external deps, mocked I/O
                    └───────────────┘
```

### 7.2 Test Organization

| Layer | Location | Runner | Command |
|-------|----------|--------|---------|
| Rust unit | `g5-*/src/**/*_test.rs` | GitHub-hosted | `cargo nextest run --workspace --lib` |
| Rust integration | `g5-*/tests/` | GitHub-hosted (+ Postgres) | `cargo nextest run --workspace --test '*'` |
| Kata E2E | `g5d/src/tests/kata/` | Self-hosted + KVM | `cargo nextest run -p g5d --features kata-e2e` |
| Python unit | `g5-analysis/tests/` | GitHub-hosted | `uv run pytest` |
| Frontend unit | `client/src/**/*.test.{ts,tsx}` | GitHub-hosted | `npx vitest run` |
| Frontend E2E | `client/e2e/` | GitHub-hosted | `npx playwright test` |

### 7.3 Feature Flags for Test Scoping

```toml
# g5d/Cargo.toml
[features]
default = []
kata-e2e = []    # Guards Kata-specific integration tests
```

```rust
#[cfg(feature = "kata-e2e")]
mod kata {
    #[tokio::test]
    async fn pod_lifecycle() { /* ... */ }
}
```

This ensures `cargo nextest run` on standard runners skips tests that
require KVM/Kata, while the dedicated Kata E2E job enables the feature.

---

## 8. Code Quality Gates

### 8.1 PR Requirements

All PRs must pass before merge:

| Check | Tool | Threshold |
|-------|------|-----------|
| Rust format | `cargo fmt` | Zero diff |
| Rust lint | `cargo clippy -D warnings` | Zero warnings |
| Rust license audit | `cargo deny` | No banned licenses, no known advisories |
| Rust tests | `cargo nextest` | All pass |
| sqlx offline check | `cargo sqlx prepare --check --workspace -- --all-targets` | Cache matches schema |
| Python format | `ruff format --check` | Zero diff |
| Python lint | `ruff check` + `mypy --strict` | Zero errors |
| Python tests | `pytest` | All pass |
| Frontend lint | `eslint` | Zero errors |
| Frontend type check | `tsc --noEmit` | Zero errors |
| Frontend build | `vite build` | Succeeds |
| Frontend tests | `vitest` | All pass |

### 8.2 Recommended (Not Blocking)

| Check | Tool | Notes |
|-------|------|-------|
| Coverage | `cargo llvm-cov`, `coverage.py`, `vitest --coverage` | Track trends, no hard threshold initially |
| Bundle size | Vite build output | Alert on >10% increase |
| Kata E2E | `kata-e2e.yml` | Required for g5d/docker changes; advisory otherwise |

---

## 9. Local Development Workflow

### 9.1 First-Time Setup

```bash
# Clone and enter devcontainer (VS Code)
git clone https://github.com/org/g5.git
cd g5
code .
# VS Code prompt: "Reopen in Container" → Yes

# Or without devcontainer:
just setup
```

### 9.2 Day-to-Day

```bash
# Run everything locally
just dev                    # g5-api + frontend dev server

# Run checks before pushing
just ci                     # mirrors GitHub Actions exactly

# Work on Rust only
just check && just test

# Work on frontend only
cd client && npm run dev    # hot-reload at localhost:5173

# Work on analysis
cd g5-analysis && uv run pytest -x
```

### 9.3 Running with a Real Workspace

```bash
# Start the API server (spawns local workers)
cargo run -p g5-api -- \
  --local-workers 2 \
  --workspace-path /path/to/your/repo

# In another terminal, start the frontend
cd client && npm run dev

# Open http://localhost:5173
```

---

## 10. Dependency Management

### 10.1 Rust

- **Version pinning:** `Cargo.lock` is committed (binary project).
- **Audit:** `cargo deny` runs on every PR — checks licenses (allow-list)
  and RustSec advisories.
- **Updates:** Dependabot or `cargo update` weekly. Review changelogs for
  major bumps.

### 10.2 Python

- **Lock file:** `uv.lock` is committed (generated by `uv lock`).
- **Audit:** `uv run pip-audit` in CI (TODO: add to workflow).
- **Updates:** Dependabot for `pyproject.toml`.

### 10.3 Frontend

- **Lock file:** `package-lock.json` is committed.
- **Audit:** `npm audit` in CI.
- **Updates:** Dependabot for `package.json`.

---

## 11. Secrets & Configuration

### 11.1 Development (Local / Devcontainer)

| Variable | Default | Notes |
|----------|---------|-------|
| `DATABASE_URL` | `postgres://g5:g5@localhost:5432/g5` | Docker Compose Postgres |
| `REDIS_URL` | `redis://localhost:6379` | Docker Compose Redis |
| `G5_LLM_PROVIDER` | `anthropic` | or `openai`, `openrouter` |
| `ANTHROPIC_API_KEY` | (none) | Required for agent features |
| `OPENAI_API_KEY` | (none) | Alternative LLM provider |

### 11.2 CI

| Secret | Scope | Notes |
|--------|-------|-------|
| `GITHUB_TOKEN` | Automatic | Container registry push |
| `GCP_SA_KEY` | Kata E2E only | GCE runner provisioning (if using GCE self-hosted) |

### 11.3 Production

Managed via GKE Workload Identity + Secret Manager. Not checked into the
repo. See WORKER_INFRASTRUCTURE.md §1.3 for pod-level env configuration.

---

## 12. Release Process

### 12.1 Versioning

SemVer: `v{major}.{minor}.{patch}`. Tags trigger the release workflow.

### 12.2 Release Checklist

1. `just ci` passes on `main`.
2. Update `CHANGELOG.md` with notable changes.
3. Bump versions in `Cargo.toml` (workspace), `package.json`, `pyproject.toml`.
4. Create and push tag: `git tag v0.1.0 && git push --tags`.
5. Release workflow builds + pushes images.
6. Deploy to staging → smoke test → promote to production.

### 12.3 Image Tags

| Tag | Meaning |
|-----|---------|
| `v0.1.0` | Immutable release tag |
| `latest` | Most recent release (mutable) |
| `main-{sha}` | Tip of main (for staging) |
| `pr-{number}` | PR preview (optional) |

---

## Appendix: Tool Versions

| Tool | Version | Managed By |
|------|---------|-----------|
| Rust | stable (latest) | `rust-toolchain.toml` |
| Python | 3.12 | devcontainer feature |
| Node.js | 22 LTS | devcontainer feature |
| uv | latest | `astral-sh/setup-uv` action / devcontainer |
| PostgreSQL | 17 | Docker Compose / CI service |
| Redis | 7 | Docker Compose / CI service |
| Kata Containers | 3.x | Self-hosted runner install |
| K3s | latest stable | Kata E2E workflow |
| just | latest | `cargo install just` |
| sqlx-cli | 0.8.6 | `cargo install sqlx-cli@0.8.6` |
| cargo-nextest | latest | `cargo install cargo-nextest` |
| cargo-deny | latest | `cargo install cargo-deny` |
