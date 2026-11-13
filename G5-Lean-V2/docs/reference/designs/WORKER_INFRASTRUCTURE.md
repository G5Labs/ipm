# Worker Infrastructure Design

## Plan of Record: VM-Isolated Containers with Shared GCS Build Cache

### Executive Summary

Our execution model is **VM-isolated containers** with a **shared, workspace-scoped build cache backed by Google Cloud Storage (GCS)**.

| Principle | Implementation |
|-----------|----------------|
| **Isolation boundary** | One agent session runs inside a VM-isolated container (GKE + Kata in hosted mode; equivalent Kata-enabled K8s or VM isolation on customer hardware) |
| **Reproducibility** | Environments defined by base image (often built from `.devcontainer`) plus deterministic toolchain configuration |
| **Performance** | Builds accelerated via local warm caches and shared remote caches stored in GCS |
| **Security** | Credentials scoped to per-workspace GCS prefixes; no shared NFS or broad IAM grants |

---

## 0. Design Principles

Four optimization criteria drive every architectural decision in G5. When trade-offs arise, they are resolved in this priority order:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    G5 Optimization Criteria                                  │
│                                                                              │
│  P1. EDITING MUST FEEL NATIVE                                               │
│  ─────────────────────────────                                               │
│  The editing experience must be fast and feel like a native IDE. Keystrokes │
│  must never block on network. Syntax highlighting, file tree, search, and   │
│  git operations work without a worker. Monaco + TextMate grammars load     │
│  instantly. CRDT edits propagate in <50ms. The browser is never waiting    │
│  for a server round-trip to show a character on screen.                     │
│                                                                              │
│  This is why: editor has its own git checkout, CRDT is the edit layer,     │
│  **Open in IDE** provides IntelliSense, and the draft model decouples      │
│  editing from persistence.                                                  │
│                                                                              │
│  P2. AGENTS NEED A REAL FILESYSTEM AND COMPUTE                              │
│  ─────────────────────────────────────────────                               │
│  Agents must have a filesystem to work with and compute to execute          │
│  commands on. They need to read code, write files, run builds, execute     │
│  tests, and use a shell — the same tools a                                  │
│  human developer would use. This cannot be faked with API-only access.     │
│                                                                              │
│  This is why: workers have full git worktrees, toolchain caches, PTYs,     │
│  and shell access. The one-binary architecture (g5d) gives every worker    │
│  a complete development environment.                                        │
│                                                                              │
│  P3. AGENT WORK IS ISOLATED AND COMMITTED                                   │
│  ─────────────────────────────────────────                                   │
│  Agents must be isolated from each other, and their changes must be        │
│  committed through the same draft→commit flow as human edits. No agent    │
│  can corrupt another's work. No change reaches the repository without      │
│  an explicit commit. This makes agent work auditable, reversible, and      │
│  reviewable.                                                                │
│                                                                              │
│  This is why: each task gets its own branch + worktree (local) or Kata     │
│  VM (cloud). Agent edits flow through the CRDT draft. The [Commit]         │
│  button is the only persistence boundary. Checkpoints + rollback provide   │
│  safety nets.                                                               │
│                                                                              │
│  P4. COLLABORATIVE EDITING OF SPECIFICATIONS IS A KEY VALUE PROPOSITION     │
│  ───────────────────────────────────────────────────────────────────────     │
│  Multiple users will collaboratively edit and comment on the ontology,      │
│  specifications, and task descriptions that drive agent behavior. This      │
│  is not a nice-to-have — it is a core differentiator. The system must     │
│  support real-time collaborative editing (CRDT), shared draft state,       │
│  cross-branch comment visibility, and persistent chat histories that        │
│  capture the full decision-making trail.                                    │
│                                                                              │
│  This is why: Yjs CRDT for drafts, server-authoritative state, node-level │
│  comments visible workspace-wide, line-level comments indexed cross-branch, │
│  and all chat sessions persisted as browsable, linkable artifacts.          │
│                                                                              │
│  ─────────────────────────────────────────────────────────────────────────  │
│                                                                              │
│  Resolution order: when principles conflict, P1 > P2 > P3 > P4.           │
│  Example: if collaborative sync would add latency to keystrokes (P4 vs    │
│  P1), we batch the sync, not slow the keystroke.                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

> **API Path Convention (see API_AND_STORAGE.md §3.0)**
>
> All HTTP/WS endpoints in this document follow the versioning scheme:
> - **`/api/v1/…`** — Public endpoints (browser → editor, orchestrator → editor)
> - **`/api/internal/…`** — Internal endpoints (worker → editor, editor → worker)
>
> Inline code examples and ASCII diagrams may omit the prefix for brevity.
> The canonical prefix for each endpoint is specified in §10.13 (Session API)
> and §11.9 (Instance API).

## 1. High-Level Architecture

### 1.1 Execution Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              GKE Cluster                                     │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                     Kata Runtime (VM Isolation)                      │    │
│  │                                                                      │    │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐     │    │
│  │  │   Agent Pod A   │  │   Agent Pod B   │  │   Agent Pod C   │     │    │
│  │  │  (Workspace X)  │  │  (Workspace X)  │  │  (Workspace Y)  │     │    │
│  │  │                 │  │                 │  │                 │     │    │
│  │  │ runtimeClass:   │  │ runtimeClass:   │  │ runtimeClass:   │     │    │
│  │  │   kata          │  │   kata          │  │   kata          │     │    │
│  │  │                 │  │                 │  │                 │     │    │
│  │  │ ┌─────────────┐ │  │ ┌─────────────┐ │  │ ┌─────────────┐ │     │    │
│  │  │ │ Local Cache │ │  │ │ Local Cache │ │  │ │ Local Cache │ │     │    │
│  │  │ │  /cache/*   │ │  │ │  /cache/*   │ │  │ │  /cache/*   │ │     │    │
│  │  │ └──────┬──────┘ │  │ └──────┬──────┘ │  │ └──────┬──────┘ │     │    │
│  │  └────────┼────────┘  └────────┼────────┘  └────────┼────────┘     │    │
│  │           │                    │                    │              │    │
│  └───────────┼────────────────────┼────────────────────┼──────────────┘    │
│              │                    │                    │                    │
└──────────────┼────────────────────┼────────────────────┼────────────────────┘
               │                    │                    │
               └────────────────────┼────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Google Cloud Storage                                 │
│                                                                              │
│  gs://<bucket>/tenants/<tenant>/workspaces/<workspace>/                     │
│  ├── repo_snapshots/                                                        │
│  ├── toolchains/                                                            │
│  ├── seed/                                                                  │
│  ├── build_cache/                                                           │
│  ├── artifacts/                                                             │
│  └── logs/                                                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Deployment Modes

| Mode | Runtime | Cache Backend | Use Case |
|------|---------|---------------|----------|
| **Hosted (GCP)** | GKE + Kata (`RuntimeClass: kata`) | GCS | G5 managed service |
| **Customer-managed** | Kata-enabled K8s or equivalent VM isolation | GCS or S3-compatible | On-prem / private cloud |

### 1.3 Agent Pod Configuration

Each agent pod:
- Runs with `runtimeClassName: kata` (VM-level isolation)
- Uses a workspace-scoped identity (Workload Identity on GKE)
- Mounts local disk for hot caches (`/cache/*`)
- Accesses GCS only via its allowed prefix

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: agent-${TASK_ID}
  annotations:
    iam.gke.io/gcp-service-account: workspace-${WORKSPACE_ID}@${GCP_PROJECT}.iam.gserviceaccount.com
spec:
  runtimeClassName: kata
  
  containers:
    - name: agent
      image: g5/worker:${VERSION}
      resources:
        requests:
          memory: "8Gi"
          cpu: "4"
        limits:
          memory: "16Gi"
          cpu: "8"
      
      env:
        - name: G5_TASK_ID
          value: "${TASK_ID}"
        - name: G5_WORKSPACE_ID
          value: "${WORKSPACE_ID}"
        - name: G5_TENANT_ID
          value: "${TENANT_ID}"
        - name: G5_GCS_BUCKET
          value: "${GCS_BUCKET}"
        - name: G5_GCS_PREFIX
          value: "tenants/${TENANT_ID}/workspaces/${WORKSPACE_ID}"
        - name: G5_CONTROL_PLANE_URL
          value: "https://control.g5.internal"
        - name: G5_REDIS_URL
          valueFrom:
            secretKeyRef:
              name: g5-redis
              key: url
        - name: G5_PORT_SCAN_ENABLED
          value: "true"        # Enables /proc/net/tcp scanning for port forwarding (§1.4.2)
      
      volumeMounts:
        - name: cache
          mountPath: /cache
        - name: workspace
          mountPath: /workspace
  
  volumes:
    - name: cache
      emptyDir:
        sizeLimit: 50Gi
    - name: workspace
      emptyDir:
        sizeLimit: 20Gi
```

### 1.4 Networking & Port Forwarding

#### 1.4.1 Network Topology

Kata containers run a lightweight VM with its **own guest kernel and full network stack**. This means:

- **localhost works normally** inside the VM. Any process binding a port (e.g., `npm run dev` on `:3000`) is reachable by other processes in the same pod at `127.0.0.1:3000`.
- **Outbound egress is unrestricted by default.** Workers already connect to: control plane API (`G5_CONTROL_PLANE_URL`), Redis (`G5_REDIS_URL`), GCS (via Workload Identity). Any TCP/HTTP call to external services works.
- **Inbound from outside the pod** requires explicit routing — the current pod spec has no `Service` or Ingress.

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                           Kata VM (Guest Kernel)                              │
│                                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                       │
│  │   g5d worker  │  │  dev server  │  │  test runner  │  ← all share        │
│  │  (agent loop) │  │  :3000       │  │  :9229       │    localhost          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                       │
│         │ can reach       │ can reach       │ can reach                      │
│         ▼                 ▼                 ▼                                │
│  ┌─────────────────────────────────────────────────────┐                     │
│  │                 127.0.0.1 (lo)                       │                     │
│  └──────────────────────┬──────────────────────────────┘                     │
│                         │                                                     │
│  ┌──────────────────────┴──────────────────────────────┐                     │
│  │              eth0 (tap/veth to pod network)          │                     │
│  └──────────────────────┬──────────────────────────────┘                     │
└─────────────────────────┼─────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     K8s Pod Network (cluster CIDR)                            │
│                                                                              │
│  Pod IP: 10.x.y.z  ← reachable from other pods / services                  │
│  Egress: ✅ control plane, Redis, GCS, public internet                       │
│  Ingress: ❌ no Service defined (by design — workers are outbound-only)      │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 1.4.2 Port Forwarding (Reverse Tunnel)

When a user's process inside the worker binds a port (e.g., a dev server, debugger, or database), we expose it to the user's browser via a **reverse tunnel** through the existing worker ↔ control-plane connection. This keeps workers **outbound-only** — no K8s Service, no Ingress, no inbound firewall rules.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  User's Browser                                                              │
│                                                                              │
│  https://g5.dev/port-forward/worker-abc/3000                                │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────┐                                    │
│  │   Control Plane (g5d --role editor)  │                                    │
│  │                                      │                                    │
│  │   Port-Forward Proxy                 │                                    │
│  │   ┌──────────────────────────────┐  │                                    │
│  │   │ Maps: worker-abc:3000       │  │                                    │
│  │   │   → tunnel on WS channel    │  │                                    │
│  │   └──────────────┬───────────────┘  │                                    │
│  │                  │                   │                                    │
│  └──────────────────┼───────────────────┘                                    │
│                     │  multiplexed on existing                               │
│                     │  worker ↔ control-plane WS                             │
│                     ▼                                                        │
│  ┌──────────────────────────────────────┐                                   │
│  │   Worker Pod (Kata VM)               │                                   │
│  │                                       │                                   │
│  │   g5d tunnel-agent                    │                                   │
│  │     → connects to 127.0.0.1:3000     │                                   │
│  │     → relays bytes over WS channel   │                                   │
│  └──────────────────────────────────────┘                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Port detection.** The `g5d` worker process periodically scans `/proc/net/tcp` for newly-listening ports (same approach as VS Code Remote). When a new port is detected:

1. Worker sends a `port_open` event to the control plane via the existing Redis pub/sub channel:
   ```json
   {
     "type": "port_open",
     "port": 3000,
     "pid": 1234,
     "processName": "node",
     "label": "Vite dev server"
   }
   ```
2. Control plane registers a tunnel route for that worker + port.
3. Browser receives the event via WebSocket and shows a notification: *"Port 3000 (Vite dev server) is available — [Open in Browser]"*
4. The proxy URL is: `https://<host>/api/v1/port-forward/:workerId/:port`

**Protocol.** The tunnel multiplexes TCP streams over the existing WebSocket using a lightweight framing protocol (channel ID + length + payload). This is the same pattern used by VS Code Remote, Gitpod, and Codespaces.

**Access control.** Port-forward URLs inherit the worker's session auth — only users with access to the workspace can reach the forwarded port. The control plane validates the JWT before proxying.

**Local/desktop workers.** No tunneling needed — the dev server binds directly to the user's machine. `g5d` simply reports the port in the UI: *"Port 3000 available at http://localhost:3000"*.

#### 1.4.3 Docker-in-Docker (Nested Containers)

Kata's VM-level isolation makes nested containers **safer and simpler** than with runc:

| Aspect | runc (standard containers) | Kata (VM containers) |
|--------|---------------------------|---------------------|
| **DinD feasibility** | Requires `--privileged` or `/var/run/docker.sock` mount — both dangerous | Guest kernel supports cgroups/namespaces natively — no privilege escalation needed |
| **Security** | Nested containers share the host kernel — escape risk | Nested containers confined to guest VM — escape only reaches the VM boundary |
| **Container runtime** | Must share host dockerd or run dockerd in privileged mode | Independent dockerd/podman runs inside VM safely |

**How it works:**

1. The `g5/worker` base image includes `podman` (rootless, daemonless — preferred over Docker for simplicity):
   ```dockerfile
   # In g5/worker Dockerfile
   RUN apt-get install -y podman fuse-overlayfs slirp4netns
   ```
2. Inside the worker, user code or the agent can run containers:
   ```bash
   # Runs a PostgreSQL instance for integration tests
   podman run -d --name testdb -p 5432:5432 postgres:16
   
   # Accessible at localhost:5432 from within the worker
   psql -h localhost -U postgres
   ```
3. Port forwarding (§1.4.2) works for nested container ports too — if the container maps `5432` to the pod's localhost, the tunnel exposes it to the user's browser.

**Resource limits.** Nested containers share the worker pod's resource allocation (8-16 Gi memory, 4-8 CPU). The worker's agent loop should account for this when running containers:

```yaml
# Worker resource budget (from §1.3)
# Total: 16Gi memory, 8 CPU
# Reserved for g5d + agent loop: ~2Gi, 1 CPU
# Available for user processes + nested containers: ~14Gi, 7 CPU
```

**Storage.** Nested container images and layers use the worker's `/cache` emptyDir (50Gi). Podman's storage root is configured to `/cache/containers/`:
```bash
# /etc/containers/storage.conf inside the worker image
[storage]
  driver = "overlay"
  graphroot = "/cache/containers/storage"
  [storage.options.overlay]
    mount_program = "/usr/bin/fuse-overlayfs"
```

#### 1.4.4 Network Policies

```yaml
# NetworkPolicy: workers can egress, but receive no unsolicited inbound
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: worker-default
  namespace: g5-workers
spec:
  podSelector:
    matchLabels:
      app: g5-worker
  policyTypes:
    - Ingress
    - Egress
  
  # No ingress rules → all inbound blocked
  # (port-forwarding uses the outbound WS tunnel, not inbound)
  ingress: []
  
  egress:
    # Control plane API
    - to:
        - namespaceSelector:
            matchLabels:
              app: g5-control-plane
      ports:
        - port: 443
          protocol: TCP
    # Redis
    - to:
        - namespaceSelector:
            matchLabels:
              app: g5-redis
      ports:
        - port: 6379
          protocol: TCP
    # GCS (Google APIs)
    - to:
        - ipBlock:
            cidr: 0.0.0.0/0
      ports:
        - port: 443
          protocol: TCP
    # DNS
    - to: []
      ports:
        - port: 53
          protocol: UDP
        - port: 53
          protocol: TCP
```

> **Note:** The egress rule for GCS (`0.0.0.0/0:443`) is intentionally broad because Google's API IPs rotate. For tighter lockdown in air-gapped deployments, replace with the S3-compatible endpoint IP range.

---

## 2. Storage Contract (GCS)

### 2.1 Directory Structure

```
gs://<bucket>/tenants/<tenant>/workspaces/<workspace>/
├── repo_snapshots/
│   └── <repo>/<commit>.tar.zst           # Compressed repo at specific commit
├── toolchains/
│   └── <tool>/<version>/<platform>.tar.zst  # Toolchain bundles
├── seed/
│   └── <ecosystem>/<hash>.tar.zst        # Dependency seed caches
├── build_cache/
│   ├── sccache/...                       # Rust/C/C++ compiled objects
│   ├── bazel/...                         # Bazel action cache
│   └── gradle/...                        # Gradle build cache
├── artifacts/
│   └── <job_id>/...                      # Build outputs
└── logs/
    └── <job_id>/...                      # Execution logs
```

### 2.2 Storage Rules

| Rule | Rationale |
|------|-----------|
| GCS is for **immutable artifacts** and **content-addressed caches** | No POSIX semantics needed; avoids consistency issues |
| No shared writable filesystems | Security + simplicity |
| Content-addressed where possible | Enables safe concurrent writes |
| Compressed with zstd | Good compression ratio + fast decompression |

### 2.3 Access Control

```yaml
# IAM condition restricts access to workspace prefix
resource: "projects/_/buckets/${BUCKET}/objects/*"
condition:
  expression: |
    resource.name.startsWith("projects/_/buckets/${BUCKET}/objects/tenants/${TENANT_ID}/workspaces/${WORKSPACE_ID}/")
```

---

## 3. Base Image Strategy

### 3.1 Image Composition

```
┌─────────────────────────────────────────────────────────────────┐
│                     Workspace Base Image                         │
│                                                                  │
│  Built from .devcontainer/ (if present) or g5/worker:base       │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  ✅ BAKED IN                                             │    │
│  │  • OS packages (build-essential, curl, git, etc.)       │    │
│  │  • Compilers (gcc, clang, rustc)                        │    │
│  │  • Language runtimes (Node, Python, Go, JDK)            │    │
│  │  • Common SDKs and CLIs                                 │    │
│  │  • sccache, cache tools                                 │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  ❌ NOT BAKED IN (hydrated at runtime)                   │    │
│  │  • Repository working tree                              │    │
│  │  • Build outputs                                        │    │
│  │  • Credentials / secrets                                │    │
│  │  • Rapidly changing dependency trees                    │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Image Build Pipeline

```bash
# Build workspace-specific image from devcontainer
docker build \
  -f .devcontainer/Dockerfile \
  -t gcr.io/${PROJECT}/g5-worker:${WORKSPACE_ID} \
  --cache-from gcr.io/${PROJECT}/g5-worker:${WORKSPACE_ID} \
  .

# Or use base image with feature layering
docker build \
  --build-arg BASE=g5/worker:base \
  --build-arg FEATURES="rust,node,python" \
  -t gcr.io/${PROJECT}/g5-worker:${WORKSPACE_ID} \
  .
```

---

## 4. Toolchain-Specific Cache Configuration

### 4.1 Rust

**What is cached:**
- Cargo registry index
- Downloaded crate archives
- Git dependencies
- Compiler outputs via sccache

**Configuration:**
```bash
# Environment variables
export CARGO_HOME=/cache/cargo
export RUSTC_WRAPPER=sccache
export SCCACHE_GCS_BUCKET=<bucket>
export SCCACHE_GCS_PREFIX=tenants/<tenant>/workspaces/<workspace>/build_cache/sccache
export SCCACHE_GCS_RW_MODE=READ_WRITE
```

**Cache flow:**
```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ rustc       │────▶│ sccache     │────▶│ Local disk  │
│             │     │ (wrapper)   │     │ /cache/     │
└─────────────┘     └──────┬──────┘     └─────────────┘
                           │ miss
                           ▼
                    ┌─────────────┐
                    │ GCS         │
                    │ build_cache/│
                    │ sccache/    │
                    └─────────────┘
```

**Still slow when:**
- Native dependencies rebuild (openssl, rocksdb, ring)
- LTO or large link steps dominate
- Toolchain version changes invalidate cache

---

### 4.2 C / C++

**What is cached:**
- Compiled object files via sccache

**Configuration:**
```bash
export CC="sccache gcc"    # or clang
export CXX="sccache g++"
export SCCACHE_GCS_BUCKET=<bucket>
export SCCACHE_GCS_PREFIX=tenants/<tenant>/workspaces/<workspace>/build_cache/sccache
```

**CMake integration:**
```cmake
set(CMAKE_C_COMPILER_LAUNCHER sccache)
set(CMAKE_CXX_COMPILER_LAUNCHER sccache)
```

**Still slow when:**
- `./configure` steps are expensive
- Compiler flags are non-deterministic
- Very large static link steps

---

### 4.3 Go

**What is cached:**
- Go build cache (compiled packages)
- Go module download cache

**Configuration:**
```bash
export GOCACHE=/cache/go-build
export GOMODCACHE=/cache/go-mod
```

**Notes:**
- Go tooling handles cache keys internally
- Cache is fully deterministic per Go version + GOOS/GOARCH
- No remote cache needed (local is sufficient for most cases)

**Still slow when:**
- CGO is enabled (falls back to C/C++ toolchain)
- Very large dependency graphs on first build

---

### 4.4 Python

**What is cached:**
- pip download cache
- Built wheels
- Optional: packaged virtualenv / conda env bundles

**Configuration (pip):**
```bash
export PIP_CACHE_DIR=/cache/pip
export PIP_DISABLE_PIP_VERSION_CHECK=1
```

**Configuration (conda / micromamba):**
```bash
# Package cache
export CONDA_PKGS_DIRS=/cache/conda/pkgs

# Optional: pre-built env bundles from GCS
gsutil cp gs://${BUCKET}/${PREFIX}/seed/python/${ENV_HASH}.tar.zst /tmp/
tar -xf /tmp/${ENV_HASH}.tar.zst -C /workspace/.venv
```

**Still slow when:**
- Native extensions compile from source (numpy, pandas without wheels)
- Python version / ABI changes
- No lockfile (resolver runs every time)

---

### 4.5 Java

**What is cached:**
- Maven local repository
- Gradle dependency cache
- Gradle build cache (compiled outputs)

**Configuration (Maven):**
```xml
<!-- settings.xml -->
<settings>
  <localRepository>/cache/m2</localRepository>
</settings>
```

**Configuration (Gradle):**
```bash
export GRADLE_USER_HOME=/cache/gradle
```

```properties
# gradle.properties
org.gradle.caching=true
org.gradle.parallel=true
org.gradle.daemon=false
```

**Still slow when:**
- Dependency resolution hits many remote repos
- Annotation processing breaks cacheability
- First build downloads entire dependency graph

---

### 4.6 Kotlin

**What is cached:**
- Same as Java (Gradle)
- Kotlin compiler task outputs via Gradle build cache

**Configuration:**
Same as Java/Gradle above.

**Still slow when:**
- kapt / annotation processors are involved
- Incremental compilation is disabled
- K2 compiler not yet stable

---

### 4.7 JavaScript / TypeScript

**What is cached:**
- pnpm store (content-addressed)
- npm/yarn cache (if used)
- TypeScript incremental build info

**Configuration (pnpm - recommended):**
```bash
pnpm config set store-dir /cache/pnpm-store
```

**Configuration (npm):**
```bash
export npm_config_cache=/cache/npm
```

**Configuration (yarn):**
```bash
yarn config set cache-folder /cache/yarn
```

**Still slow when:**
- `postinstall` scripts run native builds
- Large monorepos relink many packages
- No lockfile present

---

## 5. Cache Behavior Summary

### 5.1 Cache Layers

| Layer | Location | Scope | Purpose |
|-------|----------|-------|---------|
| **L1: Local disk** | `/cache/*` on node | Single pod | Fast LRU hot cache |
| **L2: Remote cache** | GCS `build_cache/` | Workspace-wide | Cross-worker reuse |
| **Artifacts** | GCS `artifacts/` | Job-scoped | Build outputs, test results |
| **Repo snapshots** | GCS `repo_snapshots/` | Workspace-wide | Fast deterministic checkout |

### 5.2 Cache Hit Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        Build Request                             │
└─────────────────────────────┬───────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ L1: Local Disk  │
                    │ /cache/*        │
                    └────────┬────────┘
                             │
              ┌──────────────┴──────────────┐
              │ HIT                         │ MISS
              ▼                             ▼
    ┌─────────────────┐           ┌─────────────────┐
    │ Use cached      │           │ L2: GCS Remote  │
    │ artifact        │           │ build_cache/    │
    └─────────────────┘           └────────┬────────┘
                                           │
                            ┌──────────────┴──────────────┐
                            │ HIT                         │ MISS
                            ▼                             ▼
                  ┌─────────────────┐           ┌─────────────────┐
                  │ Download to L1  │           │ Build from      │
                  │ Use artifact    │           │ source          │
                  └─────────────────┘           │ Upload to L1+L2 │
                                               └─────────────────┘
```

### 5.3 Local Cache Sizing

```yaml
# Per-tool cache allocation
/cache/
├── cargo/        # 5GB   - Rust crates + registry
├── go-build/     # 2GB   - Go compiled packages
├── go-mod/       # 3GB   - Go modules
├── pip/          # 2GB   - Python wheels
├── m2/           # 5GB   - Maven artifacts
├── gradle/       # 5GB   - Gradle deps + build cache
├── pnpm-store/   # 5GB   - Node packages
├── sccache/      # 10GB  - Compiled objects (shared)
└── misc/         # 3GB   - Other toolchains
# Total: ~45GB (fits in 50GB emptyDir)
```

---

## 6. Security Model

### 6.1 Principles

| Principle | Implementation |
|-----------|----------------|
| **Least privilege** | One GCP service account per workspace |
| **Namespace isolation** | IAM conditions restrict to `tenants/<tenant>/workspaces/<workspace>/**` |
| **No shared state** | No shared writable filesystems between workspaces |
| **No baked credentials** | Secrets injected at runtime via Secret Manager |
| **Audit trail** | GCS access logs + agent timeline |

### 6.2 Identity Model

```
┌─────────────────────────────────────────────────────────────────┐
│                     GKE Workload Identity                        │
│                                                                  │
│  Pod: agent-task-123                                            │
│    │                                                             │
│    └─▶ K8s ServiceAccount: workspace-456-agent                    │
│          │                                                       │
│          └─▶ GCP ServiceAccount: workspace-456@g5.iam.gsa.com     │
│                │                                                 │
│                └─▶ IAM Binding:                                  │
│                      roles/storage.objectAdmin                   │
│                      condition: resource.name.startsWith(        │
│                        "tenants/t1/workspaces/456/"               │
│                      )                                           │
└─────────────────────────────────────────────────────────────────┘
```

### 6.3 Secret Injection

```yaml
# Secrets mounted from Secret Manager
env:
  - name: ANTHROPIC_API_KEY
    valueFrom:
      secretKeyRef:
        name: workspace-${WORKSPACE_ID}-secrets
        key: anthropic-api-key
  # Git credential is NOT a static secret — it is resolved at task dispatch
  # time from the workspace owner's user_scm_connections (see API §3.11.2).
  - name: G5_GIT_CREDENTIAL
    value: "<injected-at-dispatch>"  # See §6.3.1 below
```

#### 6.3.1 Git Credential Resolution

When the editor dispatches a task to a worker, it resolves git credentials
for each repo in the workspace and injects them into the worker environment.
This ensures the worker can clone private repos the workspace owner has
granted access to.

```
┌─────────────────────────────────────────────────────────────────┐
│              Git Credential Resolution (at dispatch)             │
│                                                                  │
│  1. Look up workspace → workspace_branches → repos                 │
│  2. For each repo with provider != NULL:                        │
│     a. Find user_scm_connections for workspace.created_by         │
│        WHERE provider = repo.provider                           │
│     b. If GitHub App install:                                   │
│        → Request installation token (cached in Redis, 1hr TTL)  │
│     c. If OAuth token:                                          │
│        → Decrypt access_token_enc                               │
│        → If token_expires_at < now() + 5min → refresh first    │
│     d. Construct authenticated clone URL:                       │
│        GitHub: https://x-access-token:{tok}@github.com/o/r.git │
│        GitLab: https://oauth2:{tok}@gitlab.com/o/r.git          │
│  3. Inject as env vars:                                         │
│     G5_REPO_CREDENTIALS='{"repos":[                             │
│       {"url":"github.com/acme/backend","token":"ghs_xxx"},     │
│       {"url":"gitlab.com/acme/types","token":"glpat_yyy"}      │
│     ]}'                                                         │
│  4. Worker init-workspace.sh configures git credential helper:  │
│     git config credential.helper '!f(){ echo "password=$tok"   │
│       ; echo "username=x-access-token"; }; f'                  │
│                                                                  │
│  Fallback: if no SCM connection exists for a repo's provider,   │
│  the clone will fail with a clear error:                        │
│  "No SCM connection for github — connect at Settings > SCM"     │
└─────────────────────────────────────────────────────────────────┘
```

The editor **never** sends raw tokens over the worker WebSocket. Tokens are
injected into the pod environment at creation time (Kubernetes Secret or
in-memory env var), so they are not visible in task logs.

---

## 7. Known Limitations

### 7.1 Expected Slowness

| Scenario | Cause | Mitigation |
|----------|-------|------------|
| First build | Cold caches | Seed caches from previous builds |
| Toolchain upgrade | Cache keys change | Pre-warm new toolchain caches |
| Native deps | openssl, rocksdb, etc. | Bake into base image if stable |
| Large links | LTO, static linking | Not cacheable; optimize build config |
| Annotation processors | Non-deterministic | Limit kapt usage |

### 7.2 Inherent Limitations

| Limitation | Explanation |
|------------|-------------|
| Cache invalidation on toolchain upgrades | Expected; keys include toolchain version |
| Non-deterministic builds | Some build systems (autotools) are inherently flaky |
| Link time not cacheable | Final link step always runs |
| Network latency to GCS | ~10-50ms per object; batching helps |

### 7.3 Not Supported (Yet)

| Feature | Status | Timeline |
|---------|--------|----------|
| Port forwarding (reverse tunnel) | Planned (§1.4.2) | Q1 2026 |
| Docker-in-Docker (podman) | Planned (§1.4.3) | Q1 2026 |
| Persistent workspace volumes | Planned | Q2 2026 |
| Volume cloning/snapshots | Planned | Q2 2026 |
| Bazel remote execution | Evaluating | Q3 2026 |
| Air-gapped deployments | Supported via S3-compat | Available now |

---

## 8. Workspace Initialization

### 8.1 Fast Checkout via Snapshots

Instead of `git clone`, we use pre-computed repo snapshots. The
`G5_REPO_CREDENTIALS` env var (injected at dispatch — see §6.3.1) provides
authenticated access to each repo for clone, fetch, pull **and push**.

```bash
#!/bin/bash
set -euo pipefail
# init-workspace.sh

# ── 1. Configure git credential helper from injected SCM tokens ──────────
# G5_REPO_CREDENTIALS is JSON: {"repos":[{"url":"github.com/o/r","token":"..."}]}
# We write a per-host credential helper so git clone/fetch/push all Just Work.

mkdir -p ~/.config/git

# Parse credentials and write a git credential helper per repo host+path
echo "${G5_REPO_CREDENTIALS}" | jq -r '.repos[] | "\(.url) \(.token)"' | \
while read -r repo_url token; do
  HOST=$(echo "$repo_url" | cut -d/ -f1)
  # Store credentials in git credential store (plaintext in ephemeral container)
  echo "https://x-access-token:${token}@${repo_url}" >> ~/.config/git/credentials
done

git config --global credential.helper 'store --file ~/.config/git/credentials'

# ── 2. Clone / checkout each repo in the workspace ─────────────────────────
# G5_WORKSPACE_REPOS is JSON array injected at dispatch:
# [{"name":"backend","url":"github.com/acme/backend",
#   "commit":"abc1234","branch":"feat/streaming"}]

echo "${G5_WORKSPACE_REPOS}" | jq -c '.[]' | while read -r repo_json; do
  REPO_NAME=$(echo "$repo_json" | jq -r '.name')
  REPO_URL=$(echo "$repo_json"  | jq -r '.url')
  COMMIT=$(echo "$repo_json"    | jq -r '.commit')
  BRANCH=$(echo "$repo_json"    | jq -r '.branch')
  DEST="/workspace/${REPO_NAME}"

  SNAPSHOT_URL="gs://${G5_GCS_BUCKET}/${G5_GCS_PREFIX}/repo_snapshots/${REPO_NAME}/${COMMIT}.tar.zst"

  if gsutil -q stat "${SNAPSHOT_URL}"; then
    echo "[${REPO_NAME}] Using cached snapshot..."
    gsutil cp "${SNAPSHOT_URL}" /tmp/snapshot.tar.zst
    mkdir -p "${DEST}"
    tar -I zstd -xf /tmp/snapshot.tar.zst -C "${DEST}"
    cd "${DEST}"
    # Snapshot has no remote — add it so push/pull work
    git remote add origin "https://${REPO_URL}.git" 2>/dev/null || \
      git remote set-url origin "https://${REPO_URL}.git"
    git fetch origin "${BRANCH}" --depth=1
    git checkout -b "task/${G5_TASK_ID}" "${COMMIT}"
  else
    echo "[${REPO_NAME}] Snapshot not found, cloning..."
    git clone --depth=1 --branch="${BRANCH}" "https://${REPO_URL}.git" "${DEST}"
    cd "${DEST}"
    git fetch --depth=1 origin "${COMMIT}"
    git checkout -b "task/${G5_TASK_ID}" "${COMMIT}"
  fi

  # Configure push to push to the workspace branch (not task branch)
  git config push.default current
  git remote set-url --push origin "https://${REPO_URL}.git"
done

# ── 3. Hydrate dependency caches ─────────────────────────────────────────
/opt/g5/hydrate-caches.sh
```

> **Key point:** The same credential helper is used by the agent's `git_push`,
> `git_pull`, and `git_fetch` tools (§10.7) throughout the task's lifetime.
> Tokens live only in the ephemeral container's memory — they are never
> written to task logs or exposed over WebSocket.

### 8.2 Cache Hydration

```bash
#!/bin/bash
# hydrate-caches.sh

# Detect workspace/codebase type and hydrate relevant caches
if [ -f "Cargo.toml" ]; then
  echo "Hydrating Rust caches..."
  SEED_URL="gs://${G5_GCS_BUCKET}/${G5_GCS_PREFIX}/seed/rust/${CARGO_LOCK_HASH}.tar.zst"
  if gsutil -q stat "${SEED_URL}"; then
    gsutil cp "${SEED_URL}" - | tar -I zstd -xf - -C /cache/cargo
  fi
fi

if [ -f "package.json" ]; then
  echo "Hydrating Node caches..."
  SEED_URL="gs://${G5_GCS_BUCKET}/${G5_GCS_PREFIX}/seed/node/${LOCKFILE_HASH}.tar.zst"
  if gsutil -q stat "${SEED_URL}"; then
    gsutil cp "${SEED_URL}" - | tar -I zstd -xf - -C /cache/pnpm-store
  fi
fi

# Similar for Python, Java, Go...
```

---

## 9. Monitoring & Observability

### 9.1 Metrics

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `cache_hit_rate` | L1/L2 cache hit percentage | < 50% |
| `build_time_seconds` | Total build duration | > 10min |
| `gcs_download_bytes` | Data pulled from GCS | > 1GB/build |
| `workspace_init_seconds` | Time to checkout + hydrate | > 60s |
| `sccache_hit_rate` | Compiled object cache hits | < 70% |

### 9.2 Logging

```json
{
  "timestamp": "2026-02-05T14:30:00Z",
  "task_id": "task-abc123",
  "event": "cache_miss",
  "cache_layer": "L2",
  "cache_key": "sccache/rust/1.75.0/abc123def456",
  "latency_ms": 45
}
```

---

## 10. Agent Session & Task Allocation

### 10.1 Core Model

The **agent session** is the central abstraction. It is a persisted dialog (message history + state) stored in the database. The **agent loop** is the process that drives inference — it runs on the editor instance (§11), with tool calls forwarded to an attached worker when one is available.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Agent Session (DB)                                   │
│                                                                              │
│  session_id: "sess-abc123"                                                  │
│  task_id: "task-456"                                                        │
│  workspace_id: "proj-789"                                                     │
│  status: running | paused | waiting_for_input | completed | failed          │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────┐     │
│  │  Messages (ordered, append-only)                                   │     │
│  │                                                                    │     │
│  │  [0] system    "You are a coding agent..."         (init)         │     │
│  │  [1] user      "Implement error handling"          (web client)   │     │
│  │  [2] assistant "I'll start by examining..."        (agent loop)   │     │
│  │  [3] tool_call read_file("src/lib.rs")             (agent loop)   │     │
│  │  [4] tool_result "fn parse_config()..."            (worker)       │     │
│  │  [5] assistant "I see the function..."             (agent loop)   │     │
│  │  [6] user      "Also handle timeouts"              (slack)        │     │
│  │  [7] assistant "Good point, I'll add..."           (agent loop)   │     │
│  └────────────────────────────────────────────────────────────────────┘     │
│                                                                              │
│  worker_id: "worker-xyz" | null                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 10.2 Session Lifecycle

```
                  ┌──────────┐
                  │  CREATE   │  User initiates task
                  └─────┬────┘
                        │
                        ▼
              ┌─────────────────┐
              │  PROVISIONING   │  Provision worker (Kata container)
              └─────────┬───────┘
                        │
                        ▼
              ┌─────────────────┐
              │  WORKER MODE    │  Agent loop runs on worker
              │                 │  alongside the workspace
              └────────┬────────┘
                       │
                       ▼
                  ┌─────────────────┐
                  │    RUNNING       │◄──── resume
                  └────────┬────────┘
                           │
          ┌────────────────┼────────────────┐
          │                │                │
          ▼                ▼                ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│   PAUSED     │ │  WAITING     │ │  COMPLETED   │
│              │ │  FOR INPUT   │ │  / FAILED    │
└──────────────┘ └──────────────┘ └──────────────┘
```

> **Note:** Every task requires a dedicated worker — there is no degraded "backend mode." See AGENTS.md §1.1.

### 10.3 Multi-Client Architecture

Any authorized client can inject messages or control commands into a session. The session is the shared state; the agent loop is just a consumer.

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  Web Client  │  │ Deployed     │  │  Slack Bot   │  │ WhatsApp     │
│  (Browser)   │  │ Agent        │  │  (future)    │  │ (future)     │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │                 │
       │     All clients speak the same Session API         │
       │                 │                 │                 │
       └─────────────────┴────────┬────────┴─────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Control Plane                                      │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                       Session Router                                  │  │
│  │                                                                       │  │
│  │  POST /api/v1/sessions/:id/messages   → append to message log           │  │
│  │  POST /api/v1/sessions/:id/control    → pause / resume / abort          │  │
│  │  GET  /api/v1/sessions/:id/messages   → read message history            │  │
│  │  WS   /api/internal/sessions/:id/stream → subscribe to live updates     │  │
│  │                                                                       │  │
│  │  ┌──────────────────────────────────────────────────────────────┐    │  │
│  │  │              Redis: session:{id}:inbox                        │    │  │
│  │  │              (user messages + control signals)                │    │  │
│  │  └──────────────────────────────────────────────────────────────┘    │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                  │                                          │
└──────────────────────────────────┼──────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Agent Loop (Worker)                                │
│                                                                              │
│  while session.status == RUNNING:                                           │
│      check_inbox()         ← process injected messages / control            │
│      messages = get_messages()                                              │
│      response = llm.chat(messages)                                          │
│      if response.tool_calls:                                                │
│          results = execute_tools(response.tool_calls)  ← runs on worker    │
│          append(results)                                                    │
│      else:                                                                  │
│          append(response)                                                   │
│      persist_to_db()                                                        │
│      broadcast_to_subscribers()                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 10.4 Session Data Model

```typescript
interface AgentSession {
  id: string;
  taskId: string;
  workspaceId: string;
  tenantId: string;
  
  // State
  status: 'provisioning' | 'running' | 'paused' | 'waiting_for_input' 
        | 'completed' | 'failed' | 'aborted';
  
  // Where the agent loop is executing
  loopLocation: 'worker' | null;
  workerId: string | null;
  
  // Worker image
  workerImage: string | null;        // e.g., "gcr.io/g5/workspace-123:latest"
  workerImageTag: string | null;     // e.g., "latest", "2026-02-04-abc1234"
  
  // LLM configuration
  model: string;                     // e.g., "anthropic/claude-3.5-sonnet"
  systemPrompt: string;
  maxTurns: number;                  // Safety limit
  
  // Counters
  turnCount: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCostUsd: number;
  
  // Timestamps
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  lastActivityAt: string;
  
  // Git state (if worker mode)
  branch: string | null;             // task/{task-id}
  baseCommit: string | null;
  headCommit: string | null;
  checkpoints: Checkpoint[];
}

interface SessionMessage {
  id: string;
  sessionId: string;
  index: number;                     // Monotonic sequence number
  
  role: 'system' | 'user' | 'assistant' | 'tool_call' | 'tool_result';
  content: string;
  
  // Origin tracking
  source: MessageSource;
  
  // For tool calls/results
  toolName: string | null;
  toolCallId: string | null;
  
  // Metadata
  tokenCount: number | null;
  latencyMs: number | null;
  timestamp: string;
}

type MessageSource = 
  | { type: 'system' }
  | { type: 'agent_loop' }
  | { type: 'web_client'; userId: string; connectionId: string }
  | { type: 'api'; apiKeyId: string }
  | { type: 'slack'; channelId: string; userId: string }
  | { type: 'whatsapp'; phoneNumber: string }
  | { type: 'deployed_agent'; agentId: string };

interface Checkpoint {
  commitSha: string;
  messageIndex: number;              // Which message triggered this
  reason: string;
  filesChanged: string[];
  timestamp: string;
}
```

### 10.5 The Agent Loop

The agent loop is the core while-loop that drives inference. It runs either on a provisioned worker or on the backend server (connected to clients via WebSocket).

```typescript
// agent-loop.ts  —  runs on worker OR on backend server

interface AgentLoopConfig {
  sessionId: string;
  model: string;
  systemPrompt: string;
  tools: ToolDefinition[];
  maxTurns: number;
  
  // Injected dependencies (differ between worker and browser)
  toolExecutor: ToolExecutor;
  sessionStore: SessionStore;
  inbox: InboxReader;
  broadcast: Broadcaster;
}

async function agentLoop(config: AgentLoopConfig): Promise<void> {
  const { sessionId, model, tools, toolExecutor, sessionStore, inbox, broadcast } = config;
  
  let turn = 0;
  
  while (turn < config.maxTurns) {
    // ──────────────────────────────────────────────────
    // 1. Check inbox for injected messages or control signals
    // ──────────────────────────────────────────────────
    const inboxItems = await inbox.drain(sessionId);
    
    for (const item of inboxItems) {
      if (item.type === 'control') {
        switch (item.action) {
          case 'pause':
            await sessionStore.setStatus(sessionId, 'paused');
            await broadcast.emit(sessionId, { type: 'status', status: 'paused' });
            await inbox.waitForResume(sessionId);   // blocks
            await sessionStore.setStatus(sessionId, 'running');
            await broadcast.emit(sessionId, { type: 'status', status: 'running' });
            break;
            
          case 'abort':
            await sessionStore.setStatus(sessionId, 'aborted');
            await broadcast.emit(sessionId, { type: 'status', status: 'aborted' });
            return;
            
          case 'checkpoint':
            await toolExecutor.execute('git_commit', { 
              message: `[checkpoint] User requested` 
            });
            break;
        }
      }
      
      if (item.type === 'message') {
        // Injected user message — append to session
        await sessionStore.appendMessage(sessionId, {
          role: 'user',
          content: item.content,
          source: item.source,
        });
        await broadcast.emit(sessionId, { 
          type: 'message', 
          role: 'user', 
          content: item.content 
        });
      }
    }
    
    // ──────────────────────────────────────────────────
    // 2. Get full message history and call LLM
    // ──────────────────────────────────────────────────
    const messages = await sessionStore.getMessages(sessionId);
    
    const response = await streamLLMResponse({
      model,
      messages,
      tools,
      onChunk: (chunk) => broadcast.emit(sessionId, { 
        type: 'chunk', 
        content: chunk 
      }),
    });
    
    // ──────────────────────────────────────────────────
    // 3. Process response
    // ──────────────────────────────────────────────────
    if (response.toolCalls && response.toolCalls.length > 0) {
      // Persist assistant message with tool calls
      await sessionStore.appendMessage(sessionId, {
        role: 'assistant',
        content: response.text || '',
        source: { type: 'agent_loop' },
      });
      
      // Execute each tool call
      for (const toolCall of response.toolCalls) {
        await sessionStore.appendMessage(sessionId, {
          role: 'tool_call',
          content: JSON.stringify(toolCall.args),
          toolName: toolCall.name,
          toolCallId: toolCall.id,
          source: { type: 'agent_loop' },
        });
        
        await broadcast.emit(sessionId, { 
          type: 'tool_call', 
          tool: toolCall.name, 
          args: toolCall.args 
        });
        
        const result = await toolExecutor.execute(toolCall.name, toolCall.args);
        
        await sessionStore.appendMessage(sessionId, {
          role: 'tool_result',
          content: typeof result === 'string' ? result : JSON.stringify(result),
          toolCallId: toolCall.id,
          source: { type: 'agent_loop' },
        });
        
        await broadcast.emit(sessionId, { 
          type: 'tool_result', 
          tool: toolCall.name, 
          result 
        });
      }
      
    } else {
      // Plain assistant response — may be final
      await sessionStore.appendMessage(sessionId, {
        role: 'assistant',
        content: response.text,
        source: { type: 'agent_loop' },
      });
      
      await broadcast.emit(sessionId, { 
        type: 'message', 
        role: 'assistant', 
        content: response.text 
      });
      
      // If assistant didn't call tools, check if it's done
      if (response.stopReason === 'end_turn') {
        break;  // Agent decided it's finished
      }
    }
    
    turn++;
  }
  
  await sessionStore.setStatus(sessionId, 'completed');
  await broadcast.emit(sessionId, { type: 'status', status: 'completed' });
}
```

### 10.6 Agent Loop Execution Environment

The agent loop runs on the assigned **worker** (Kata container). Tools execute locally against the container's filesystem and toolchain.

```typescript
const workerConfig: AgentLoopConfig = {
  sessionId,
  model,
  systemPrompt,
  maxTurns: 200,
  
  tools: fullToolkit,   // File ops, git, build, test
  
  toolExecutor: new WorkerToolExecutor({
    workspacePath: '/workspace',
    // Tools execute directly on the container's filesystem
  }),
  
  sessionStore: new APISessionStore({
    controlPlaneUrl: process.env.G5_CONTROL_PLANE_URL,
    // Persists to Postgres via control plane API
  }),
  
  inbox: new RedisInboxReader({
    redisUrl: process.env.G5_REDIS_URL,
    channel: `session:${sessionId}:inbox`,
    // Receives messages from any client
  }),
  
  broadcast: new RedisBroadcaster({
    redisUrl: process.env.G5_REDIS_URL,
    channel: `session:${sessionId}:events`,
    // Publishes to all subscribed clients
  }),
};
```

> **Historical note:** An earlier design included a "backend mode" where the agent loop ran on a backend server without a dedicated container. This has been removed — every task requires a worker. See AGENTS.md §1.1.

### 10.7 Tool Availability

> **Scope note:** This table shows *where* each tool executes on the worker. Canonical tool definitions (schemas, contracts, semantic groupings) live in **AGENTS.md §6**.

| Tool | Execution | Notes |
|------|-----------|-------|
| `read_file` | Local fs | Reads from CRDT-materialized workspace |
| `write_file` | Local fs + CRDT | Writes locally, enqueues CRDT delta |
| `edit_file_replace` | Local fs + CRDT | Same as write_file |
| `list_directory` | Local fs | |
| `semantic_search` | Via API | Uses search service |
| `git_status` | Local git | Reflects base commit + CRDT draft overlay |
| `git_diff` | Local git | Reflects base commit + CRDT draft overlay |
| `git_commit` | Local git | Materializes draft → commit → resets overlay |
| `git_push` | Local git | Pushes to remote via SCM creds (§6.3.1) |
| `git_pull` | Local git | Pulls from remote; rebases onto task branch |
| `git_fetch` | Local git | Background fetch every 30s + on-demand |
| `run_in_terminal` | Persistent PTY | Long-lived shell session in Kata container; reused across commands |

### 10.8 Message Injection Protocol

Any client appends messages via the same API. The agent loop's inbox processes them on its next iteration.

```typescript
// Session API — used by all clients

// Append a user message
POST /api/v1/sessions/:sessionId/messages
{
  "role": "user",
  "content": "Also handle timeout errors",
  "source": { "type": "web_client", "userId": "user-1", "connectionId": "ws-abc" }
}

// Send control signal
POST /api/v1/sessions/:sessionId/control
{
  "action": "pause"  // or "resume", "abort", "checkpoint"
}

// Subscribe to live updates (internal: worker → editor)
WS /api/internal/sessions/:sessionId/stream
→ { "type": "chunk", "content": "I'll start by..." }
→ { "type": "tool_call", "tool": "read_file", "args": {...} }
→ { "type": "tool_result", "tool": "read_file", "result": "..." }
→ { "type": "status", "status": "paused" }
→ { "type": "message", "role": "user", "content": "...", "source": {...} }
```

**Inbox Semantics:**
```
┌──────────────────────────────────────────────────────────────────┐
│                   Redis Stream: session:{id}:inbox               │
│                                                                   │
│  Entry 1: { type: "message", content: "...", source: {slack} }   │
│  Entry 2: { type: "control", action: "pause" }                  │
│  Entry 3: { type: "message", content: "...", source: {web} }    │
│  Entry 4: { type: "control", action: "resume" }                 │
│                                                                   │
│  Consumer: Agent loop drains on each iteration                   │
│  Ordering: FIFO, processed in sequence                           │
│  Persistence: Trimmed after processing, messages saved to DB     │
└──────────────────────────────────────────────────────────────────┘
```

### 10.9 Execution Intervention

A running task agent can be influenced in two fundamentally different ways. Both use the same underlying inbox mechanism (§10.8), but they serve different purposes and present different UIs.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Two Intervention Modes                                     │
│                                                                              │
│  Mode 1: MESSAGE INJECTION (indirect)                                       │
│  ─────────────────────────────────────                                       │
│  The user's main chat sends a message that gets injected into the           │
│  task agent's context. The user does NOT join the task's conversation —     │
│  they fire a message from the outside.                                      │
│                                                                              │
│  Analogy: texting your coworker "hey also handle timeout errors"            │
│  while they're working on a task. They'll see it next time they check.     │
│                                                                              │
│  ┌──────────────────┐                ┌─────────────────────────────┐       │
│  │  Main Chat        │                │  Task Agent (running)       │       │
│  │  (user's session) │   inject msg   │  Session S-456              │       │
│  │                    │ ──────────────▶│                             │       │
│  │  User: "tell the  │                │  inbox receives:            │       │
│  │   auth task to    │                │    { role: "user",          │       │
│  │   also handle     │                │      content: "Also handle  │       │
│  │   timeouts"       │                │        timeout errors",     │       │
│  │                    │                │      source: { type:        │       │
│  │  Main agent:      │                │        "injection",         │       │
│  │  "Done, I've told │                │        fromSession: S-123 } │       │
│  │   the task agent."│                │    }                        │       │
│  └──────────────────┘                │                             │       │
│                                       │  Agent picks it up on next  │       │
│                                       │  loop iteration and adjusts │       │
│                                       │  its plan.                  │       │
│                                       └─────────────────────────────┘       │
│                                                                              │
│  Implementation: The main chat agent has a tool `send_message` that        │
│  takes a session ID and message content. It's a fire-and-forget — the      │
│  main chat doesn't subscribe to the task's response stream.                 │
│                                                                              │
│  ─────────────────────────────────────────────────────────────────────────  │
│                                                                              │
│  Mode 2: DIRECT ATTACHMENT (join the task)                                  │
│  ──────────────────────────────────────────                                  │
│  The user's chat attaches directly to the task session, becoming a          │
│  live participant alongside the agent. The user sees the agent's full       │
│  conversation history, tool calls, and output in real time — and can        │
│  type messages that appear inline in the task's message stream.             │
│                                                                              │
│  Analogy: sitting down next to your coworker and pair-programming.          │
│  You're both looking at the same screen, talking in the same thread.        │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────┐       │
│  │  Browser: Task View (attached to S-456)                          │       │
│  │                                                                   │       │
│  │  ┌─ Task: Fix auth error handling ────────────────────────────┐  │       │
│  │  │                                                             │  │       │
│  │  │  🤖 Agent: I'll start by reading the auth module...        │  │       │
│  │  │  🔧 read_file("src/auth.rs") → [268 lines]                │  │       │
│  │  │  🤖 Agent: I see the issue. The error path at L45...       │  │       │
│  │  │  🔧 edit_file_replace("src/auth.rs", ...) → ✅                    │  │       │
│  │  │                                                             │  │       │
│  │  │  👤 You: Also handle the timeout case on line 120          │  │       │
│  │  │                                                             │  │       │
│  │  │  🤖 Agent: Good catch. I'll add timeout handling there...  │  │       │
│  │  │  🔧 edit_file_replace("src/auth.rs", ...) → ✅                    │  │       │
│  │  │                                                             │  │       │
│  │  └─────────────────────────────────────────────────────────────┘  │       │
│  │                                                                   │       │
│  │  ┌──────────────────────────────────────────────────────────────┐│       │
│  │  │ Type a message...                                [Detach] ▶│ │       │
│  │  └──────────────────────────────────────────────────────────────┘│       │
│  └──────────────────────────────────────────────────────────────────┘       │
│                                                                              │
│  Implementation: The user's browser subscribes to the task session's        │
│  WebSocket stream and renders its full message history. User messages       │
│  are posted via the same POST /api/v1/sessions/:id/messages endpoint.          │
│  The user's main chat is paused/backgrounded while attached.                │
└─────────────────────────────────────────────────────────────────────────────┘
```

**When to use which:**

| Scenario | Mode | Why |
|----------|------|-----|
| Quick course correction ("also do X") | Injection | One-shot, don't need to follow the conversation |
| Agent is blocked (needs human input) | Attachment | Need to see the context to provide guidance |
| Debugging a stuck agent | Attachment | Need to see what it's doing, guide it step-by-step |
| Reviewing agent's work in progress | Attachment (read-only) | Watch without intervening |
| Batch oversight ("tell all 3 tasks to skip tests") | Injection | Fire-and-forget across multiple sessions |
| Collaborative pair-programming with agent | Attachment | Sustained back-and-forth dialog |

#### Injection: How the Main Chat Does It

The main chat agent has a `send_message` tool. When the user says "tell the auth task to also handle timeouts," the main chat agent figures out which session to target and sends the message.

```typescript
// Tool available to the main chat agent
const send_message: ToolDefinition = {
  name: 'send_message',
  description: 'Send a message to another agent session (task or chat)',
  parameters: {
    sessionId: { type: 'string', description: 'Target session ID' },
    content: { type: 'string', description: 'Message to send' },
  },
};

// Implementation: just POSTs to the session API
async function executeSendMessage(args: { sessionId: string; content: string }) {
  await fetch(`/api/v1/sessions/${args.sessionId}/messages`, {
    method: 'POST',
    body: JSON.stringify({
      role: 'user',
      content: args.content,
      source: { 
        type: 'injection', 
        fromSession: currentSessionId,    // The main chat's session
        fromUser: currentUserId,
      },
    }),
  });
  
  return { success: true, message: `Message sent to session ${args.sessionId}` };
}
```

The agent resolves the target session from the task graph context (the task's session ID is available via the Jobs & Tasks API):
```
User: "tell the auth task to skip the integration tests"

Main agent thinks:
  1. Look up the auth task's session ID from the task graph
  2. send_message({ sessionId: "S-456", content: "Skip integration tests, only run unit tests" })

Main agent responds: "Done — I've told the auth task agent to skip integration tests."
```

#### Attachment: Joining a Task Session

Attachment connects the user's browser directly to the task's session stream. The user becomes a live participant.

```typescript
// Client-side: attach to a running task session
async function attachToTask(sessionId: string): Promise<TaskAttachment> {
  // 1. Subscribe to the task's message stream
  const ws = new WebSocket(`/api/v1/sessions/${sessionId}/stream`);
  
  // 2. Fetch full message history (user sees what the agent has done so far)
  const history = await fetch(`/api/v1/sessions/${sessionId}/messages`);
  
  // 3. Render task view (replaces or splits with main chat)
  return {
    sessionId,
    ws,
    history: await history.json(),
    
    // Send a message as the user (inline in the task conversation)
    send: (content: string) => fetch(`/api/v1/sessions/${sessionId}/messages`, {
      method: 'POST',
      body: JSON.stringify({
        role: 'user',
        content,
        source: { type: 'web_client', userId: currentUserId, connectionId: ws.id },
      }),
    }),
    
    // Detach: stop rendering the task stream, return to main chat
    detach: () => {
      ws.close();
      // Main chat resumes in the UI
    },
  };
}
```

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Attachment Lifecycle                                       │
│                                                                              │
│  1. User clicks a task in the task list (or agent suggests attachment)      │
│       → "Attach to task: Fix auth error handling"                           │
│                                                                              │
│  2. Browser subscribes to WS /api/v1/sessions/S-456/stream                    │
│     Browser fetches GET /api/v1/sessions/S-456/messages (full history)         │
│                                                                              │
│  3. UI switches to task view:                                               │
│     ┌─────────────────────────────────────────────────────────────┐         │
│     │  Left panel: task conversation (agent + user messages)      │         │
│     │  Right panel: editor (draft overlay)                        │         │
│     │    • .g5/specs/, .g5/docs/ → DocumentEditor (rich/source)   │         │
│     │    • code files → MonacoEditor                              │         │
│     │  Bottom: terminal (if worker has PTY)                       │         │
│     └─────────────────────────────────────────────────────────────┘         │
│                                                                              │
│  4. User types in the task chat → POST /api/v1/sessions/S-456/messages        │
│     Agent sees the message on next inbox drain, responds in context         │
│                                                                              │
│  5. User clicks [Detach] → WS closes, UI returns to main chat             │
│     Task agent continues running uninterrupted                              │
│                                                                              │
│  Key behavior:                                                               │
│  • The agent doesn't know or care whether the user is attached — it        │
│    just processes inbox messages as usual (§10.8)                            │
│  • Multiple users can attach simultaneously (read-only viewers are          │
│    just WS subscribers that never POST messages)                            │
│  • Attachment is non-destructive: attaching/detaching doesn't pause,       │
│    restart, or modify the agent's execution                                 │
│  • The attached view shares the same CRDT draft session as the task's      │
│    branch — user can see (and contribute to) the agent's edits in the      │
│    draft overlay                                                            │
│  • If the agent enters BLOCKED while the user is attached, the reason      │
│    appears inline in the task chat — no notification needed, the user       │
│    is already there                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Attachment + Draft Integration

When a user attaches to a task, they join the task's **branch draft session** (§11.7). This means:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Attached to task S-456 (branch: task/auth-fix):                            │
│                                                                              │
│  • The task agent edits files → edits appear in the CRDT draft            │
│  • The user edits files (DocumentEditor for specs/docs, Monaco for code)  │
│    → same CRDT draft                                                       │
│  • Both see each other's changes in real time (Yjs awareness)              │
│  • Diff annotations in the gutter show both agent and user edits           │
│  • [Commit] and [Fork] buttons are available — user can commit the         │
│    agent's work-in-progress, or fork it to a new branch                    │
│                                                                              │
│  This is genuine pair programming: the agent writes code, the user          │
│  reviews it live in the draft, asks questions in the task chat, and         │
│  can commit when satisfied.                                                 │
│                                                                              │
│  The agent doesn't need to "pause" for review — the user watches the      │
│  draft evolve and intervenes (via chat or direct edit) whenever needed.    │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### UI Entry Points for Intervention

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    How the User Reaches Each Mode                            │
│                                                                              │
│  FROM MAIN CHAT:                                                            │
│    • Type naturally: "tell the auth task to skip tests"                     │
│      → Main agent uses send_message tool (Mode 1)                          │
│                                                                              │
│    • Type: "let me join the auth task" / "show me what it's doing"         │
│      → Main agent triggers attachment (Mode 2)                              │
│                                                                              │
│  FROM TASK LIST (sidebar):                                                  │
│    • Hover a running task → quick actions:                                  │
│      [💬 Send Message]  → opens injection dialog (Mode 1)                  │
│      [🔗 Attach]        → switches to task view (Mode 2)                   │
│      [⏸ Pause]          → control signal                                   │
│      [🛑 Abort]         → control signal                                   │
│                                                                              │
│  FROM NOTIFICATION (agent is blocked):                                      │
│    • Toast notification: "Task 'auth-fix' is blocked: should I..."         │
│      [Reply Inline]     → opens injection dialog (Mode 1)                  │
│      [Attach to Task]   → switches to task view (Mode 2)                   │
│                                                                              │
│  FROM STATUS BAR:                                                           │
│    • "3 tasks running" badge → click opens task list                       │
│    • Blinking indicator if any task is waiting for input                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 10.10 Task Allocation Flow

```
User creates task
       │
       ▼
┌──────────────┐
│  API Server  │
│              │
│  1. Create   │
│     session  │──────▶ Postgres: INSERT session
│     record   │
│              │
│  2. Provision│
│     worker   │──────▶ Schedule Kata pod from worker pool
└──────┬───────┘
       │
       │  ┌──────────────────────────────────────────────┐
       │  │  3. Set up worker                             │
       │  │      - Schedule Kata pod                      │
       │  │      - Clone repo → task branch               │
       │  │      - Hydrate caches                         │
       │  │      - Start agent loop on worker              │
       │  │      - Update session.loopLocation = "worker" │
       │  └──────────────────────────────────────────────┘
       │
       ▼
  Session is running.
  Any client can inject messages or control signals.
```

### 10.11 Worker Base Images

Every provisioned worker starts from a **base image**. This is a container snapshot of the workspace at a known state.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Image Registry                                     │
│                                                                              │
│  workspace-123/                                                               │
│    ├── latest               ← HEAD of main, rebuilt on every merge          │
│    ├── 2026-02-05-a1b2c3d   ← tagged snapshot (date + short SHA)           │
│    ├── 2026-02-04-d4e5f6a   ← previous snapshot                            │
│    ├── 2026-02-01-7890abc   ← weekly snapshot                              │
│    └── task-456-checkpoint   ← saved from a prior task's checkpoint         │
│                                                                              │
│  Base image contents:                                                       │
│    /workspace/              ← repo at the tagged commit                     │
│    /cache/                  ← pre-hydrated L1 caches                        │
│    toolchain binaries       ← language runtimes, compilers, linters         │
│    node_modules / .venv /   ← pre-installed dependencies                   │
│      target/ etc.                                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Image Selection:**
```typescript
interface WorkerProvisionRequest {
  sessionId: string;
  workspaceId: string;
  
  // Image selection — default is "workspace-latest"
  image?: string;              // Full image ref, e.g., "gcr.io/g5/workspace-123:latest"
  imageTag?: string;           // Shorthand: "latest", "2026-02-04-d4e5f6a"
  commitSha?: string;          // Build from specific commit (may require on-demand build)
  
  // Branch to create for the task
  taskBranch: string;          // e.g., "task/task-456"
  baseBranch?: string;         // e.g., "main" (default)
}
```

**Default resolution order:**
1. If `imageTag` provided → use that exact tag
2. If `commitSha` provided → find nearest cached image, then `git pull` to target
3. Otherwise → use `workspace-latest` (rebuilt on every merge to main)

**Image build pipeline:**
```
main branch merge
       │
       ▼
┌──────────────────┐     ┌──────────────────────────────────────────────┐
│  CI/CD pipeline  │────▶│  Build base image:                            │
│  (on merge)      │     │    1. Clone repo at new HEAD                  │
│                  │     │    2. Install dependencies                    │
│                  │     │    3. Warm caches (build once)                │
│                  │     │    4. Tag: workspace-{id}:latest                │
│                  │     │    5. Tag: workspace-{id}:{date}-{short-sha}   │
│                  │     │    6. Push to registry                        │
│                  │     │    7. Prune images older than retention policy │
└──────────────────┘     └──────────────────────────────────────────────┘
```

### 10.12 Worker Swap (Hot-Replace)

A session's worker can be **replaced** while the chat remains open in the browser. This enables:
- Switching to a different base image (e.g., rolling back to yesterday's snapshot)
- Replacing a worker that has diverged or become stale
- Scaling up to a larger worker (more CPU/RAM)

```typescript
// Swap worker for an active session
POST /api/v1/sessions/:sessionId/worker
{
  "action": "swap",
  "imageTag": "2026-02-04-d4e5f6a"    // optional: different base image
}
```

**Uncommitted work IS preserved on swap.** The CRDT draft session (§11.7) is server-authoritative — all file changes are persisted in the draft the instant they reach g5d. On swap, the new worker materializes the draft onto its filesystem and continues. See AGENTS.md §4 for the dirty tree preservation mechanism.

**Swap Flow:**
```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Worker Swap Protocol                             │
│                                                                          │
│  1. Pause agent loop                                                    │
│  2. On old worker:                                                      │
│     a. git push origin task-branch   ← ensure commits are pushed        │
│     b. Flush pending CRDT deltas to server                              │
│  3. Provision new worker from requested image                           │
│     a. Check out draft's baseCommit                                     │
│     b. Apply CRDT draft overlay (materialize uncommitted changes)       │
│     c. Warm caches                                                      │
│  4. Redirect agent loop to new worker                                   │
│  5. Resume agent loop                                                   │
│  6. Tear down old worker                                                │
│                                                                          │
│  Browser client stays connected throughout — sees a brief                │
│  "Swapping worker..." status, then resumes streaming.                   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 10.13 Session API Summary

```
# Session lifecycle                              (public: /api/v1/)
POST   /api/v1/sessions                           # Create new session
GET    /api/v1/sessions/:id                       # Get session details
DELETE /api/v1/sessions/:id                       # Delete session + cleanup

# Messages                                       (public: /api/v1/)
GET    /api/v1/sessions/:id/messages              # Full message history
POST   /api/v1/sessions/:id/messages              # Inject user message
GET    /api/v1/sessions/:id/messages?since=:idx   # Messages after index

# Control                                        (public: /api/v1/)
POST   /api/v1/sessions/:id/control              # pause / resume / abort / checkpoint

# Worker management                              (public: /api/v1/)
POST   /api/v1/sessions/:id/worker               # attach / detach / swap worker
GET    /api/v1/sessions/:id/worker               # Current worker status + image info

# Real-time (worker → editor only)               (internal: /api/internal/)
WS     /api/internal/sessions/:id/stream          # Agent loop event stream

# Git (worker mode only)                         (public: /api/v1/)
GET    /api/v1/sessions/:id/diff                  # Current diff
GET    /api/v1/sessions/:id/checkpoints           # List checkpoints
POST   /api/v1/sessions/:id/rollback              # Rollback to checkpoint
```

### 10.14 Chat Session History

All chat sessions — main chat, task agent dialogs, and attached conversations — are **persisted to the editor database** as browsable, linkable artifacts. Users can flip back to old dialogs, reference them in comments, and resume them.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Session Persistence Model                                 │
│                                                                              │
│  Every session is stored in full:                                           │
│    • Complete message history (user, agent, tool calls, tool results)       │
│    • LLM model + system prompt used                                        │
│    • Token counts, cost, duration                                           │
│    • Git state at each turn (branch, commits, checkpoints)                 │
│    • Which files were read/written/built/tested                            │
│    • Final status (completed, failed, aborted)                             │
│                                                                              │
│  Sessions are never deleted by default. They are the decision trail —      │
│  the record of why the code looks the way it does.                         │
│                                                                              │
│  Types of sessions persisted:                                               │
│    • Main chat — the user's primary conversation thread                    │
│    • Task session — an agent working on a task (possibly unattended)       │
│    • Attached session — when a user joined a running task (§10.9 Mode 2)  │
│      (these are the same session, the attachment is just a view)            │
│                                                                              │
│  Main chat sessions form a timeline:                                        │
│    ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐                     │
│    │ Chat 1  │  │ Chat 2  │  │ Chat 3  │  │ Chat 4  │  ← current         │
│    │ Jan 15  │  │ Jan 22  │  │ Feb 1   │  │ Feb 5   │                     │
│    │ 47 msgs │  │ 12 msgs │  │ 89 msgs │  │ 6 msgs  │                     │
│    │ ✓ done  │  │ ✓ done  │  │ ✓ done  │  │ ● active│                     │
│    └─────────┘  └─────────┘  └─────────┘  └─────────┘                     │
│                                                                              │
│  Users can:                                                                 │
│    • Browse past sessions in a sidebar (like chat app history)             │
│    • Open any session read-only to review the conversation                 │
│    • Resume a completed session (appends new messages, same context)        │
│    • Link to a session or specific message from comments/ontology:         │
│      "See discussion in [Chat 2, msg 7](/sessions/S-234#msg-7)"           │
│    • Search across all sessions (full-text search on messages)             │
│    • Fork a session (start new conversation from a point in history)       │
│                                                                              │
│  Each workspace has its own session timeline:                                 │
│    Workspace A → [ Chat 1, Chat 2, ... ]  + [ Task S-456, Task S-789, ... ] │
│    Workspace B → [ Chat 1, ... ]          + [ Task S-111, ... ]              │
└─────────────────────────────────────────────────────────────────────────────┘
```

```typescript
// Session list API
GET /api/v1/workspaces/:workspaceId/sessions?type=chat&limit=50&offset=0
// Returns paginated list of sessions for the workspace
{
  "sessions": [
    {
      "id": "S-456",
      "type": "chat",                // "chat" | "task"
      "title": "Discuss auth refactor",  // Auto-generated or user-set
      "status": "completed",
      "messageCount": 47,
      "participantCount": 2,         // Users who participated
      "model": "anthropic/claude-3.5-sonnet",
      "totalCostUsd": 0.42,
      "createdAt": "2026-01-15T10:00:00Z",
      "lastActivityAt": "2026-01-15T14:30:00Z",
      "summary": "Discussed auth module error handling...",  // LLM-generated
      "tags": ["auth", "error-handling"],                     // Auto or manual
      "linkedCommits": ["abc1234", "def5678"],                // Commits made during
      "linkedNodes": ["node:auth-module"],                    // Ontology nodes referenced
    },
    // ...
  ],
  "total": 23
}

// Session detail (full message history)
GET /api/v1/sessions/:id/messages
// Already defined in §10.8 — returns full message history

// Resume a completed session
POST /api/v1/sessions/:id/resume
// Status changes from "completed" → "running"
// Agent loop restarts with existing message history as context
// New messages append to the existing session

// Fork a session from a specific message
POST /api/v1/sessions/:id/fork
{
  "fromMessageIndex": 12,           // Start new session from this point
  "title": "Alternative approach"    // Optional title for the fork
}
// Creates a new session with messages 0..12 copied as context
// Returns the new session ID

// Search across sessions
GET /api/v1/workspaces/:workspaceId/sessions/search?q=timeout+error+handling
// Full-text search across all session messages in the workspace
// Returns matching sessions with highlighted message excerpts

// Auto-generate session title + summary (called on session complete)
POST /api/v1/sessions/:id/summarize
// Uses the session's own LLM model to generate:
//   - title (5-8 words)
//   - summary (2-3 sentences)
//   - tags (extracted topics)
//   - linkedNodes (ontology nodes referenced in conversation)
```

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    UI: Session History Sidebar                                │
│                                                                              │
│  ┌──────────────────────────────────────────┐                               │
│  │  💬 Chat History           [🔍] [+ New] │                               │
│  │                                          │                               │
│  │  ● Feb 5 — Current session               │                               │
│  │    "Setting up draft model"              │                               │
│  │    6 messages · active                   │                               │
│  │                                          │                               │
│  │  ✓ Feb 1 — Discuss auth refactor         │                               │
│  │    89 messages · $0.42                   │                               │
│  │    → 2 commits, auth-module              │                               │
│  │                                          │                               │
│  │  ✓ Jan 22 — Quick question               │                               │
│  │    12 messages · $0.08                   │                               │
│  │                                          │                               │
│  │  ─── Tasks ──────────────────            │                               │
│  │                                          │                               │
│  │  ✓ S-456 Fix auth error handling         │                               │
│  │    142 messages · 3 checkpoints          │                               │
│  │    branch: task/auth-fix (merged)        │                               │
│  │                                          │                               │
│  │  ● S-789 Add caching layer              │                               │
│  │    67 messages · running                 │                               │
│  │    branch: task/cache-layer              │                               │
│  │    [Attach] [Inject Message]             │                               │
│  └──────────────────────────────────────────┘                               │
│                                                                              │
│  Clicking a completed session opens it read-only in the main panel.        │
│  [Resume] button appears at the bottom to continue the conversation.       │
│  Active task sessions show [Attach] and [Inject Message] actions (§10.9). │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Session titles** are auto-generated by the LLM when a session completes (or after the first few messages, for in-progress sessions). Users can rename them manually. The title appears in the sidebar, in linked references, and in search results.

**Session linking** creates a traceable decision trail: ontology comments can reference sessions ("See [Chat: Auth refactor, msg 7](/sessions/S-234#msg-7) for the rationale"), and sessions reference the commits they produced. This means you can follow the chain: *why was this code written?* → session → *what was the context?* → ontology node → *what comments exist?* → cross-branch threads.

---

## 11. Unified Instance Architecture

### 11.1 One Binary, Two Roles

There is a single binary — `g5d` — that runs in one of two roles:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              g5d                                             │
│                     (one binary, two roles)                                  │
│                                                                              │
│  ┌──────────────────────────────────┐  ┌─────────────────────────────────┐ │
│  │  Role: EDITOR                    │  │  Role: WORKER                   │ │
│  │                                  │  │                                 │ │
│  │  • Agent loop (chat host)        │  │  • Search index                │ │
│  │  • Connection broker (§11.6)     │  │                                 │ │
│  │  • Git checkout (user's branch)  │  │  • Agent loop (coding)         │ │
│  │  • File editing + git commit     │  │  • Git worktree (task branch)  │ │
│  │  • Search index                  │  │  • File tree + ontology + files│ │
│  │  • Ontology viewer               │  │  • Build + test + shell        │ │
│  │  • WebSocket ← browser connects  │  │  • Toolchain caches            │ │
│  │                                  │  │  • Checkpoints + rollback      │ │
│  │  No build, no test,              │  │  • Terminal (PTY)              │ │
│  │  no shell, no terminal           │  │  • File watch events           │ │
│  └──────────────────────────────────┘  │                                 │ │
│                                        └─────────────────────────────────┘ │
│                                                                              │
│  Same process. Same code. Different config.                                 │
│  g5d --role editor --workspace ./myproject --port 7500                        │
│  g5d --role worker --workspace ./myproject --task task-456 --port 7501        │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.2 Local Multi-Instance

A developer runs as many instances as their machine can handle:

```
Developer's Laptop
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  ┌─────────────────────────┐                                                │
│  │  g5d (editor)           │◄──── Browser connects here                     │
│  │  :7500                  │      Chat, connection broker, fallback browse  │
│  │  pinned: main @ abc123  │      (views come from worker when connected)   │
│  └─────────────────────────┘                                                │
│                                                                              │
│  ┌─────────────────────────┐  ┌─────────────────────────┐                  │
│  │  g5d (worker)           │  │  g5d (worker)           │                  │
│  │  :7501                  │  │  :7502                  │                  │
│  │  task: task-456         │  │  task: task-789         │                  │
│  │  branch: task/task-456  │  │  branch: task/task-789  │                  │
│  │  worktree: .g5/work/456 │  │  worktree: .g5/work/789 │                  │
│  │  agent loop: running    │  │  agent loop: paused     │                  │
│  └─────────────────────────┘  └─────────────────────────┘                  │
│                                                                              │
│  All share the same repo on disk.                                           │
│  Editor uses the main checkout. Workers use git worktrees.                  │
│  No containers, no VMs — just processes.                                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Desktop: git worktrees for isolation.** Each worker gets its own worktree branched from the base ref. Multiple agents can code in parallel without stepping on each other or the user's working tree.

```bash
# User starts the editor
$ g5d --role editor --workspace . --port 7500 &

# Spawn a local worker for a task
$ g5d --role worker --workspace . --task task-456 --port 7501 &

# Spawn another
$ g5d --role worker --workspace . --task task-789 --port 7502 &

# Or let the editor auto-spawn workers:
$ g5d --role editor --workspace . --port 7500 --local-workers 3
# Reserves 3 local worker slots, provisions on demand when tasks are created
```

### 11.3 Cloud Overflow

When the local machine doesn't have capacity (or the user doesn't have one running), the control plane provisions cloud workers. From the session's perspective, it doesn't matter — the protocol is identical.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Worker Provisioning                                  │
│                                                                              │
│  Task created → where to run?                                               │
│                                                                              │
│  1. Check local worker pool                                                 │
│     ├── Slot available? → spawn g5d --role worker locally                   │
│     └── No slots?                                                           │
│                                                                              │
│  2. Check cloud worker pool                                                 │
│     ├── Capacity available? → provision cloud worker (Kata VM)              │
│     └── No capacity?                                                        │
│                                                                              │
│  3. Queue the task (wait for a slot to free up)                             │
│                                                                              │
│  Priority: local first (zero latency, free), cloud as overflow.             │
│                                                                              │
│  The session's agent loop doesn't know or care where its worker is.         │
│  It calls tools via the same API. Results come back the same way.           │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Hybrid example — 5 tasks, 2 local slots:**
```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Laptop (2 local worker slots)          Cloud (GKE)                         │
│  ┌──────────────────────────┐           ┌──────────────────────────┐       │
│  │  g5d editor (:7500)      │           │  g5d worker (task-003)   │       │
│  │  g5d worker  (task-001)  │           │  g5d worker (task-004)   │       │
│  │  g5d worker  (task-002)  │           │  g5d worker (task-005)   │       │
│  └──────────────────────────┘           └──────────────────────────┘       │
│                                                                              │
│  Browser → editor (:7500)                                                   │
│  Tasks 1-2: local workers (fast, free)                                      │
│  Tasks 3-5: cloud workers (Kata VMs, billed per-minute)                    │
│                                                                              │
│  All 5 agent loops stream back to the same browser session.                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.4 Instance Configuration

```typescript
interface G5DConfig {
  role: 'editor' | 'worker';
  workspacePath: string;               // Path to repo root
  port: number;
  
  // Identity
  workspaceId: string;
  tenantId?: string;
  
  // Git
  gitRef: string;                    // Editor: branch to pin to. Worker: base for task branch.
  
  // Worker-specific
  taskId?: string;                   // Which task this worker serves
  taskBranch?: string;               // Auto: "task/{taskId}"
  worktreePath?: string;             // Auto: ".g5/worktrees/{taskId}"
  
  // Local pool (editor only)
  localWorkerSlots?: number;         // Max concurrent local workers (default: 2)
  
  // Cloud connection
  controlPlaneUrl?: string;          // e.g., "https://api.g5.dev"
  // If set: registers with control plane, cloud can provision overflow workers
  // If unset: fully offline, local workers only
  
  // Resources
  maxMemoryMb?: number;              // Per-instance memory limit
  
  // Lifecycle
  idleTimeoutMinutes?: number;       // Worker auto-stops after inactivity
}
```

### 11.5 Desktop vs Cloud Workers

Same binary, different isolation guarantees:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                Desktop Worker              Cloud Worker                       │
│  ─────────────────────────────    ───────────────────────────────           │
│  Binary:     g5d                  Binary:     g5d                           │
│  Isolation:  Process + worktree   Isolation:  Kata VM + container           │
│  Filesystem: Git worktree on      Filesystem: Cloned repo in               │
│              local disk                       ephemeral disk                │
│  Caches:     Shared local         Caches:     L1 local + L2 GCS            │
│              (same ~/.cache)                  (§4-9 cache strategy)         │
│  Network:    Same machine,        Network:    Control plane API,            │
│              localhost IPC                     mTLS between pods             │
│              Ports bind directly   Ports:      Reverse tunnel via WS         │
│                                                (§1.4.2) — outbound-only     │
│  DinD:       Native Docker/podman  DinD:       podman in Kata VM (§1.4.3)   │
│  Security:   User's own code,     Security:   Multi-tenant, untrusted       │
│              no isolation needed               code, VM-level isolation     │
│  Cost:       $0                   Cost:       Per-minute compute            │
│  Startup:    ~2-5s (worktree +    Startup:    ~10-30s (image pull +         │
│              cache warm)                       cache hydration)              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.6 Browser ↔ Editor ↔ Workers

The browser connects to the editor instance. The editor routes to workers.

```
┌──────────┐         ┌─────────────────────────┐
│ Browser  │◄──ws───▶│  g5d (editor)            │
│ (Monaco) │         │                          │
│          │         │  Chat ← agent loop here   │
│          │         │  Search ← editor serves  │
│          │         │  File browse ← editor    │
│          │         │  Connection broker        │
└──────┬───┘         │                          │
       │             │  Task panel:             │
       │             │    task-456 → worker A   │
       │             │    task-789 → worker B   │
       │             │    task-012 → worker C   │
       │             └────────┬────────────────┘
       │                      │
       │           ┌──────────┼──────────┐
       │           │          │          │
       │           ▼          ▼          ▼
       │    ┌──────────┐ ┌──────────┐ ┌──────────┐
       │    │ worker A │ │ worker B │ │ worker C │
       └─ws─▶ (local)  │ │ (local)  │ │ (cloud)  │
 (direct)   │ :7501    │ │ :7502    │ │ Kata VM  │
            │ streams: │ │ streams: │ │ streams: │
            │ UI conn  │ │ agent    │ │ UI conn  │
            └──────────┘ └──────────┘ └──────────┘

UI streams on workers are conditional:
  • Browser connects to worker for UI → UI streams become active
  • Worker is agent-only (no browser connected) → UI streams stay off

Editor routes agent tool calls to workers.
Browser connects directly to workers for data streams (terminal, logs).
Both connections use signed tokens issued by g5-api (brokered by the editor).

Agent loop on the editor sends tool calls:
  No worker:                          Worker connected:
  read_file   → editor checkout       read_file       → worker filesystem
  write_file  → ❌ unavailable         write_file      → worker
  run_in_terminal → ❌ unavailable     run_in_terminal → worker (persistent PTY)

Browser opens direct data channels to workers:
  terminal    → worker (PTY stream)
  build-log   → worker (compilation output)
  test-output → worker (test results)
  file-watch  → worker (file tree, ontology viewer, workspace view updates)
  git-status  → worker (branch, dirty files, upstream behind/ahead)
```

#### Direct Browser ↔ Worker Connections

From the browser's perspective, every service is just **"WebSocket to a URL with a signed token."** The browser does not know or care whether the URL points to localhost or a cloud gateway. The editor hands back a `{ url, token }` pair, and the browser connects.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                Uniform Connection Model                                      │
│                                                                              │
│  The browser's view:                                                        │
│                                                                              │
│    1. Ask editor for a connection to resource X                             │
│       POST /api/v1/connect { resource: "worker:w-123", streams: ["terminal"] }│
│                                                                              │
│    2. Editor checks permissions (§12)                                       │
│       → requests a signed token from g5-api                                 │
│                                                                              │
│    3. Editor returns:                                                       │
│       { "url": "<some websocket url>", "token": "<signed JWT>" }           │
│       (token minted by g5-api)                                              │
│                                                                              │
│    4. Browser opens WebSocket to url with token. Done.                      │
│                                                                              │
│  The browser never builds URLs or decides routing.                          │
│  It always asks the editor, gets a URL + token, and connects.              │
│                                                                              │
│  ─────────────────────────────────────────────────────────────────────────  │
│                                                                              │
│  What the editor returns depends on topology:                               │
│                                                                              │
│  Local worker (same machine):                                               │
│    { "url": "ws://localhost:7501/stream", "token": "ey..." }              │
│    → browser connects directly to worker process on localhost              │
│    → editor is NOT in the data path. Zero-copy, native latency.            │
│                                                                              │
│  Cloud worker (Kata VM, no direct route):                                   │
│    { "url": "wss://edge.g5.dev/stream/w-456", "token": "ey..." }         │
│    → browser connects to a cloud edge gateway                              │
│    → edge gateway validates token and tunnels to worker pod via mTLS       │
│    → editor is NOT in the data path either. Edge handles it.               │
│                                                                              │
│  Editor connection (chat, search, file browse):                             │
│    The initial browser ↔ editor WebSocket uses the user's OAuth token      │
│    directly. No signed JWT needed — the editor IS the destination.         │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key insight:** g5-api is the **token issuer**; the editor is the **connection broker**, not a proxy. It never sits in the data path for streaming. For local workers the browser hits localhost directly. For cloud workers a stateless edge gateway handles the tunnel. The editor's job is to decide *whether* you can connect and *where* to connect — then get out of the way.

```
┌──────────┐      ┌──────────────────┐
│ Browser  │      │ g5d (editor)     │
│          │      │  • Agent loop    │
│  control ──ws──▶│  • Search + file │
│  plane   │      │  • Connection broker │
│          │      └────────┬─────────┘
│          │               │ manages
│          │      ┌────────┴─────────┐
│          │      │                  │
│  data    │   ┌──▼─────┐    ┌──────▼──────┐
│  plane   │   │ LOCAL  │    │ CLOUD EDGE  │
│          │   │ worker │    │ gateway     │
│          ──ws──▶:7501  │    │ edge.g5.dev │
│          │   │        │    │     │       │
│          │   └────────┘    │  ┌──▼────┐  │
│          │                 │  │worker │  │
│          ──ws──────────────▶──▶Kata VM│  │
│          │                 │  └───────┘  │
│          │                 └─────────────┘
└──────────┘
```

#### Signed Token Model

All data-plane connections use the same token format. The token encodes what you're allowed to do.

```typescript
// Token payload (JWT, signed by g5-api's key)
interface DataChannelToken {
  // Standard JWT claims
  sub: string;                      // "user:alice" or "sa:ci-bot"
  iss: string;                      // "g5-api:instance-abc"  (issuing API instance)
  iat: number;                      // Issued-at timestamp
  exp: number;                      // Expiry: iat + 300s (5 minutes)
  
  // G5-specific claims
  worker: string;                   // "w-123" — which worker this token is for
  streams: string[];                // ["terminal", "build-log"] — allowed streams
  permissions: string[];            // ["read", "execute"] — what ops are allowed
}

// Token lifecycle:
//   1. Browser holds user's long-lived OAuth token (for editor connection)
//   2. When it needs a data channel, it POSTs to editor with the OAuth token
//   3. g5-api mints a short-lived JWT scoped to that specific worker + streams
//   4. Browser uses the JWT to connect to the worker (or edge gateway)
//   5. Worker/gateway validates JWT signature + expiry locally (no callback)
//   6. On expiry: browser transparently re-requests from editor, reconnects
//
// Why not just use the OAuth token directly?
//   • OAuth tokens are long-lived and broadly scoped (user identity)
//   • Data channel tokens are short-lived and narrowly scoped (one worker, specific streams)
//   • If a data channel token leaks, damage is limited to 5 minutes on one worker
//   • Workers don't need to be OAuth-aware — they just verify a JWT signature
```

#### Edge Gateway (Cloud)

For cloud workers, a lightweight stateless gateway terminates browser WebSockets and tunnels to worker pods. The editor never proxies data.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                 Cloud Edge Gateway                                            │
│                                                                              │
│  Runs as a K8s Deployment (2+ replicas behind a load balancer).             │
│  Stateless — all state is in the signed token.                              │
│                                                                              │
│  On WebSocket connect:                                                      │
│    1. Extract token from query param or Sec-WebSocket-Protocol              │
│    2. Validate JWT signature (g5-api public key, fetched on startup)        │
│    3. Check expiry                                                          │
│    4. Look up worker pod IP from token.worker claim (K8s service discovery) │
│    5. Open mTLS WebSocket to worker pod                                     │
│    6. Bidirectional byte-level proxy. No parsing, no buffering.             │
│                                                                              │
│  The edge gateway does NOT:                                                 │
│    • Run agent loops                                                        │
│    • Store session state                                                    │
│    • Check permissions beyond JWT validation                                │
│    • Know about G5's domain model                                           │
│                                                                              │
│  It's ~200 lines of Rust. A WebSocket reverse proxy with JWT auth.         │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Data Channel Streams

Each direct connection is scoped to a named stream. The browser opens one WebSocket per stream it needs.

| Stream | Direction | Payload | Why direct matters |
|--------|-----------|---------|-------------------|
| `terminal` | worker → browser (+ input) | PTY frames, ANSI sequences | Latency-sensitive keystrokes, high throughput output |
| `build-log` | worker → browser | Structured log lines (JSON-ND) | Hundreds of lines/sec during compilation |
| `test-output` | worker → browser | TAP/JUnit streaming results | Live test progress without buffering |
| `file-watch` | worker → browser | File change events | Instant refresh on save |
| `file-content` | bidirectional | Raw bytes | Large file preview, binary downloads |

```typescript
// Browser-side: opening any data channel (local or cloud — same code)
async function openDataChannel(
  editorUrl: string,
  workerId: string, 
  stream: string,
  authToken: string,
): Promise<WebSocket> {
  // 1. Ask editor for a connection (editor decides local vs. cloud)
  const res = await fetch(`${editorUrl}/api/v1/connect`, {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${authToken}` },
    body: JSON.stringify({ resource: `worker:${workerId}`, streams: [stream] }),
  });
  const { url, token } = await res.json();
  // url might be:
  //   "ws://localhost:7501/stream"              (local worker)
  //   "wss://edge.g5.dev/stream/w-456"         (cloud worker via edge gateway)
  // Browser doesn't care which. Same code path.

  // 2. Connect to whatever URL we got, with the signed token
  const ws = new WebSocket(`${url}/${stream}?token=${token}`);
  return ws;
}

// Usage: terminal stream (works identically for local and cloud)
const termWs = await openDataChannel(
  'http://localhost:7500',  // always the editor
  'w-123',                  // worker ID
  'terminal',               // stream name
  userOAuthToken,           // user's long-lived OAuth token
);
termWs.onmessage = (e) => xterm.write(e.data);
xterm.onData((data) => termWs.send(data));

// Note: LSP/IntelliSense is intentionally **not** a g5d stream.
// Users access language servers via **Open in IDE**.
```

#### Connection Preference: Local First

When the editor issues a `{ url, token }` pair, it always prefers local workers:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                  Connection Preference Order                                  │
│                                                                              │
│  1. Local worker on localhost  (if available)                               │
│     → ws://localhost:750x — zero network hops, sub-ms latency              │
│     → This is the default for any developer running g5d on their laptop    │
│                                                                              │
│  2. Cloud worker via edge gateway  (overflow / remote-only)                 │
│     → wss://edge.g5.dev/... — regional edge, ~20-50ms                      │
│     → Used when: no local slots, user explicitly requests cloud,           │
│       CI/CD pipelines, deployed agent sessions                              │
│                                                                              │
│  The editor decides. The browser doesn't choose.                            │
│  If a local worker exists for the task, the editor always routes there.     │
│  Cloud is never preferred over local for the same task.                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.7 Chat and the Agent Loop

The agent loop runs on the assigned **worker**. All tools — including reads, writes, builds, and tests — execute on the worker's local filesystem.

```
Agent loop always runs on the worker (Kata container).

No worker attached?
  → Task cannot execute (queued until worker available)

Worker attached?
  → full tools (read, write, build, test)
```

This means:
- The **worker** runs the agent loop — it calls the LLM, parses tool calls, executes tools on its local filesystem, and streams results back
- The **editor instance** is the session broker — it persists the dialog to Postgres, relays streaming events to the browser via WebSocket, and manages connection routing
- The editor does NOT run the agent loop itself — it receives timeline events from the worker and forwards them
- Workers are self-contained: they hold the ReAct loop, the tool executor, and the filesystem. The editor is stateless with respect to agent reasoning
- A worker can be local (process on same machine) or cloud (Kata VM) — transparent to the agent loop
- Multiple chat sessions can each have their own worker, or share one, or have none

#### Feature Availability Without a Worker

G5 is useful even without a worker configured — but many features require one. The UI should clearly indicate degraded state and offer to provision a worker when the user tries something that needs one.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                 Feature Availability by Mode                                  │
│                                                                              │
│                              Editor Only     Editor + Worker                │
│  Feature                     (no worker)     (local or cloud)               │
│  ───────────────────────     ──────────────  ────────────────               │
│                                                                              │
│  File tree                   ✅              ✅ (worker FS, live updates)  │
│  File content (Monaco)       ✅ (editable)   ✅ (worker FS, editable)      │
│  Ontology viewer             ✅ (editable)   ✅ (worker FS, editable)      │
│  Workspace view              ✅              ✅ (worker FS, live updates)  │
│  Search (text, regex)        ✅              ✅ (from worker index)         │
│  Git log / blame / diff      ✅              ✅ (worker's branch)           │
│  File editing (save)         ✅              ✅                             │
│  Git commit / push           ✅              ✅                             │
│  Branch creation             ✅              ✅                             │
│  Chat (conversation only)    ✅              ✅                             │
│  Agent: answer questions     ✅              ✅                             │
│  Agent: read/analyze code    ✅              ✅                             │
│  Agent: write / edit files   ✅              ✅                             │
│                                                                              │
│  Terminal                    ❌              ✅                             │
│  Build                       ❌              ✅                             │
│  Test                        ❌              ✅                             │
│  Agent: run commands         ❌              ✅                             │
│                                                                              │
│  ─────────────────────────────────────────────────────────────────────────  │
│                                                                              │
│  UI behavior without a worker:                                              │
│                                                                              │
│  • Monaco loads with syntax highlighting (TextMate grammars) and full      │
│    editing support. IntelliSense is via **Open in IDE**.                    │
│  • You can edit files, save, commit, push, create branches — all git       │
│    operations work on the editor's checkout                                 │
│  • Status bar shows: "No worker" with a one-click                          │
│    "Start Worker" button                                                   │
│  • Attempting a build/test/terminal action shows a prompt:                  │
│    "This requires a worker. Start one locally (2s) or in the cloud (15s)?"│
│  • Chat agent can answer questions, read code, and write files.            │
│    It tells the user when it needs a worker for build/test:                │
│    "I've made the fix. Want me to start a worker to run the tests?"       │
│  • Once a worker is attached, build/test/terminal become available.         │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Graceful Degradation Ladder

The browser must be useful at **every level** of backend availability. Workers can be broken, editors can be temporarily unreachable, and the user should never be blocked from basic work.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Degradation Levels                                       │
│                                                                              │
│  Level 1: Editor + Worker (full)                                            │
│  ──────────────────────────────                                              │
│    Everything works: editing, build, test, terminal, ontology,              │
│    workspace view, all served from worker filesystem.                         │
│                                                                              │
│  Level 2: Editor + Worker (worker broken)                                   │
│  ─────────────────────────────────────────                                   │
│    Worker is connected but unhealthy: build fails,                          │
│    dependencies missing, etc.                                               │
│                                                                              │
│    Still works:                                                             │
│      • Editing files on the worker filesystem (save still works)           │
│      • Viewing file tree, ontology, workspace view (filesystem ok)           │
│      • Git commit, push (git works even if the workspace is broken)          │
│      • Terminal (the workspace may be broken but the shell is fine)           │
│      • Chat + agent (can read code, suggest fixes, edit files)             │
│      • Commenting / annotations on ontology nodes                          │
│      • Viewing workflow status (task queue, execution history)              │
│                                                                             │
│    Degraded:                                                                │
│      • Build fails → build output shows errors (user can still edit)       │
│      • Test fails → test output shows failures (not blocking)              │
│                                                                             │
│    UI: status bar shows "● Worker: build broken"                            │
│    with restart options. Editing is never blocked by worker health.         │
│                                                                              │
│  Level 3: Editor only (no worker)                                           │
│  ────────────────────────────────                                            │
│    No worker connected at all.                                              │
│                                                                              │
│    Still works:                                                             │
│      • Editing files (saved to editor's checkout)                          │
│      • Git commit, push, branch creation                                   │
│      • File tree, ontology viewer, workspace view (from editor checkout)     │
│      • Search (text/regex on editor index)                                 │
│      • Commenting / annotations (persisted by editor to git or DB)         │
│      • Viewing workflow status (from editor's state / control plane DB)    │
│      • Chat + agent (can read, analyze, and write code)                    │
│                                                                             │
│    Unavailable:                                                             │
│      • Terminal, build, test                                                │
│                                                                              │
│  Level 4: Browser only (editor temporarily unreachable)                     │
│  ──────────────────────────────────────────────────────                       │
│    WebSocket to editor dropped. Network hiccup, editor restarting,          │
│    laptop lid closed and reopened.                                           │
│                                                                              │
│    Still works (browser-local):                                             │
│      • Editing open files (Monaco buffers are in browser memory)           │
│      • Viewing already-loaded file tree, ontology, workflows               │
│      • Writing comments on already-loaded ontology nodes                   │
│      • Browsing already-loaded search results                              │
│                                                                             │
│    Queued for sync:                                                         │
│      • File saves → queued in IndexedDB, flushed when editor reconnects   │
│      • Comments → queued locally, synced on reconnect                      │
│      • Git operations → queued, executed on reconnect                      │
│                                                                             │
│    UI: banner "Reconnecting to editor..." with spinner. Edits continue    │
│    in Monaco. Save indicator shows "⏳ pending sync". When editor          │
│    reconnects, queued operations flush automatically.                       │
│                                                                              │
│  Key principle: EDITING IS NEVER BLOCKED.                                   │
│  The user can always type, always comment, always view.                     │
│  Only execution (build/test/terminal) requires a healthy worker.            │
│  Everything else degrades gracefully down to browser-local state.           │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Draft-Based Editing Model

All edits — file changes, comments, ontology annotations — are **drafts** until the user explicitly commits. There is no auto-save to disk. This applies uniformly whether a worker is connected or not, and naturally supports collaborative review.

**Draft state is server-authoritative.** The CRDT document is hosted on the editor process (g5d) and persisted to the editor's database. Every user who opens the same workspace URL sees the same draft state — same pending edits, same diff annotations, same comments. The draft is not a browser-local concept; it is a shared, durable, URL-addressable resource.

**The canonical state of any workspace view is always: base commit + draft diff.** Even when "clean" (no pending edits), the state is `{baseCommit: "abc1234", diff: ∅}`. This means:
- Every file view shows the base commit content overlaid with draft edits (if any)
- Diff annotations (gutter markers, inline highlights) are computed from the draft diff and visible to all participants
- The ontology viewer reflects draft-modified nodes (e.g., new files, changed dependencies)
- Workers also operate on this model — the worker's filesystem is the materialized base commit, and the CRDT draft is the overlay

**Drafts must be committed before uplift/downlift.** Rebasing, merging upstream, or switching branches requires a clean draft state. If there are uncommitted drafts when the user triggers a pull/rebase, the UI blocks with: "Commit or discard drafts before rebasing." This prevents the impossible task of rebasing a CRDT diff on top of a moving base.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      Draft → Commit Model                                    │
│                                                                              │
│  Canonical state:  { baseCommit: SHA, draftDiff: CRDT document }            │
│                                                                              │
│  1. User (or agent) edits files, writes comments, modifies ontology nodes  │
│     → All changes accumulate in the server-side CRDT draft session          │
│     → CRDT state is persisted to editor DB (survives process restart)       │
│     → All clients syncing to that draft see the same state                  │
│     → Diff annotations update in real time for everyone                     │
│                                                                              │
│  2. Draft is visible to all participants — including late joiners           │
│     → Opening the workspace URL loads the current draft state                 │
│     → Multiple users see each other's cursors and changes in real time     │
│     → Comments appear immediately in the ontology viewer                   │
│     → Agent edits are visible as they stream in                            │
│     → Diff gutter markers show which lines have draft changes              │
│                                                                              │
│  3. User clicks [Commit]:                                                   │
│     → Draft diff is materialized to filesystem (editor or worker)          │
│     → git add + git commit with a message                                  │
│     → baseCommit advances to the new SHA                                   │
│     → Draft session resets to clean: { baseCommit: NEW_SHA, diff: ∅ }     │
│     → All collaborators see the commit                                     │
│                                                                              │
│  4. User clicks [Fork]:                                                     │
│     → Creates a new branch from current baseCommit                         │
│     → Draft state carries over to the new branch's draft session           │
│     → Original branch's draft is reset to clean                            │
│     → Equivalent to: commit to new branch without touching the original    │
│                                                                              │
│  No intermediate "save to disk" step. Drafts persist server-side           │
│  (surviving tab close, reconnect, multi-tab) but are not git commits       │
│  until [Commit] is clicked.                                                │
└─────────────────────────────────────────────────────────────────────────────┘
```

**The Commit and Fork buttons** live in the status bar, next to the worker selector:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Status bar (with draft changes):                                           │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │ ● worker-A │ main @ abc1234 │ ↓2 │ ⟳ Ready │ 3 drafts │[Fork][Commit]│  │
│  └──────────────────────────────────────────────────────────────────────┘   │
│    ▲             ▲                ▲      ▲        ▲         ▲      ▲        │
│    Worker        Branch +         Pull   Worker   Draft     Fork   Commit   │
│    selector      base commit      badge  status   count     btn    btn      │
│                                                                              │
│  "3 drafts" — count of files/comments/annotations with uncommitted edits  │
│  Click "3 drafts" → opens draft review panel (diff staging area)          │
│  Click [Commit]   → opens commit dialog with message + diff summary       │
│  Click [Fork]     → creates new branch, moves drafts to it               │
│                                                                              │
│  [Fork] is always visible when there are drafts. It provides a way to     │
│  move work-in-progress to a new branch without committing to the current   │
│  branch. Flow: prompt for branch name → create branch from baseCommit →  │
│  transfer draft session to new branch → switch to new branch.             │
│                                                                              │
│  No draft changes:                                                          │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │ ● worker-A │ main @ abc1234 │ ↓2 │ ✓ Ready │ ✓ clean              │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  [Fork] and [Commit] are hidden when clean — no visual noise.             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Collaborative Draft Sessions

Multiple users (or a user + agent) can edit the same draft session simultaneously. Drafts are synced in real time using a CRDT (Conflict-free Replicated Data Type). **The server-side CRDT document is the single source of truth** — it is persisted to the editor's database, survives process restarts, and is loaded on demand when any client opens the workspace URL.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Collaborative Editing Architecture                         │
│                                                                              │
│  Draft sessions are server-authoritative and URL-addressable.               │
│  Opening /workspace/:workspaceId/branch/:branch always loads the current draft.      │
│  Late joiners see the exact same diff annotations, comments, and edits.    │
│                                                                              │
│  ┌──────────┐     ┌──────────┐     ┌─────────────────────────────┐        │
│  │ Alice    │     │ Bob      │     │ Agent (chat session)        │        │
│  │ (browser)│     │ (browser)│     │ (edits via tool calls)      │        │
│  └────┬─────┘     └────┬─────┘     └────────────┬────────────────┘        │
│       │                │                         │                          │
│       │  CRDT sync     │  CRDT sync              │  CRDT ops               │
│       ▼                ▼                         ▼                          │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │                    g5d (editor)  — CRDT authority                │       │
│  │                                                                   │       │
│  │  Draft Session DS-789 (persisted to DB, loaded on demand)         │       │
│  │  ┌─────────────────────────────────────────────────────────┐    │       │
│  │  │  Base commit:  abc1234  (the "clean" reference point)    │    │       │
│  │  │                                                           │    │       │
│  │  │  File drafts (CRDT diff overlay):                         │    │       │
│  │  │    src/main.rs  → +L45 (Alice), +L120 (Agent)             │    │       │
│  │  │    src/lib.rs   → ~L10-15 (Bob)                           │    │       │
│  │  │                                                           │    │       │
│  │  │  Comment drafts:                                          │    │       │
│  │  │    node:auth-module → Alice: "needs error handling"       │    │       │
│  │  │    node:auth-module → Bob: "agreed, see RFC-123"          │    │       │
│  │  │                                                           │    │       │
│  │  │  Ontology drafts:                                         │    │       │
│  │  │    node:cache-layer → Bob added dependency link           │    │       │
│  │  │                                                           │    │       │
│  │  │  Diff annotations (computed, shared with all clients):    │    │       │
│  │  │    gutter: green (+), yellow (~), red (-)                 │    │       │
│  │  │    inline: author attribution per hunk                    │    │       │
│  │  └─────────────────────────────────────────────────────────┘    │       │
│  │                                                                   │       │
│  │  On [Commit]:                                                     │       │
│  │    1. Materialize CRDT diff → file patches + comment records      │       │
│  │    2. Write files to filesystem (editor checkout or worker)       │       │
│  │    3. git add + git commit -m "message"                           │       │
│  │    4. Persist comments to DB                                      │       │
│  │    5. Update baseCommit → new SHA                                 │       │
│  │    6. Reset draft to clean: { baseCommit: NEW_SHA, diff: ∅ }     │       │
│  │    7. Broadcast commit event to all connected clients             │       │
│  │                                                                   │       │
│  │  On [Fork]:                                                       │       │
│  │    1. Prompt for new branch name                                  │       │
│  │    2. git checkout -b <name> from baseCommit                      │       │
│  │    3. Transfer CRDT draft session to new branch                   │       │
│  │    4. Reset original branch's draft to clean                      │       │
│  │    5. Switch UI to new branch (same draft state, new branch)      │       │
│  └───────────────────────────────────────────────────────────────────┘       │
│                                                                              │
│  CRDT choice: Yjs (proven, used by VS Code Live Share, Notion, etc.)       │
│  Persistence: Yjs doc serialized to editor DB (survives restarts)          │
│  Transport: WebSocket (same connection as the editor control channel)       │
│  Awareness: cursors, selections, and presence are shared via Yjs awareness │
│                                                                              │
│  Editor bindings (see FRONTEND_DESIGN.md §11.5):                           │
│    • Code files: y-monaco (Y.Text ↔ Monaco text buffer)                   │
│    • Specs/docs rich mode: y-prosemirror (Y.XmlFragment ↔ ProseMirror)    │
│    • Specs/docs source mode: y-monaco (same Y.Text, raw Markdown)          │
│  Only one binding is active per file at a time (based on editor mode).     │
└─────────────────────────────────────────────────────────────────────────────┘
```

```typescript
// Draft session data model — server-authoritative, persisted to DB
interface DraftSession {
  id: string;                         // DS-789
  workspaceId: string;
  branch: string;                     // Which branch this draft applies to
  baseCommit: string;                 // SHA of the commit this draft is based on
  
  // Server-side persistence
  yjsStateVector: Uint8Array;         // Serialized Yjs document state
  persistedAt: string;                // Last time CRDT was flushed to DB
  
  // Participants (live connections)
  participants: Participant[];
  
  // Draft content (CRDT-backed, server-authoritative)
  //
  // Code files use Y.Text (character-level CRDT, binds to Monaco).
  // Spec/doc files also use Y.Text as the canonical layer — the rich
  // editor (Tiptap/ProseMirror) parses on load and serializes on save.
  // See FRONTEND_DESIGN.md §11.5 for the mode-switch algorithm.
  fileDrafts: Map<string, YText>;     // path → Yjs text (tracks all edits)
  commentDrafts: YArray<CommentDraft>;
  ontologyDrafts: YMap<OntologyEdit>;
  
  // Computed diff (derived from baseCommit + fileDrafts)
  diffSummary: DiffSummary;           // Shared with all clients for annotations
  
  // State
  dirtyFiles: string[];               // Files with uncommitted edits
  dirtyComments: number;              // Count of draft comments
  createdAt: string;
}

interface DiffSummary {
  files: Array<{
    path: string;
    status: 'added' | 'modified' | 'deleted';
    hunks: Array<{ startLine: number; endLine: number; author: string }>;
  }>;
  totalAdditions: number;
  totalDeletions: number;
}

interface Participant {
  identity: Identity;                  // User or agent
  role: 'editor' | 'reviewer';        // Can edit or just comment
  cursor?: CursorPosition;            // Live cursor position
  connectedAt: string;
}

interface CommentDraft {
  id: string;
  nodeId: string;                      // Ontology node
  author: Identity;
  content: string;                     // Markdown
  scope: 'line' | 'node';             // Line = branch-scoped, Node = workspace-global
  lineRef?: { path: string; line: number };  // Required when scope = 'line'
  timestamp: string;
}

// Commit operation
POST /api/v1/drafts/:draftId/commit
{
  "message": "Fix auth error handling, add cache dependency",
  "push": true                         // Also push to remote after commit
}
// Returns 409 if baseCommit is stale (upstream moved) — rebase first
// Response
{
  "commit": "def5678",
  "baseCommit": "def5678",             // Updated base
  "filesChanged": 2,
  "commentsCreated": 2,
  "ontologyEditsApplied": 1
}

// Fork operation — move drafts to a new branch
POST /api/v1/drafts/:draftId/fork
{
  "branchName": "task/auth-review",
  "push": false                        // Push the new branch to remote?
}
// Response
{
  "newBranch": "task/auth-review",
  "newDraftId": "DS-790",             // New draft session on the new branch
  "baseCommit": "abc1234",
  "originalBranchReset": true          // Original branch's draft is now clean
}

// Get draft state (for late joiners, page loads)
GET /api/v1/drafts/:draftId
// Response includes full CRDT state vector for Yjs sync,
// plus computed diff summary for immediate rendering
{
  "id": "DS-789",
  "baseCommit": "abc1234",
  "branch": "main",
  "diffSummary": { ... },
  "participants": [ ... ],
  "dirtyFiles": ["src/main.rs", "src/lib.rs"],
  "dirtyComments": 2
}
```

#### How Concurrent Edits Are Resolved

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Same file, same line — two users type at the same position:               │
│    → CRDT (Yjs) merges character-by-character, both edits preserved        │
│    → Same behavior as Google Docs / VS Code Live Share                     │
│    → No locks, no conflicts, no "someone else is editing this file"        │
│                                                                              │
│  Same file, different lines — most common case:                            │
│    → Edits are independent, merged trivially                               │
│                                                                              │
│  Comments on the same node:                                                 │
│    → Both comments are appended (comments don't conflict, they accumulate) │
│                                                                              │
│  Agent edits while user is editing the same file:                          │
│    → Agent's edits stream into the CRDT as they arrive                     │
│    → Agent hunks are visually distinct (see Agent Edit Attribution below)  │
│    → User can undo/revert agent edits independently of their own           │
│    → When both edit the same region, agent yields to human (see below)     │
│                                                                              │
│  User + agent both editing, then user clicks [Commit]:                     │
│    → User reviews agent-authored hunks (highlighted) before committing     │
│    → Can selectively exclude agent hunks from the commit                   │
│    → Agent can keep editing (new drafts accumulate for next commit)        │
│                                                                              │
│  Two browser tabs, same user:                                              │
│    → Same server-side draft session, CRDT keeps them in sync              │
│    → Either tab can commit (or fork)                                       │
│                                                                              │
│  Late joiner (new tab, new user opens URL):                                │
│    → Receives full CRDT state from server on connect                       │
│    → Sees all pending edits, comments, diff annotations immediately       │
│    → No "catching up" — server state is authoritative                     │
│                                                                              │
│  User disconnects temporarily (Level 4 degradation):                       │
│    → Edits continue in local CRDT replica                                  │
│    → On reconnect: CRDT auto-merges with server state                      │
│    → No manual conflict resolution needed                                  │
│                                                                              │
│  Uplift / downlift (pull, rebase, merge upstream):                         │
│    → BLOCKED if draft is dirty — must [Commit] or [Discard] first         │
│    → Rationale: rebasing a CRDT diff on a moving base is undefined        │
│    → UI shows: "Commit or discard 3 drafts before rebasing"               │
│    → After commit: rebase proceeds normally, baseCommit advances           │
│                                                                              │
│  Draft sessions are per-branch:                                             │
│    → Switching workers = switching branches = switching draft sessions      │
│    → Each branch has its own server-side draft session                      │
│    → Drafts persist server-side (not lost on tab close or worker switch)   │
│    → Switching away from a dirty branch: drafts remain on the server       │
│      for that branch, ready when you switch back                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Agent Edit Attribution

Agent and human edits in the same CRDT draft are **visually distinct** and **independently manageable**. Every CRDT operation carries an `author` field (from the `Identity` on the `Participant`), which the frontend uses to render attribution.

**Visual distinction in the editor:**

| Element | Human edits | Agent edits |
|---------|-------------|-------------|
| Gutter marker | Standard green/yellow/red | Purple with robot icon |
| Inline highlight | None (standard text) | Faint purple background on changed lines |
| Diff hunk label | Username | Agent name + task ID (e.g., "Agent · auth/validator") |
| Cursor/selection | User's color (from Yjs awareness) | Agent color (distinct, non-overlapping) |

**Conflict resolution — human authority:**

The core invariant: **agents never silently overwrite human edits**. When an agent's tool call would modify a region that a human has edited since the agent's last read:

1. The worker detects the overlap by comparing the agent's `old_string` against the current CRDT content.
2. If the region has been modified by a human participant since the agent's task started (tracked via per-hunk author metadata), the edit is **rejected** with an error: `"Region modified by human since last read — re-read the file before editing."`
3. The agent must `read_file` again, observe the human's changes, and decide how to proceed.
4. If the agent re-issues the edit with the correct `old_string` (incorporating the human's changes), it succeeds — this proves the agent has seen and accounted for the human's work.

This is enforced at the tool level (`edit_file_replace`, `write_file`), not in the CRDT layer itself. The CRDT would happily merge concurrent edits, but the tool-level check prevents agents from unknowingly undoing human intent.

**Selective commit:**

When a user clicks [Commit] on a draft that contains both human and agent edits, the commit dialog shows a per-file breakdown with author attribution. The user can:
- **Commit all**: Both human and agent edits are committed together (default).
- **Commit mine only**: Only human-authored hunks are committed; agent hunks remain in the draft.
- **Review agent hunks**: Expand each agent hunk inline, with [Accept] / [Reject] per hunk.

This gives humans explicit control over which agent changes enter the commit history, without blocking the agent's workflow (rejected hunks stay in the draft and the agent can refine them).

#### Large Document Degradation

Yjs performs well for documents up to ~1–5 MB of text. Beyond that, CRDT merge overhead and history growth degrade performance. G5 degrades collaborative editing gracefully for large files:

| Document Size | Behavior |
|---------------|----------|
| **< 1 MB** (~25K lines) | Full collaborative editing. Real-time CRDT sync, remote cursors, live awareness. |
| **1–5 MB** (~25K–125K lines) | **Read-only collaboration.** File is CRDT-tracked for the writer (agent or single editor), but other participants see a read-only view refreshed on save/commit. No keystroke-level sync to observers. |
| **> 5 MB** | **No CRDT tracking.** File is excluded from the draft overlay entirely. Edits go directly to the worker filesystem. Diff is computed from git on demand (`git diff`). This is the fallback for generated files, data dumps, and vendored dependencies. |

**Implementation:**

```typescript
// Threshold constants
const CRDT_FULL_COLLAB_MAX_BYTES  = 1 * 1024 * 1024;   // 1 MB
const CRDT_SINGLE_WRITER_MAX_BYTES = 5 * 1024 * 1024;  // 5 MB

function getCrdtMode(fileSize: number): 'full' | 'single-writer' | 'none' {
  if (fileSize <= CRDT_FULL_COLLAB_MAX_BYTES) return 'full';
  if (fileSize <= CRDT_SINGLE_WRITER_MAX_BYTES) return 'single-writer';
  return 'none';
}
```

In `single-writer` mode, the `Y.Text` is still used for the primary editor (enabling undo/redo, transactional edits, and draft persistence), but awareness updates (cursors/selections) are not broadcast and remote `Y.Text` observers do not bind to the editor (Monaco or Tiptap) — they receive a snapshot on `textDocument/didSave` instead.

In `none` mode, the file is listed in `dirtyFiles` (for the diff summary) but its content is not stored in the `Y.Doc`. Agent tools (`write_file`, `edit_file_replace`) write directly to disk, and the post-`run_in_terminal` filesystem diff (AGENTS.md §4) detects changes.

> **Rationale:** The vast majority of source files are under 100 KB. Files exceeding 1 MB are almost always generated code, data fixtures, or vendored dependencies — not files humans collaboratively edit. This policy keeps the common path fast without limiting what agents can process.

#### Comment Scope: Branch-Local vs. Workspace-Global

Comments have two distinct natures, and conflating them is the gotcha:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                  Two Kinds of Comments                                        │
│                                                                              │
│  1. LINE-LEVEL COMMENTS (code review)                                       │
│     ─────────────────────────────────                                       │
│     Anchored to a specific file + line in a specific diff.                  │
│     Example: "This unwrap() will panic on None — use ? instead"            │
│     These are inherently branch-scoped:                                     │
│       • The line may not exist on other branches                            │
│       • The diff context is branch-specific                                 │
│       • They only make sense to people reviewing that branch                │
│                                                                              │
│     Lifecycle: draft (CRDT) → committed → visible on that branch           │
│                → merged to main when branch merges                          │
│                → resolved/archived after merge                              │
│                                                                              │
│  2. NODE-LEVEL COMMENTS (discussion / annotation)                           │
│     ────────────────────────────────────────────                            │
│     Anchored to an ontology node (a module, a concept, an API).             │
│     Example: "This module needs better error handling — see RFC-123"       │
│     These are workspace-scoped:                                               │
│       • The ontology node exists across branches                            │
│       • The discussion is about the concept, not a specific line            │
│       • Everyone should see it regardless of which branch they're on        │
│                                                                              │
│     Lifecycle: draft (CRDT) → committed → visible workspace-wide             │
│                → not branch-gated (stored in editor DB, not git)            │
│                → resolved manually or by convention                          │
│                                                                              │
│  The gotcha: if ALL comments lived only in per-branch draft sessions,      │
│  then Alice's architectural note on node:auth-module (written on branch    │
│  task/auth-fix) would be invisible to Bob on main. He'd have no idea       │
│  the discussion existed until Alice's branch merges — which could be       │
│  days or never.                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Resolution: comments are persisted to two stores based on their type.**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                  Comment Storage Model                                       │
│                                                                              │
│  All comments are indexed in the editor DB regardless of type.              │
│  The DB is the cross-branch comment index — the single place to query      │
│  "what discussion exists about this file/node across all branches?"         │
│                                                                              │
│  Comment type       Primary storage     Also indexed in     Default view    │
│  ────────────       ───────────────     ───────────────     ────────────    │
│  Line-level         Branch (.g5/)*      Editor DB (index)   This branch    │
│  Node-level         Editor DB           Editor DB           All branches   │
│                                                                              │
│  * Line-level comments travel with the branch in git (notes or .g5/ file)  │
│    so they merge when the branch merges. The editor DB index is a          │
│    denormalized copy for cross-branch queryability.                          │
│                                                                              │
│  On [Commit]:                                                                │
│    1. Line-level comments:                                                  │
│       a. Written to branch (git notes or .g5/comments)  ← primary          │
│       b. Indexed to editor DB with branch + path + line + commit range     │
│       → default: visible only on that branch                                │
│       → queryable cross-branch via the index                                │
│                                                                              │
│    2. Node-level comments:                                                  │
│       a. Persisted to editor DB with workspace scope      ← primary          │
│       → immediately visible to anyone on any branch                         │
│       → tagged with origin branch for context                               │
│                                                                              │
│  Cross-branch comment discovery:                                             │
│    • Bob is on main, opens src/auth.rs in Monaco                           │
│    • Gutter shows his branch's line comments (default)                     │
│    • Status bar / file tab shows: "💬 5 comments on 2 other branches"      │
│    • Toggling "Show cross-branch comments" (UI toggle):                     │
│      → Line-level comments from other branches appear as faded/dimmed      │
│        annotations with branch badges: "[task/auth-fix] Alice: ..."        │
│      → Comments whose line anchors can't map to the current branch         │
│        (line doesn't exist, file renamed) show in a sidebar panel instead  │
│    • The ontology viewer always shows a cross-branch comment count         │
│      per node — clicking expands all threads regardless of branch          │
│                                                                              │
│  Line mapping across branches:                                               │
│    • The index stores { path, line, commitSHA } at creation time            │
│    • To display on a different branch, the editor computes a best-effort   │
│      line mapping using git diff between the comment's commit and the      │
│      current branch HEAD (same algorithm as "git blame --reverse")          │
│    • If the file/line was deleted: comment shows in the sidebar panel      │
│      with a "⚠ line no longer exists on this branch" badge                 │
│    • This is best-effort — imperfect mapping is acceptable because          │
│      cross-branch comments are informational, not authoritative             │
│                                                                              │
│  Draft comments (not yet committed):                                         │
│    • Visible only to participants of that branch's draft session            │
│    • Not indexed cross-branch until committed                               │
│    • This is intentional — draft comments are work-in-progress             │
│                                                                              │
│  What about stale node-level comments after refactoring?                    │
│    • If the ontology node is deleted/renamed on a branch, the comment      │
│      remains in the DB (orphaned). The ontology viewer shows it with       │
│      a ⚠ "node removed on task/refactor" badge.                            │
│    • On merge: if the node still exists on main, comment stays.            │
│      If the node was deleted, comment is archived (not lost).              │
│                                                                              │
│  Cleanup:                                                                    │
│    • When a branch is deleted (after merge or abandonment), its line-level │
│      comments remain in the index but are marked as "branch-deleted".      │
│    • If the comments were merged to main, they also exist in main's git.   │
│    • Stale index entries are garbage-collected after a configurable TTL     │
│      (default: 90 days after branch deletion).                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

```typescript
// Comment data model — all comments indexed in editor DB.
// Canonical DB schema: API_AND_STORAGE.md §2.6 (comment_index + comment_replies).
// REST endpoints: API_AND_STORAGE.md §3.5.
interface Comment {
  id: string;
  workspaceId: string;
  author: Identity;
  content: string;                     // Markdown
  createdAt: string;
  resolvedAt?: string;
  
  // Scope determines primary storage and default visibility
  scope: 'line' | 'node';
  
  // Line-level: anchored to a diff hunk (primary: git, indexed: DB)
  lineRef?: {
    path: string;
    line: number;
    commitSHA: string;                 // Exact commit for line mapping
    commitRange?: { base: string; head: string };  // Diff context
  };
  
  // Node-level: anchored to an ontology node (primary + index: DB)
  nodeId?: string;
  
  // Provenance
  originBranch: string;                // Where the comment was created
  originCommit?: string;               // Which commit materialized it
  branchDeleted?: boolean;             // True after branch is deleted
}

// API: get all comments visible for a given branch context
GET /api/v1/workspaces/:workspaceId/comments?branch=main&crossBranch=false
// crossBranch=false (default):
//   - All node-level comments (workspace-global, from any branch)
//   - Line-level comments from this branch only
//
// crossBranch=true:
//   - All of the above, plus:
//   - Line-level comments from other branches, with best-effort line mapping
//   - Each cross-branch comment includes: { mappedLine, confidence, originBranch }

// API: get cross-branch comment summary for a file
GET /api/v1/workspaces/:workspaceId/comments/cross-branch?path=src/auth.rs&branch=main
// Returns: { totalComments: 5, branches: ["task/auth-fix", "task/perf"] }
// Lightweight — used for the "💬 5 on 2 branches" badge, no full content
```

#### Workflows & Ontology: Not Worker-Dependent

Workflow status (task queue, execution history, agent session states) and ontology annotations/comments are **persisted by the editor** — not stored on the worker filesystem. They survive worker crashes, swaps, and disconnections.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                Where data lives                                              │
│                                                                              │
│  Data                   Persisted by           Worker required?             │
│  ────                   ────────────           ────────────────             │
│  File content           Editor or worker FS    No (editor has checkout)     │
│  Ontology structure     Files in git           No (editor has checkout)     │
│  Ontology annotations   Editor DB / git notes  No                          │
│  Comments               Editor DB              No                          │
│  Workflow definitions   Files in git           No (editor has checkout)     │
│  Task queue status      Control plane DB       No                          │
│  Execution history      Control plane DB       No                          │
│  Agent session history  Editor DB              No                          │
│  Build output           Worker (ephemeral)     Yes (worker ran the build)  │
│  Test results           Worker (ephemeral)     Yes (worker ran the tests)  │
│  Terminal session       Worker (PTY)           Yes                         │
│                                                                              │
│  Only the bottom four require a healthy worker. Everything else is          │
│  available as long as the editor is reachable — or even just from the     │
│  browser's local cache.                                                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Edit & Save Data Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Draft model (same flow regardless of worker state):                        │
│                                                                              │
│    Monaco (browser)         g5d (editor)          Filesystem                │
│    ┌─────────────┐         ┌──────────────┐       ┌───────────────┐        │
│    │ User types   │  CRDT   │ Draft Session│       │ Editor or     │        │
│    │ → Yjs doc    │ ◄─────▶ │ (server-side │       │ Worker FS     │        │
│    │              │  sync   │  CRDT, DB-   │       │               │        │
│    │ Agent types  │ ───────▶│  persisted)  │       │               │        │
│    │ → Yjs doc    │         │              │       │               │        │
│    │              │         │ ─── diff ──▶ │       │               │        │
│    │ Late joiner  │ ◄────── │ full state   │       │               │        │
│    │ (new tab/    │  on     │ on connect   │       │               │        │
│    │  new user)   │  join   │              │       │               │        │
│    └─────────────┘         └──────┬───────┘       └───────┬───────┘        │
│                                    │                       │                │
│                         [Commit] or [Fork]                 │                │
│                                    │                       │                │
│                          [Commit]: write files ───────────▶│                │
│                                    git add + commit        │                │
│                                    persist comments        │                │
│                                    advance baseCommit      │                │
│                                                                              │
│                          [Fork]:   git checkout -b ───────▶│                │
│                                    transfer draft to branch│                │
│                                    reset original branch   │                │
│                                                                              │
│  Nothing touches disk until [Commit].                                       │
│  All edits live in the server-side CRDT draft session.                     │
│  CRDT is DB-persisted — survives tab close, process restart, reconnect.    │
│  ⌘S is "save to CRDT" (persists the Yjs state) not "save to disk."        │
│  Opening the workspace URL loads the current draft state for that branch.    │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### View Source: Worker-First + Draft Overlay

All UI views follow the same **base commit + draft diff** model. The base content comes from the filesystem (worker when connected, editor checkout as fallback). The CRDT draft provides the overlay — pending edits, comments, annotations — visible to everyone.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Where Views Come From                                     │
│                                                                              │
│                           No Worker            Worker Connected              │
│  View                     (editor only)        (local or cloud)              │
│  ────                     ─────────────        ─────────────────            │
│                                                                              │
│  File tree                Editor checkout      Worker filesystem            │
│  File content (Monaco)    Editor checkout      Worker filesystem            │
│   + draft overlay         + CRDT edits         + CRDT edits (same draft)   │
│  Diff annotations         Draft diff (green/   Draft diff (same — shared   │
│                            yellow/red gutter)   server-side CRDT)           │
│  Ontology viewer          Editor checkout      Worker filesystem            │
│   + draft annotations     + CRDT comments      + CRDT comments             │
│  Workspace view            Editor checkout      Worker filesystem            │
│  Search (text, regex)     Editor index         Worker index *               │
│  Git log / blame          Editor checkout      Worker checkout              │
│  Git diff (committed)     Editor git log       Worker git log               │
│  File editing             ✅ (→ CRDT draft)    ✅ (→ CRDT draft)            │
│  Git commit (= [Commit])  ✅ (editor checkout) ✅ (worker checkout)         │
│  Fork (= [Fork])          ✅ (editor checkout) ✅ (worker checkout)         │
│  Terminal                 ❌ unavailable       Worker PTY                   │
│  Build / Test             ❌ unavailable       Worker                       │
│                                                                              │
│  * Search may use the editor's index for speed if the worker's index is    │
│    still warming up. Once the worker's index is ready, it takes over.      │
│                                                                              │
│  Key insight: the draft layer is the SAME regardless of whether you have   │
│  a worker. It's the shared server-side CRDT. The worker only changes       │
│  where the *base content* comes from (its FS instead of the editor's).     │
│  This means switching between worker/no-worker preserves draft state.      │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Why worker-first for the base layer?** The ontology (API_AND_STORAGE.md §2.3) is file-backed — `OntologyNode.filePath` points to source files, `OntologyNode.status` tracks git state. If the user is working on a task branch with new ontology nodes, those nodes only exist on the worker's filesystem. Showing the editor's checkout as a "base layer" would hide them. Same for workspace structure — new files, renamed modules, updated configs only exist on the worker.

**Why the same draft for both?** The CRDT draft session is keyed to `(workspace, branch)`, not to a specific worker. When a worker connects, it materializes the base commit on its filesystem. Edits go into the shared draft. When the worker disconnects, the draft persists server-side — the editor can render it atop its own checkout. This is the universal view model.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    View Switching on Worker Change                            │
│                                                                              │
│  User connects to worker A (branch: task/redesign-auth):                    │
│    • Base layer: worker A's filesystem                                      │
│    • Draft layer: server-side CRDT for (workspace, task/redesign-auth)        │
│    • File tree = worker A FS; Monaco = base + draft overlay                │
│    • Git status shows worker A's branch + draft diff                       │
│                                                                              │
│  User switches to worker B (branch: task/fix-perf):                         │
│    • Base layer flips to worker B's filesystem                              │
│    • Draft layer switches to CRDT for (workspace, task/fix-perf)              │
│    • Draft for task/redesign-auth persists server-side (not lost)           │
│    • Previously open tabs reload from worker B + that branch's draft       │
│      (or close if the file doesn't exist on worker B's branch)             │
│                                                                              │
│  User disconnects from all workers:                                         │
│    • Base layer falls back to editor checkout                               │
│    • Draft layer: still the same server-side CRDT for the current branch   │
│    • Edits continue accumulating in the draft — same diff annotations      │
│    • Open tabs remain editable (edits → CRDT)                               │
│                                                                              │
│  This is analogous to switching branches in a local IDE — the entire        │
│  workspace view updates. Except here, each "branch" is a separate worker,    │
│  and the draft state for every branch persists independently on the server. │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Data Channel: Filesystem Streams

The browser subscribes to filesystem-level streams from the connected worker. These power the file tree, ontology viewer, and workspace view in real time.

| Stream | What it powers | Payload |
|--------|---------------|---------|
| `file-watch` | File tree, ontology viewer auto-refresh | File change events (create/modify/delete + path) |
| `file-content` | Monaco file open, large previews | Raw bytes / text |
| `terminal` | Terminal panel | PTY frames |
| `build-log` | Build output panel | Structured log lines |
| `test-output` | Test results panel | TAP/JUnit streaming |
| `git-status` | Git indicators, diff view, pull badge | Branch + dirty files + upstream status |

The `file-watch` stream is key: when the agent (or the user via terminal) modifies a file on the worker, the worker emits a change event. The browser picks it up and updates the file tree, ontology viewer, and any open Monaco tabs instantly.

```typescript
// file-watch event payload
interface FileWatchEvent {
  type: 'create' | 'modify' | 'delete' | 'rename';
  path: string;                // Relative to workspace root
  oldPath?: string;            // For renames
  isDirectory: boolean;
  contentHash?: string;        // For dedup / staleness check
}

// Browser-side: keeping views in sync with the worker
const watchWs = await openDataChannel(editorUrl, workerId, 'file-watch', token);
watchWs.onmessage = (e) => {
  const event: FileWatchEvent = JSON.parse(e.data);
  
  fileTreeStore.applyEvent(event);       // Update file tree sidebar
  ontologyStore.applyEvent(event);       // Update ontology viewer if ontology file changed
  
  // If the modified file is open in a tab, refetch content
  if (openTabs.has(event.path)) {
    refetchFileContent(workerId, event.path);
  }
};
```

#### Switching Workers: Draft Persistence Across Switches

Because draft state lives server-side (not on the worker's filesystem), switching workers does **not** lose uncommitted edits. The CRDT draft session is keyed to `(workspace, branch)` — it persists regardless of which worker (or no worker) is backing the base layer.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  User clicks "Disconnect" or selects a different worker:                   │
│                                                                              │
│  Same branch (worker swap / reconnect):                                     │
│    → Draft state is unaffected — same CRDT session continues               │
│    → Base layer switches to new worker's FS (or editor fallback)           │
│    → No prompt needed. Seamless.                                            │
│                                                                              │
│  Different branch (switching workers = switching branches):                  │
│    → Current branch's draft persists server-side                            │
│    → UI loads the target branch's draft session                             │
│    → If target branch has pending drafts: they appear immediately          │
│    → If target branch is clean: fresh start                                │
│    → No work is lost. Switch freely.                                       │
│                                                                              │
│  Worker has filesystem-level uncommitted changes (outside CRDT):            │
│    → This happens if the agent edited files directly via terminal           │
│    → These are NOT in the draft — they're raw FS changes on the worker    │
│    → Modal warning:                                                         │
│    ┌─────────────────────────────────────────────────────────────────────┐  │
│    │  ⚠ Worker has filesystem changes outside the draft                 │  │
│    │                                                                     │  │
│    │  Worker "task-456" has 3 files modified outside the draft system:  │  │
│    │    • src/main.rs (agent terminal edit)                              │  │
│    │    • build.rs (build artifact)                                      │  │
│    │    • Cargo.lock (dependency change)                                 │  │
│    │                                                                     │  │
│    │  These changes are on the worker filesystem, not in the draft.     │  │
│    │  Switching workers will discard them.                               │  │
│    │                                                                     │  │
│    │  [Import to Draft]  [Cancel]         [Discard & Switch]            │  │
│    └─────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  [Import to Draft] reads the worker's FS diff and applies it to the CRDT  │
│  draft — after which the user can commit or fork normally.                 │
│                                                                              │
│  The agent respects this too: if it edits files via terminal (outside the  │
│  CRDT), it imports those changes to the draft before requesting a swap.    │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Git Sync & Worker Status Bar

The connected worker keeps its checkout up to date. The UI shows sync status and indexing activity.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Status Bar (bottom of UI)                                │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ ● worker-A (local)  │  main @ abc1234  │  ↓2 │  ⟳ Indexing...    │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│     ▲                      ▲                 ▲       ▲                      │
│     │                      │                 │       │                      │
│     Connected worker       Branch + short    Pull    Worker activity        │
│     name + type            commit hash       badge   indicator              │
│     (click: switch)        (click: git log)                                 │
│                                                                              │
│  Pull badge (↓N):                                                           │
│    • Worker runs `git fetch` every 30s in the background                   │
│    • If origin has new commits: badge shows ↓N (N = commits behind)        │
│    • Click badge → worker runs `git pull --rebase`                         │
│    • If worker has uncommitted changes:                                     │
│        pull is blocked, badge tooltip: "Commit changes before pulling"     │
│    • After pull: search index refreshes, editor updates                    │
│    • Pull can also be triggered by agent: tool `git_pull`                  │
│                                                                              │
│  Worker activity indicator:                                                 │
│    ⟳ Indexing...    → Search index is building                               │
│    ⟳ Building...    → Build task is running                                │
│    ⟳ Testing...     → Test suite is running                                │
│    ⟳ Pulling...     → git pull in progress                                 │
│    ✓ Ready          → Worker idle, all indexes current                     │
│    ● 3 errors       → Build/test diagnostics summary                         │
│                                                                              │
│  No worker connected:                                                       │
│    ┌─────────────────────────────────────────────────────────────────────┐  │
│    │ ○ No worker  │  main @ abc1234  │  3 drafts │[Fork][Commit]       │  │
│    │              │                  │           │[Start Worker]        │  │
│    └─────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│    Note: editing is fully functional without a worker — edits go to the    │
│    CRDT draft. [Commit] writes to the editor's checkout. [Fork] creates   │
│    a new branch. Only build/test/terminal require a worker.                │
└─────────────────────────────────────────────────────────────────────────────┘
```

```typescript
// Worker status API (polled by UI, or pushed via WebSocket)
GET /api/internal/workers/:id/status
{
  "id": "w-123",
  "state": "ready",                    // ready | indexing | building | testing | pulling
  "branch": "task/task-456",
  "commit": "abc1234",
  "dirty": true,                       // has uncommitted changes
  "dirtyFiles": ["src/main.rs"],
  "upstream": {
    "behind": 2,                       // commits behind origin
    "ahead": 5,                        // commits ahead of origin  
    "lastFetchAt": "2026-02-05T10:30:00Z"
  },
}

// Trigger pull (internal: worker → editor)
POST /api/internal/workers/:id/pull
// Returns 409 if worker has uncommitted changes
// Returns 200 with { "pulled": 2, "commit": "def5678" } on success
```

### 11.8 Instance Lifecycle

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Instance Lifecycle                                    │
│                                                                              │
│  Editor Instance:                                                           │
│    Desktop: started by user (`g5d --role editor`)                           │
│             runs until killed. Watches git for changes.                     │
│    Cloud:   started on browser connect. Scales to zero after idle timeout.  │
│                                                                              │
│  Worker Instance:                                                           │
│    Desktop: spawned by editor on task creation. Uses git worktree.          │
│             Killed on task complete/abort. Respects localWorkerSlots limit. │
│    Cloud:   provisioned by control plane. Kata VM. Destroyed on complete.   │
│                                                                              │
│  Worker lifecycle (both local and cloud):                                   │
│    1. Provision: create worktree/clone, hydrate caches                      │
│    2. Ready: accept tool calls from agent loop on editor                    │
│    3. Active: execute writes, builds, tests, stream results                 │
│    4. Complete: merge/PR, tear down worktree, release slot                  │
│                                                                              │
│  Swap: A task's worker can be replaced (§10.12) without disrupting          │
│        the chat session. Move local→cloud, cloud→local, or rebase.         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.9 Instance API

```
# Editor instance                                (public: /api/v1/)
POST   /api/v1/instances                         # Create / wake editor for workspace
GET    /api/v1/instances/:id                     # Instance status
DELETE /api/v1/instances/:id                     # Teardown

# Git pinning (editor)                           (public: /api/v1/)
GET    /api/v1/instances/:id/ref                 # Current pinned ref + commit
POST   /api/v1/instances/:id/pin                 # Pin to new ref
POST   /api/v1/instances/:id/pull                # Pull latest

# Chat sessions (on editor)                      (public: /api/v1/)
WS     /api/v1/instances/:id/chat/:sessionId     # Chat streaming
POST   /api/v1/instances/:id/chat                # Create new session

# Search (editor)                                 (public: /api/v1/)
POST   /api/v1/instances/:id/search              # Code search
WS     /api/v1/instances/:id/search/live         # Streaming results

# Files (editor fallback — used only when no worker is connected)
GET    /api/v1/instances/:id/files/*path         # Read file (from pinned checkout)
GET    /api/v1/instances/:id/tree                # File tree (from pinned checkout)

# Worker management (on editor, manages its pool)
GET    /api/v1/instances/:id/workers             # List active workers
POST   /api/v1/instances/:id/workers             # Spawn worker for task
DELETE /api/v1/instances/:id/workers/:workerId   # Kill worker
GET    /api/v1/instances/:id/workers/:workerId   # Worker status + task info
```

### 11.10 Instance Cache Architecture

The `g5d` binary uses two cache tiers. The same code runs everywhere — only the backing store changes.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Cache Tiers                                          │
│                                                                              │
│  ┌───────────────────────────────────────┐                                  │
│  │  Instance Cache (L1)                   │                                  │
│  │                                        │                                  │
│  │  Fast local storage. Shared across     │                                  │
│  │  all g5d processes on the same host.   │                                  │
│  │                                        │                                  │
│  │  Desktop: ~/.g5/cache/ on local SSD    │                                  │
│  │  Cloud:   /cache/ on attached SSD      │                                  │
│  │           (persistent across pod       │                                  │
│  │            restarts on same node)      │                                  │
│  └───────────────────────┬───────────────┘                                  │
│                          │ miss                                              │
│                          ▼                                                   │
│  ┌───────────────────────────────────────┐                                  │
│  │  Remote Cache (L2)                     │                                  │
│  │                                        │                                  │
│  │  Workspace-scoped shared cache.          │                                  │
│  │  All instances (local + cloud) for     │                                  │
│  │  the same workspace share this.          │                                  │
│  │                                        │                                  │
│  │  Hosted: GCS bucket per §2             │                                  │
│  │  Self-hosted: S3-compatible / MinIO    │                                  │
│  │  Offline: disabled (L1 only)           │                                  │
│  └───────────────────────────────────────┘                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

**The key insight:** On a developer's laptop, the instance cache is *already warm* from previous runs. The local SSD acts like a permanent L1. This makes local development extremely fast — often faster than cloud, because there's no L2 roundtrip.

### 11.11 Instance Cache Configuration

```typescript
interface CacheConfig {
  // Instance cache (L1)
  instanceCachePath: string;        // Desktop: ~/.g5/cache, Cloud: /cache
  instanceCacheMaxGb: number;       // Desktop: 50, Cloud: configurable per node
  
  // Remote cache (L2)
  remoteCache: RemoteCacheConfig | null;  // null = offline / L1 only
}

type RemoteCacheConfig = {
  type: 'gcs';
  bucket: string;
  prefix: string;                   // tenants/{t}/workspaces/{w}/build_cache
  serviceAccountKey?: string;       // Cloud: via Workload Identity. Desktop: key file.
} | {
  type: 's3';
  bucket: string;
  prefix: string;
  endpoint?: string;                // For MinIO / self-hosted
  accessKeyId: string;
  secretAccessKey: string;
} | {
  type: 'http';
  baseUrl: string;                  // For Gradle/Bazel remote cache services
  authToken?: string;
};
```

**Desktop defaults (`.g5/config.yaml`):**
```yaml
cache:
  instance:
    path: ~/.g5/cache
    max_gb: 50                      # Grows up to 50GB, LRU eviction

  remote:
    type: gcs
    bucket: g5-cache-prod
    prefix: tenants/t1/workspaces/myproject/build_cache
    # Auth: uses `gcloud auth application-default` credentials
    # Set to null for fully offline operation
```

**Cloud defaults (injected via pod env):**
```yaml
cache:
  instance:
    path: /cache
    max_gb: 100                     # Larger SSD in cloud nodes

  remote:
    type: gcs
    bucket: ${G5_GCS_BUCKET}
    prefix: ${G5_GCS_PREFIX}/build_cache
    # Auth: Workload Identity (automatic)
```

### 11.12 How g5d Uses the Cache

Every `g5d` process — whether editor or worker, local or cloud — resolves caches the same way:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     g5d Cache Resolution                                     │
│                                                                              │
│  Tool: sccache, cargo, go, pip, pnpm, gradle, maven                        │
│  Each tool is configured to use:                                            │
│    L1 = ${instanceCachePath}/<tool>/                                        │
│    L2 = remote cache (if configured)                                        │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  Example: Rust build on a local worker                               │  │
│  │                                                                       │  │
│  │  1. cargo build                                                      │  │
│  │  2. rustc invoked → sccache wrapper intercepts                       │  │
│  │  3. sccache checks L1: ~/.g5/cache/sccache/{hash}                   │  │
│  │     HIT → use cached .o, done (< 1ms)                               │  │
│  │     MISS ↓                                                           │  │
│  │  4. sccache checks L2: gs://bucket/prefix/sccache/{hash}            │  │
│  │     HIT → download to L1, use cached .o (~20-50ms)                  │  │
│  │     MISS ↓                                                           │  │
│  │  5. Compile from source                                              │  │
│  │  6. Upload to L1 + L2                                                │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  Example: npm install on a cloud worker                              │  │
│  │                                                                       │  │
│  │  1. pnpm install                                                     │  │
│  │  2. Check L1: /cache/pnpm-store/                                     │  │
│  │     Packages present → hardlink into node_modules, done              │  │
│  │     Missing ↓                                                        │  │
│  │  3. Check L2: gs://bucket/prefix/seed/node/{lockfile-hash}.tar.zst  │  │
│  │     HIT → extract to L1, then hardlink                              │  │
│  │     MISS ↓                                                           │  │
│  │  4. Download from npm registry                                       │  │
│  │  5. Store in L1, snapshot to L2                                      │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.13 Local Instance Cache — Shared Across Processes

On the desktop, **all `g5d` processes share the same instance cache** because they're on the same filesystem. This is a major advantage:

```
Developer's Laptop
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  ~/.g5/cache/  (shared SSD, LRU-managed, 50GB)                             │
│  ├── sccache/          ← shared by all Rust workers                        │
│  ├── cargo/            ← shared crate index + downloads                    │
│  ├── go-build/         ← shared Go build cache                             │
│  ├── go-mod/           ← shared Go modules                                 │
│  ├── pnpm-store/       ← shared Node packages (content-addressed)          │
│  ├── pip/              ← shared Python wheels                              │
│  ├── m2/               ← shared Maven artifacts                            │
│  ├── gradle/           ← shared Gradle deps + build cache                  │
│                                                                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                                 │
│  │ g5d      │  │ g5d      │  │ g5d      │  All read/write ~/.g5/cache/    │
│  │ editor   │  │ worker A │  │ worker B │  File locks prevent corruption  │
│  │ :7500    │  │ :7501    │  │ :7502    │                                 │
│  └──────────┘  └──────────┘  └──────────┘                                 │
│                                                                              │
│  Worker A compiles a crate → sccache stores in L1                          │
│  Worker B needs same crate → L1 HIT immediately (no L2 roundtrip)         │
│                                                                              │
│  This is why local development is fast: the cache is always warm.           │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.14 Cloud Instance Cache — Attached SSD

In cloud deployments, the instance cache lives on a **persistent SSD attached to the node**, not the ephemeral `emptyDir` from §1.3. This means cache survives pod restarts.

```yaml
# Cloud: Node-level cache on persistent SSD
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: g5-node-cache-${NODE_ID}
spec:
  accessModes: ["ReadWriteOnce"]
  storageClassName: pd-ssd
  resources:
    requests:
      storage: 200Gi          # Shared across all g5d pods on this node
---
# Pod mounts the shared node cache
spec:
  containers:
    - name: g5d
      volumeMounts:
        - name: node-cache
          mountPath: /cache
  volumes:
    - name: node-cache
      persistentVolumeClaim:
        claimName: g5-node-cache-${NODE_ID}
```

**Cloud cache lifecycle:**
```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Cloud Node Cache                                         │
│                                                                              │
│  Node starts:                                                               │
│    PVC attached → /cache/ available                                         │
│    May already contain warm caches from previous pods                       │
│                                                                              │
│  Pod A starts (worker for task-1):                                          │
│    Mounts /cache/ → sccache, cargo, etc. use it as L1                      │
│    Builds workspace → L1 populates                                            │
│                                                                              │
│  Pod B starts on same node (worker for task-2):                             │
│    Mounts same /cache/ → gets warm L1 from Pod A's builds                  │
│    L1 HIT rate: high (same workspace, similar dependencies)                  │
│                                                                              │
│  Pod A finishes, gets destroyed:                                            │
│    /cache/ persists on node's SSD                                           │
│                                                                              │
│  Pod C starts on same node later:                                           │
│    Still warm L1. Fast startup.                                             │
│                                                                              │
│  Node drains / scales down:                                                 │
│    L1 lost (PVC released). L2 (GCS) preserves everything.                  │
│    Next node starts cold, pulls from L2 on first build.                    │
│                                                                              │
│  LRU eviction keeps /cache/ within 200GB.                                  │
│  Per-workspace cache isolation via path prefix:                               │
│    /cache/{workspace-id}/sccache/                                             │
│    /cache/{workspace-id}/cargo/                                               │
│    etc.                                                                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.15 Cache Strategy Summary

| Scenario | L1 (Instance SSD) | L2 (Remote) | Typical hit rate |
|----------|-------------------|-------------|-----------------|
| **Desktop, warm** | `~/.g5/cache/` (persistent, shared) | GCS (optional) | L1: 90%+, L2: rarely needed |
| **Desktop, cold** | Empty | GCS pulls on miss | L1: 0%, L2: 60-80% |
| **Desktop, offline** | `~/.g5/cache/` | Disabled | L1: 90%+ (if warm) |
| **Cloud, same node** | `/cache/` PVC (shared across pods) | GCS | L1: 70-90%, L2: 5-20% |
| **Cloud, new node** | Empty PVC | GCS | L1: 0%, L2: 60-80% |
| **Cloud, cross-workspace** | Isolated by path prefix | Isolated by GCS prefix | No cross-workspace sharing |

**Team benefit:** When a team allocates fixed capacity (e.g., 4 workers on a dedicated node), the node's SSD cache warms up over hours. By day 2, most builds hit L1 — the remote cache is almost never consulted. This is the same dynamic as a developer's laptop, but scaled to a team.

### 11.16 Cache Smart Client

The L1→L2 waterfall and pre-warming are orchestrated by a **cache client** inside `g5d`, not by the individual build tools. The build tools (sccache, cargo, pnpm, etc.) only see L1. The `g5d` cache client handles L2 transparently.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         g5d Cache Client                                     │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │  Waterfall Orchestrator                                             │    │
│  │                                                                     │    │
│  │  Build tools write to L1 (local paths). They don't know about L2.  │    │
│  │  The cache client handles promotion and fetching:                   │    │
│  │                                                                     │    │
│  │  On worker startup:                                                 │    │
│  │    1. Read lockfiles → compute cache keys                          │    │
│  │    2. Check L1 for each key                                        │    │
│  │    3. For L1 misses → batch-fetch from L2 in parallel              │    │
│  │    4. Build tools see warm L1 on first run                         │    │
│  │                                                                     │    │
│  │  On build completion:                                               │    │
│  │    1. Scan L1 for new entries (not yet in L2)                      │    │
│  │    2. Background upload to L2 (non-blocking)                       │    │
│  │    3. Compress with zstd before upload                             │    │
│  └────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │  Pre-Warming Engine                                                 │    │
│  │                                                                     │    │
│  │  Triggers:                                                          │    │
│  │    • Worker provision → pre-warm from lockfiles before agent starts │    │
│  │    • Lockfile change detected → fetch new deps in background        │    │
│  │    • Scheduled: nightly rebuild of seed caches for active workspaces  │    │
│  │    • On merge to main → rebuild workspace-latest image with warm L1  │    │
│  │                                                                     │    │
│  │  Pre-warm strategy per ecosystem:                                   │    │
│  │    Rust:   fetch crates for Cargo.lock → populate /cache/cargo     │    │
│  │    Node:   pnpm fetch → populate /cache/pnpm-store                 │    │
│  │    Python: pip download -r requirements.txt → /cache/pip           │    │
│  │    Java:   mvn dependency:go-offline → /cache/m2                   │    │
│  │    Go:     go mod download → /cache/go-mod                         │    │
│  └────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │  LRU Manager                                                       │    │
│  │                                                                     │    │
│  │  • Tracks access time per cache entry                              │    │
│  │  • Evicts least-recently-used when L1 exceeds max_gb               │    │
│  │  • Eviction is per-workspace on shared cloud nodes                   │    │
│  │  • Never evicts entries <1 hour old (prevent thrashing)            │    │
│  │  • Reports eviction events to metrics                              │    │
│  └────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.17 Client Surfaces

G5 has four client surfaces. All speak the same g5-api REST/WS API and the same g5d instance API (§11.9). The surfaces differ in what they render locally and what they delegate to g5d.

#### Design Constraints

The surface architecture is shaped by seven practical constraints:

| # | Constraint | Implication |
|---|-----------|-------------|
| C1 | **Language servers must be colocated with the IDE.** LSP servers read the filesystem, load compilation databases, resolve toolchain paths, and depend on host-specific state (installed SDKs, PATH, platform headers). They cannot run remotely without reimplementing the entire build environment. | We do **not** ship LSP in g5d. Users get IntelliSense via **Open in IDE** with their local toolchain. |
| C2 | **LSP setup is non-standard.** Language server configuration depends on toolchain versions, prior compilation artifacts (e.g., `target/` for rust-analyzer, `tsconfig.json` paths for tsserver), and machine-specific characteristics (installed SDKs, compiler paths, environment variables). There is no universal "install LSP and it works" — every project has bespoke setup. | We defer to IDE-native LSPs via **Open in IDE** rather than shipping a generic LSP configuration. |
| C3 | **Containers add onboarding friction.** Running workers in containers requires Docker, Podman, or a VM hypervisor. Many developers — especially on macOS and Windows — do not have these installed, and installing them is a multi-step process with licensing considerations (Docker Desktop). Requiring containers for local use would violate the "it just works" principle. | Local workers use git worktrees (zero dependencies beyond git). Containers are for cloud workers only. The CLI and extension must work without Docker installed. |
| C4 | **Isolates have mixed cross-platform support.** The strongest isolation stories are WASI (WebAssembly System Interface) and filesystem-level CoW (XFS reflinks, APFS clones). WASI is too immature to support standard container-like images with language server deployment — it lacks full POSIX compatibility, filesystem semantics vary across runtimes, and toolchain support is incomplete. CoW is filesystem-dependent (works on APFS/macOS, XFS/Linux; absent on ext4, NTFS). | We use git worktrees for local isolation (universally supported) and Kata VMs for cloud isolation (strong, portable). No WASI dependency. CoW is opportunistic — nice when available, not required. |
| C5 | **IDEs have significant toolchain auto-detection.** VS Code, JetBrains, and other IDEs invest heavily in detecting installed SDKs, resolving toolchains, remembering per-project settings, and guiding users through setup wizards. This institutional knowledge — accumulated over thousands of user interactions — is impractical to replicate. | We leverage VS Code's existing toolchain detection rather than reimplementing it. The extension is a sidecar, not a replacement. |
| C6 | **Agents don't need LSP.** Agents navigate code via `read_file`, `extract_interface` (tree-sitter), and `grep`/`rg`. Diagnostics are better captured by running the compiler/linter directly via `run_in_terminal`. | We do not invest in agent-facing LSP tooling. |
| C7 | **"It just works" on client install and web browser.** The CLI and VS Code extension must work immediately after installation with zero configuration beyond pointing at a workspace. The browser must provide a useful experience without installing anything. Deferring IntelliSense to the IDE achieves this. | The CLI has no LSP. The extension relies on VS Code's native LSP. The web app uses **Open in IDE** for IntelliSense. |

#### Experience Tiers

These constraints produce three distinct editing experiences, each with different tradeoffs:

| Tier | Surface | Experience | Editing Model | IntelliSense | Offline | Tradeoff |
|------|---------|-----------|---------------|-----|---------|----------|
| **1. Google Docs (web)** | Browser | Real-time collaborative editing. Multiple cursors, presence indicators, instant propagation. Richest UI: ontology tree, task timeline, comments, dual-mode editor. | Yjs CRDT (server-authoritative). Edits flow through g5-api. | **Open in IDE** (local LSP). | ❌ Requires network | Best collaboration; weakest for heavy coding (no local toolchain, no refactoring, no debugging). |
| **2. Google Docs (plugin)** | VS Code + Yjs extension (Phase 3) | Real-time collaborative editing inside VS Code. Same CRDT merge semantics as web. Agent edits appear in real time. | Yjs CRDT peer in extension. Extension joins `DraftSession` directly. | VS Code native. Full local toolchain. | ✅ Offline-capable (see below) | Best of both worlds: full IDE + real-time collaboration. Highest implementation cost. Phase 3. |
| **3. Synced workspace** | Any editor + g5d checkout | g5d materializes CRDT drafts to disk. Editor reads/writes files normally via FS events. No CRDT awareness in the editor. | File-based. g5d owns the checkout; editor sees FS changes. | Editor's own. | ⚠️ Degraded (git-level only) | Works with any editor (Vim, Emacs, JetBrains, etc.). No real-time co-editing — changes propagate on save, not on keystroke. |

**Tier 1 is the web default.** Every browser user gets this automatically.

**Tier 3 is the VS Code default (Phase 2).** The extension opens g5d's checkout as a workspace folder. Zero CRDT integration needed — VS Code just edits files on disk. This is also the story for any non-VS-Code editor: open the directory, edit files, g5d syncs.

**Tier 2 is the VS Code upgrade (Phase 3).** The extension embeds Yjs and joins as a CRDT peer. This upgrades the experience from "synced workspace" to "Google Docs in VS Code" — but requires significant extension engineering.

These tiers produce the architecture below: three user-facing surfaces with different editing strategies, plus headless workers.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Client Surfaces                                          │
│                                                                              │
│  ┌──────────┐  ┌──────────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   CLI    │  │  VS Code Plugin  │  │    Web App   │  │   Backend    │  │
│  │          │  │                  │  │   (Browser)  │  │   Workers    │  │
│  └────┬─────┘  └────────┬─────────┘  └──────┬───────┘  └──────┬───────┘  │
│       │                 │                    │                  │          │
│       │  g5 CLI         │  Extension API     │  React SPA       │  g5d     │
│       │  (thin client)  │  (sidecar panels)  │  (canonical UI)  │  binary  │
│       ▼                 ▼                    ▼                  ▼          │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                    g5d (editor) — local daemon                       │  │
│  │                                                                       │  │
│  │  • Git checkout (materializes CRDT drafts to disk)                   │  │
│  │  • Agent loop (chat)                                                  │  │
│  │  • File watching + search index                                       │  │
│  │  • Worker pool management                                             │  │
│  │  • Terminal sessions (PTY)                                            │  │
│  └───────────────────────────────┬─────────────────────────────────────┘  │
│                                  │                                         │
│                                  ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                    g5-api — stateful service layer                    │  │
│  │                                                                       │  │
│  │  • Yjs CRDT authority (persists to Postgres)                          │  │
│  │  • Job orchestrator                                                   │  │
│  │  • Session host                                                       │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Surface Comparison

| Capability | CLI | VS Code Plugin | Web App | Workers |
|------------|-----|---------------|---------|---------|
| **Start / attach to g5d** | ✅ `g5 start`, `g5 attach` | ✅ Auto-launch on workspace open | ✅ Connects to running g5d | ✅ Is g5d |
| **Agent chat** | ✅ Inline REPL | ✅ Sidebar panel | ✅ Chat panel | ✅ Agent loop (headless) |
| **Task selection & control** | ✅ `g5 tasks`, `g5 run` | ✅ Task tree view | ✅ Task timeline | N/A (receives tasks) |
| **File editing** | ❌ (use IDE/editor) | ✅ VS Code native editor | ✅ Monaco + Tiptap (CRDT) | ✅ Agent tools |
| **IntelliSense** | ❌ | ✅ VS Code native LSP | ✅ **Open in IDE** | ❌ |
| **Diff / review** | ✅ `g5 diff` | ✅ VS Code diff view | ✅ Draft diff panel | N/A |
| **CRDT collaborative editing** | ❌ | Phase 3 (Yjs peer) | ✅ Primary surface | ✅ (worker is CRDT peer) |
| **Terminal** | ❌ (user's own shell) | ✅ VS Code terminal | ✅ Browser terminal | ✅ PTY |
| **Build / test** | ❌ (via g5d worker) | ✅ VS Code tasks / g5d | ✅ Via worker | ✅ Native |
| **Ontology viewer** | ❌ (text list only) | ✅ Tree view panel | ✅ Full graph + tree | N/A |
| **Comments / annotations** | ❌ | ✅ Inline via VS Code API | ✅ Full comment system | N/A |
| **Ideal for** | SSH, headless, CI, scripting | Day-to-day development | Review, collaboration, onboarding | Agent execution |

#### 1. CLI (`g5`)

The `g5` command-line tool is a thin client that starts, attaches to, and controls a local `g5d` instance. It is the headless / SSH / quick-run interface.

```bash
# Start g5d for a workspace (or attach to an existing one)
g5 start .                           # starts g5d --role editor for current dir
g5 start --workers 3                 # also spawn 3 local workers
g5 attach                            # attach to already-running g5d

# Chat mode — conversational agent control
g5 chat                              # open REPL: select task, ask questions, give instructions
g5 chat "uplift src/auth/"           # one-shot: start uplift and stream output

# Task control
g5 tasks                             # list active tasks + status
g5 tasks --watch                     # live-updating task list
g5 run uplift src/auth/              # start uplift job on a node
g5 run downlift auth/validator       # start downlift on a spec

# Workspace operations
g5 diff                              # show CRDT draft diff (uncommitted edits)
g5 commit -m "Uplift auth module"    # commit CRDT draft → git
g5 status                            # workspace + worker pool status
g5 workers                           # list workers + their tasks
```

The CLI communicates with g5d via the same REST/WS API used by all clients (§11.9). Chat mode streams `SessionMessage`s to the terminal with formatted tool calls and agent reasoning. The CLI does **not** provide file editing — that is the IDE's job.

#### 1.1 g5d Workspace CLI (daemon)

The `g5d` binary also provides **workspace lifecycle** commands. These are
distinct from the `g5` thin client: they create/clone/refresh workspaces and
own the on-disk checkout layout. See `G5D_CLI.md` for full details.

Key commands:
- `g5d init` — create a new workspace with `.g5/workspace.yaml`.
- `g5d clone` — materialize a workspace from g5-api (no pre-config needed).
- `g5d fork` — create a new workspace in g5-api from the current one.
- `g5d refresh` — pull new repos/updates from g5-api.
- `g5d add` — add a repo to the workspace and update config.
- `g5d commit` — commit CRDT draft across all modified repos + workspace meta.
- Publishing — native `git push` to `<api>/git/<repoId>` with
  `-c "http.extraHeader=Authorization: Bearer <jwt>"`, leased to the mirror
  head (`--force-with-lease=refs/heads/<branch>:<old>`), then
  `POST /api/v1/workspaces/<ws>/ontology/sync-specs` per code repo and
  `POST .../docs/sync-committed`; `g5d push` is retired.
- `g5d revert` — revert draft/worktrees back to git HEAD.
- `g5d serve` — start editor role and Yjs sync.
- `g5d worker` — run a task in process or microVM isolation.

Defaults:
- **MicroVM isolation is preferred** and selected by default when supported.
- Default worker base image is `g5/worker:base`.
- Per-repo `g5.yaml` includes setup hints and OCI build/push references.
- Workspace operations are **backed by g5-api** and recorded in the workspace
  meta repo (`workspace.yaml` + `workspace.lock`).

#### 2. VS Code Extension

The VS Code extension is the **primary developer UX**. It communicates with the local `g5d` daemon and renders G5 functionality as native VS Code panels.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     VS Code + g5d Integration                                │
│                                                                              │
│  VS Code Extension                        g5d (editor)                      │
│  ─────────────────                        ─────────────                     │
│                                                                              │
│  Sidebar panels:                          Provides:                         │
│    • Task tree / timeline                   • REST + WS API (§11.9)        │
│    • Agent chat (streaming)                 • Git checkout + draft state    │
│    • Ontology browser                       • Worker pool management        │
│    • Draft status + [Commit]/[Fork]         • Agent loop (chat)            │
│                                              • Search index                 │
│  File editing:                                                              │
│    VS Code native editor                  g5d materializes CRDT            │
│    (reads/writes g5d's checkout dir)      drafts to disk → VS Code         │
│                                           sees file changes via FS events  │
│  IntelliSense:                                                             │
│    VS Code's own LSP clients              (g5d does not provide LSP)       │
│                                                                              │
│  Terminal:                                                                  │
│    VS Code integrated terminal            (user's local shell, not g5d's   │
│    (same directory as g5d checkout)        PTY — unless proxied for cloud  │
│                                            workers)                         │
│  Build/Test:                                                                │
│    VS Code tasks / native test runner     Workers execute via g5d API      │
└─────────────────────────────────────────────────────────────────────────────┘
```

**File editing model (Phase 1–2):** The extension opens `g5d`'s checkout directory as the VS Code workspace folder. `g5d` materializes committed state (and CRDT draft state, once committed) to this directory. VS Code reads and writes files normally. This is standard file-based editing — no CRDT integration in the editor itself.

**File editing model (Phase 3, optional):** The extension embeds a Yjs client and joins the `DraftSession` as a CRDT peer (same protocol as the browser and workers). Local edits flow through Yjs → g5-api → other clients. Remote edits from the browser or agents materialize in real time. This is the "Google Docs in VS Code" experience — architecturally identical to the worker-side sync model (AGENTS.md §4).

**Key decisions:**
- **VS Code owns IntelliSense** (C1, C2, C5). VS Code runs its own language servers directly. This gives users their existing LSP extensions (rust-analyzer, Pylance, etc.) with their existing configs. We do not replicate VS Code's toolchain auto-detection — it has years of institutional knowledge we cannot match (C5).
- **g5d owns the git checkout.** The extension does not manage git — g5d's checkout is the workspace. `[Commit]`, `[Fork]`, and branch operations go through the g5d API.
- **The extension is a sidecar, not a fork** (C5, C7). It adds panels and commands to VS Code; it does not replace VS Code's editor, terminal, or source control. Users keep their full VS Code setup (keybindings, themes, other extensions). Zero configuration required beyond opening a workspace.
- **No container dependency for local use** (C3). The extension and CLI work with git worktrees. Docker/Podman is never required on the developer's machine.

**Extension activation:**
```
User opens a directory containing .g5/ →
  Extension activates →
    Detects g5d running? →
      Yes: Attach via API (token from g5 CLI or OAuth flow)
      No:  Offer to start g5d (`g5 start .`)
```

#### 3. Web App (Browser)

The browser is the **canonical collaborative surface**. It is the only client with full CRDT real-time co-editing, the rich Tiptap/Monaco dual-mode editor (FRONTEND_DESIGN.md §11), and the complete workspace UI.

The browser connects to `g5-api` for REST/WS API and directly to `g5d` for terminal and file content streams (§11.6). IntelliSense is provided via **Open in IDE**, not through g5d.

The web app is ideal for:
- **Collaboration:** Multiple users editing specs simultaneously (CRDT).
- **Review:** Browsing ontology, task timelines, diffs, and comments.
- **Non-IDE contexts:** Quick edits, onboarding, documentation.

It is not ideal for:
- **Heavy refactoring:** No native refactoring tools (rename-across-files, extract-method). Use VS Code.
- **Debugging:** No DAP integration. Use VS Code.
- **Custom toolchains:** Browser has no access to local CLI tools. Use VS Code or CLI.

#### 4. Backend Workers

Workers are `g5d` instances running in worker mode (§11.1). They execute agent tasks headlessly. Workers are not a "user surface" — they are the execution backend. They are documented here for completeness because they participate in the same CRDT protocol as all other clients: worker tools write files → CRDT deltas propagate to g5-api → browser and VS Code see changes in real time.

See §11.8 for worker lifecycle, §8 for workspace initialization, §3 for base image strategy.

#### Phased Rollout

| Phase | CLI | VS Code | Web | Workers |
|-------|-----|---------|-----|---------|
| **1 (local-first)** | `g5d` launch commands only | Not yet | Full browser UI, single-user | Local git worktree workers |
| **2 (v1 ship)** | Full `g5` CLI | Sidecar panels (tasks, chat, diffs, commands). File editing via g5d checkout (no CRDT). | Full multi-user collaborative UI | Cloud Kata workers |
| **3 (collab IDE)** | (unchanged) | Yjs-in-extension: VS Code joins DraftSession as CRDT peer. Real-time co-editing with browser/agent. | (unchanged) | (unchanged) |

**Explicitly not planned:** Snapshot-based file sync (send `{path, newContent, expectedRev}`) for the VS Code extension. The entire editing model is CRDT-first; introducing a second concurrency model would create UX inconsistency. The path is: no CRDT (Phase 2, file-based) → full CRDT (Phase 3, Yjs peer). There is no intermediate "snapshot sync with conflict resolution" step.

#### Offline Mode

G5 is an online-first system — agents, task orchestration, and collaboration require g5-api. However, the architecture naturally supports **degraded offline operation** at different tiers:

| Surface | Offline editing | Offline agents | Reconnection strategy |
|---------|----------------|----------------|----------------------|
| **Web** | ❌ No. Browser has no local state; closing the tab loses unsaved content. | ❌ | N/A — requires network. |
| **VS Code (Tier 2, Yjs peer)** | ✅ **Yes.** The extension holds a local Yjs document. CRDTs are designed for offline-first: edits accumulate locally, and Yjs automatically merges divergent state on reconnect — no conflict resolution UI needed. | ❌ Agents require g5-api + workers. | Yjs sync protocol handles reconnection natively. The extension reconnects to g5-api, exchanges Yjs state vectors, and merges. Worst case: two users edited the same line offline → CRDT resolves deterministically (last-writer-wins per character position). |
| **VS Code / any editor (Tier 3, synced workspace)** | ⚠️ **Degraded.** The user edits files on disk in g5d's checkout. These are real files in a real git worktree — editing works fine offline. But changes are NOT CRDT-tracked. | ❌ | On reconnect, g5d diffs the local filesystem against the CRDT draft and reconciles. This is a git-level merge, not a CRDT merge — conflicts are possible and require manual resolution. |
| **CLI** | ❌ No. Thin client, no local state. | ❌ | N/A. |

**What works offline** (VS Code Tier 2):
- File editing with full CRDT merge safety
- Git operations (local commits, branch, log, diff)
- VS Code's own LSP (local language servers, autocomplete, diagnostics)
- Search (VS Code native search against local files)

**What requires network:**
- Agent execution (needs g5-api for task orchestration + workers for compute)
- Real-time collaboration (needs g5-api as CRDT authority for multi-user sync)
- Job submission (uplift, downlift, review)
- Task status and history (stored in g5-api's database)
- Comments and annotations (stored server-side)

**Design decision:** Offline mode is a Phase 3 capability, gated by the Yjs-in-extension work. We do not invest in a separate offline architecture. The CRDT model gives us offline for free once the extension is a Yjs peer — this is one of the core advantages of choosing CRDTs over operational transform. Tier 3 (synced workspace) provides a serviceable fallback: the user can always edit files on disk and rely on git for merge resolution.

### 11.18 Draft Materialization

When a CRDT draft is materialized to the filesystem (on `[Commit]`, on worker cold start, or for g5d's editor checkout), the system writes files and must handle failures.

#### Materialization Contract

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Draft → Disk Materialization                               │
│                                                                              │
│  Input:  CRDT draft (server-authoritative Yjs document)                     │
│  Output: Files written to a git checkout on the local filesystem            │
│                                                                              │
│  Trigger points:                                                            │
│    1. [Commit] button / `g5 commit`                                        │
│       → Materialize draft → git add -A → git commit → push               │
│    2. Worker cold start                                                     │
│       → git checkout baseCommit → materialize draft as batch overlay       │
│    3. g5d editor continuous sync                                            │
│       → On inbound CRDT delta, write affected file to editor checkout      │
│       → VS Code / local IDE sees FS event, reloads file                    │
│    4. Agent `write_file` / `edit_file_replace`                              │
│       → Write to local FS synchronously → enqueue CRDT delta outbound     │
│                                                                              │
│  Ordering guarantee:                                                        │
│    Trigger 4 (agent tools) writes FS first, then CRDT.                     │
│    Trigger 3 (editor sync) writes FS after CRDT delta arrives.             │
│    This means:                                                              │
│      • On the WRITING side (agent/human), local FS is always ≥ CRDT       │
│      • On the READING side (other clients), CRDT is always ≥ local FS     │
│      • Consistency converges within one sync cycle (~50ms local, ~200ms    │
│        cloud)                                                               │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Write Failure Handling

Disk writes can fail: permission errors, full disk, symlink loops, path encoding issues. The materialization layer handles these explicitly rather than silently dropping edits.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Write Failure Protocol                                     │
│                                                                              │
│  On materialization of a CRDT delta to disk:                                │
│                                                                              │
│  1. Attempt write (create parent dirs if needed, write atomically via       │
│     temp file + rename)                                                     │
│                                                                              │
│  2. Success → update file mtime/hash in local state tracker                │
│     → emit file-watch event to connected clients                           │
│     → done                                                                  │
│                                                                              │
│  3. Failure → log error with { path, error_kind, draft_rev }               │
│     → add to failed_writes set                                             │
│     → emit materialization_failed event:                                   │
│                                                                              │
│       {                                                                      │
│         "type": "materialization_failed",                                   │
│         "path": "src/auth/validator.rs",                                    │
│         "error": "ENOSPC: no space left on device",                         │
│         "draftRev": 42,                                                     │
│         "retriable": true                                                   │
│       }                                                                      │
│                                                                              │
│  4. Retry policy: exponential backoff (1s, 2s, 4s) up to 3 attempts       │
│     for retriable errors (ENOSPC after cleanup, EAGAIN, temp permission).  │
│     Non-retriable errors (path too long, encoding) fail immediately.       │
│                                                                              │
│  5. Persistent failure → the file remains in the CRDT draft but NOT on     │
│     disk. The CRDT is the source of truth — no data is lost. But:         │
│     • The file tree shows a ⚠ badge on the failed file                    │
│     • Agent `run_in_terminal` may see stale content on disk               │
│     • `[Commit]` is blocked until all materializations succeed:           │
│                                                                              │
│       ┌─────────────────────────────────────────────────────────────────┐  │
│       │  ⚠ Cannot commit: 1 file failed to materialize                 │  │
│       │                                                                 │  │
│       │  src/auth/validator.rs — ENOSPC: no space left on device       │  │
│       │                                                                 │  │
│       │  The CRDT draft has the correct content. Fix the disk issue    │  │
│       │  and retry, or discard this file's draft changes.              │  │
│       │                                                                 │  │
│       │  [Retry Materialization]  [Discard File Draft]  [Cancel]       │  │
│       └─────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  Key invariant: CRDT never loses data due to disk failure.                 │
│  The draft is server-side and durable. Materialization failure means the   │
│  local filesystem is temporarily inconsistent with the draft — not that    │
│  the edit was lost.                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Agent-side failure notification:** When a worker's materialization fails (inbound CRDT delta could not be written to disk), the error is injected into the agent's session as a system message:

```
[system] Materialization failed: src/auth/validator.rs — ENOSPC: no space left on device.
         The file on disk may be stale. CRDT draft has the correct content.
         Builds and tests may see outdated file content until resolved.
```

The agent can respond by:
- Running cleanup commands (`run_in_terminal("rm -rf target/")`) to free space.
- Retrying the operation (the next `write_file` call will re-attempt materialization).
- Calling `transition_task_state(state: "blocked", reason: "Disk full on worker")` to escalate.

### 11.19 Worker Workspace Lifecycle

Each worker — local or cloud — follows a deterministic lifecycle for workspace preparation and execution. The terminology below standardizes how the docs refer to each phase.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Worker Workspace Lifecycle                                 │
│                                                                              │
│  Phase 1: PREPARE BASE                                                      │
│  ──────────────────────                                                      │
│  Produces a prepared workspace keyed by:                                    │
│    (repo, base_commit, image_digest, env_key)                               │
│                                                                              │
│  Steps:                                                                      │
│    a. Start from pinned worker image (§3, §10.11)                          │
│       Image is built from .devcontainer/ or g5/worker:base + features      │
│       Image digest is recorded — same digest = same environment            │
│    b. Checkout repo at base_commit                                          │
│       Via GCS snapshot (§8.1) or git clone --depth=1 fallback              │
│    c. Hydrate caches (§8, hydrate-caches.sh)                               │
│       Detect Cargo.toml/package.json/etc. → pull seed caches from GCS     │
│    d. Write manifest:                                                       │
│       { repo, base_commit, image_digest, env_key, prepared_at }            │
│    e. Base is now IMMUTABLE for this key tuple                             │
│                                                                              │
│  Cloud optimization: GCS snapshot tar.zst is itself a "prepared base."     │
│  If a snapshot exists for (repo, commit), steps b-c are replaced with     │
│  a single tar extraction — already contains repo tree + warm caches.       │
│                                                                              │
│  Local optimization: ~/.g5/cache/ persists across runs. The local SSD      │
│  acts as a permanent L1 — subsequent workers for the same repo reuse       │
│  the cached checkout and only run git fetch + reset.                       │
│                                                                              │
│  Phase 2: CLONE + PATCH (start a run)                                       │
│  ────────────────────────────────────                                        │
│  From the prepared base, create an isolated workspace for this task:       │
│                                                                              │
│    a. Clone base                                                            │
│       Local: git worktree add (CoW on APFS/btrfs, copy otherwise)          │
│       Cloud: container start from prepared image layer                      │
│    b. Create task branch: git checkout -b task/<task-id>                    │
│    c. Apply CRDT draft overlay (if resuming a task with dirty state)       │
│       → Materialize all draft file contents to disk (§11.18)               │
│       → On materialization failure: emit event, agent is notified          │
│    d. Worker enters READY state, begins accepting tool calls               │
│                                                                              │
│  Phase 3: EXECUTE                                                           │
│  ────────────────                                                            │
│  Worker runs the agent loop (AGENTS.md §1):                                │
│    • Agent reads/writes files → local FS + CRDT delta outbound             │
│    • Inbound CRDT deltas → materialized to local FS (§11.18)             │
│    • Checkpoints: git commit at milestones (AGENTS.md §4.1)               │
│    • Terminal commands: build, test, lint via run_in_terminal               │
│                                                                              │
│  Phase 4: COLLECT                                                           │
│  ────────────────                                                            │
│  On task completion (transition_task_state → complete):                     │
│    a. Materialize full CRDT draft to disk                                  │
│    b. git add -A && git commit (with structured message)                   │
│    c. git push to remote                                                    │
│    d. Reset CRDT draft to clean                                            │
│    e. Report results (cost, tokens, files changed) to g5-api              │
│                                                                              │
│  Phase 5: CLEANUP                                                           │
│  ─────────────────                                                           │
│    Local: git worktree remove (frees disk, branch remains in repo)         │
│    Cloud: pod terminated, container destroyed                               │
│    Caches: L1 entries remain for future workers (LRU eviction, §11.16)    │
│    Old bases: GC after configurable TTL (default: 7 days unused)           │
│                                                                              │
│  Key invariant: the prepared base is never mutated during execution.       │
│  Every run starts from a clean clone. This makes workspace state           │
│  deterministic and reproducible — same (repo, commit, image, env)          │
│  always produces the same starting filesystem.                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Image digest pinning:** The prepared base includes the worker image digest in its cache key. This means a change to the `.devcontainer/` (which produces a new image digest) invalidates all cached bases for that workspace, forcing a fresh prepare. This is intentional — environment changes should produce clean rebuilds.

```
# Cache key for prepared bases
base_key = f"{repo_url}:{base_commit}:{image_digest[:12]}:{env_key}"

# Examples:
# github.com/org/app:abc1234:sha256:3f2a1b:default     → standard base
# github.com/org/app:abc1234:sha256:3f2a1b:rust-nightly → variant with nightly
# github.com/org/app:def5678:sha256:3f2a1b:default      → different commit, same image
# github.com/org/app:abc1234:sha256:9c8d7e:default       → same commit, new image (devcontainer changed)
```

---

## 12. Authorization & Permissions

### 12.1 Principles

Every resource in G5 — workspaces, branches, workers, sessions, files — is
governed by **OAuth-style scopes** assigned to users and organizations.
The canonical data model (tables, scope resolution algorithm, predefined
role sets) is in **API_AND_STORAGE.md §2.11**. The REST API for managing
scopes is in **API_AND_STORAGE.md §3.16**.

| Principle | Implementation |
|-----------|----------------|
| **OAuth identity everywhere** | Every request carries an OAuth token (user JWT) or service token (worker) |
| **Scopes on users + orgs** | Org-level default scopes ∪ per-user grants ∪ optional per-workspace overrides |
| **Default to user-private** | Workers, scratch branches, and sessions belong to their creator |
| **Explicit sharing** | Access must be granted — no implicit inheritance across users |
| **Uniform model** | Same scope enforcement for local desktop and cloud — `g5d` enforces locally too |

### 12.2 Scope Levels

Scopes follow the `resource:action` pattern (e.g., `workspaces:read`,
`jobs:execute`). See API_AND_STORAGE.md §2.11 for the full catalog.

Convenience roles that bundle scopes:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Permission Matrix                                   │
│                                                                              │
│  ┌──────────┐  Can view the resource, read its contents,                   │
│  │  READ    │  see its status. Cannot change anything.                     │
│  └──────────┘                                                               │
│                                                                              │
│  ┌──────────┐  Can modify the resource: edit files, change config,         │
│  │  MODIFY  │  update metadata, push to branches.                          │
│  └──────────┘                                                               │
│                                                                              │
│  ┌──────────┐  Can trigger actions: start builds, run tests,               │
│  │  EXECUTE │  invoke agent loops, run shell commands on workers.          │
│  └──────────┘                                                               │
│                                                                              │
│  ┌──────────┐  Can create child resources: new branches, new workers,      │
│  │  CREATE  │  new sessions, new tasks within the workspace.                 │
│  └──────────┘                                                               │
│                                                                              │
│  Shorthand roles that bundle permissions:                                   │
│                                                                              │
│    viewer   = READ                                                          │
│    editor   = READ + MODIFY                                                 │
│    operator = READ + MODIFY + EXECUTE                                       │
│    admin    = READ + MODIFY + EXECUTE + CREATE                              │
│    owner    = admin + can grant/revoke access                               │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 12.3 Resource Hierarchy & Default Ownership

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       Resource Hierarchy                                     │
│                                                                              │
│  Tenant (org/team)                                                          │
│  └── Workspace                                                                │
│      ├── Branch                                                             │
│      │   └── (files, commits — inherit workspace permissions)                 │
│      ├── Worker                                                             │
│      │   └── (task branch, worktree, build outputs)                        │
│      └── Session (chat)                                                     │
│          └── (messages, tool calls, artifacts)                              │
│                                                                              │
│  Default ownership rules:                                                   │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │  Resource         Default Owner    Default Others     Override       │  │
│  │  ─────────        ─────────────    ──────────────     ────────       │  │
│  │  Workspace        creator          team: viewer       configurable   │  │
│  │  Branch (main)    workspace admins   workspace viewers    per-branch     │  │
│  │  Branch (task/*)  task creator     workspace viewers    per-branch     │  │
│  │  Worker (local)   spawning user    NONE               explicit grant │  │
│  │  Worker (cloud)   task creator     NONE               explicit grant │  │
│  │  Session          creator          NONE               explicit grant │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  Workers and sessions are PRIVATE BY DEFAULT.                               │
│  A worker on my laptop is mine. Nobody else can see it, connect to it,     │
│  or read its output — unless I explicitly share it.                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 12.4 Identity Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Identity Sources                                    │
│                                                                              │
│  ┌────────────────────┐  ┌────────────────────┐  ┌──────────────────────┐ │
│  │  User (human)      │  │  Service Account   │  │  Agent Identity      │ │
│  │                    │  │                    │  │                      │ │
│  │  OAuth: GitHub,    │  │  OAuth: client     │  │  Derived from the   │ │
│  │  Google, OIDC,     │  │  credentials flow  │  │  session that       │ │
│  │  corporate SSO     │  │                    │  │  spawned it         │ │
│  │                    │  │  Used by: CI/CD,   │  │                      │ │
│  │  Carries scopes:   │  │  Slack bot,        │  │  Agent inherits     │ │
│  │  user:id, tenant,  │  │  WhatsApp bridge,  │  │  the creating       │ │
│  │  teams             │  │  deployed agents   │  │  user's permissions │ │
│  └────────────────────┘  └────────────────────┘  └──────────────────────┘ │
│                                                                              │
│  Every g5d API call requires a Bearer token:                                │
│    Authorization: Bearer <oauth-access-token>                               │
│                                                                              │
│  Desktop mode:                                                              │
│    $ g5d --role editor --workspace . --port 7500                              │
│    # On first run: opens browser for OAuth login                            │
│    # Stores refresh token in ~/.g5/credentials.json                         │
│    # All local workers inherit this identity                                │
│    # Browser connects with same user's token                                │
│                                                                              │
│  Cloud mode:                                                                │
│    Pod gets identity from:                                                  │
│      1. Workload Identity (GKE) → maps to service account                  │
│      2. Forwarded user token (for user-initiated tasks)                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 12.5 Access Control on Workers

Workers are the most sensitive resource — they can execute arbitrary code. Default: **private to the creating user**.

```typescript
interface WorkerAccessPolicy {
  workerId: string;
  
  // Owner — full control, set at creation time
  owner: Identity;                   // The user who spawned this worker
  
  // Explicit grants
  grants: AccessGrant[];
}

interface AccessGrant {
  identity: Identity;                // User, service account, or team
  permissions: Permission[];         // ['read', 'modify', 'execute', 'create']
  scope?: string;                    // Optional: restrict to subtree, e.g., "src/**"
  expiresAt?: string;                // Optional: time-limited access
}

type Identity = 
  | { type: 'user'; userId: string }
  | { type: 'service_account'; clientId: string }
  | { type: 'team'; teamId: string }
  | { type: 'everyone_in_tenant' };   // Rare — explicit "open to org"

type Permission = 'read' | 'modify' | 'execute' | 'create';
```

**Default worker permissions by scenario:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│  Scenario: I spawn a local worker as a scratchpad                           │
│  ──────────────────────────────────────────────────                          │
│  $ g5d --role worker --workspace . --task scratch-1                           │
│                                                                              │
│  owner: me (from ~/.g5/credentials.json)                                    │
│  grants: []  ← empty, nobody else has access                               │
│                                                                              │
│  • Only I can connect to it from the browser                                │
│  • Only my agent sessions can send tool calls to it                         │
│  • If someone else on my team opens the workspace, they don't see this worker │
│  • I can grant access: POST /api/v1/workers/:id/grants                    │
│                                                                              │
│  ─────────────────────────────────────────────────                           │
│                                                                              │
│  Scenario: Team CI spawns a cloud worker for a PR                           │
│  ──────────────────────────────────────────────────                          │
│  owner: ci-bot (service account)                                            │
│  grants:                                                                    │
│    - { identity: { type: 'team', teamId: 'eng' }, permissions: ['read'] }  │
│    - { identity: PR author, permissions: ['read', 'execute'] }             │
│                                                                              │
│  • Team can view build output                                               │
│  • PR author can re-run tests                                               │
│  • Nobody else can modify files on the worker                               │
│                                                                              │
│  ─────────────────────────────────────────────────                           │
│                                                                              │
│  Scenario: Pair programming — I share my worker with a colleague            │
│  ──────────────────────────────────────────────────                          │
│  POST /api/v1/workers/:id/grants                                           │
│  { "identity": { "type": "user", "userId": "alice" },                      │
│    "permissions": ["read", "modify", "execute"] }                           │
│                                                                              │
│  Now Alice can also send tool calls and see output                          │
│  She cannot grant access to others (no 'create' or owner rights)            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 12.6 Access Control on Workspaces & Branches

```typescript
interface WorkspaceAccessPolicy {
  workspaceId: string;
  tenantId: string;
  
  // Workspace-level roles
  roles: WorkspaceRole[];
  
  // Branch-level overrides
  branchPolicies: BranchPolicy[];
}

interface WorkspaceRole {
  identity: Identity;
  role: 'viewer' | 'editor' | 'operator' | 'admin' | 'owner';
}

interface BranchPolicy {
  branchPattern: string;            // "main", "release/*", "task/*"
  
  // Who can do what on this branch
  rules: BranchRule[];
}

interface BranchRule {
  identity: Identity;
  permissions: Permission[];
  
  // Branch-specific constraints
  requireReview?: boolean;           // Require PR review for modify
  requireCI?: boolean;               // Require passing CI for modify
  protectHistory?: boolean;          // Prevent force-push
}
```

**Example workspace config (`.g5/permissions.yaml`):**
```yaml
workspace:
  default_role: viewer               # Everyone in tenant can read

  roles:
    - identity: { type: team, teamId: backend-team }
      role: operator                  # Can read, modify, execute
    - identity: { type: user, userId: alice }
      role: owner                     # Full control

  branches:
    - pattern: "main"
      rules:
        - identity: { type: everyone_in_tenant }
          permissions: [read]
        - identity: { type: team, teamId: backend-team }
          permissions: [read, modify]
          requireReview: true
          requireCI: true
          protectHistory: true

    - pattern: "task/*"
      rules:
        # Task branches: creator gets full access
        - identity: { type: branch_creator }
          permissions: [read, modify, execute, create]
        # Team can read
        - identity: { type: everyone_in_tenant }
          permissions: [read]
```

### 12.7 Access Control on Sessions

Chat sessions follow the same model. The user who created the session owns it.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Session Permissions                                       │
│                                                                              │
│  Default: only the creator can interact with a session.                     │
│                                                                              │
│  Permission   What it allows                                                │
│  ──────────   ──────────────                                                │
│  READ         View message history, see streaming output                    │
│  MODIFY       Inject messages into the agent loop                           │
│  EXECUTE      Pause/resume/abort the agent, trigger checkpoints            │
│  CREATE       Fork the session, spawn sub-agents                            │
│                                                                              │
│  Multi-client scenarios (§10.3):                                            │
│    Slack bot injects a message → needs MODIFY on the session                │
│    Deployed agent runs autonomously → owns its session                      │
│    Colleague watches my agent work → needs READ on the session              │
│    Team lead pauses a runaway agent → needs EXECUTE on the session          │
│                                                                              │
│  Sessions inherit worker permissions:                                       │
│    If you can't EXECUTE a worker, you can't run tools on it                │
│    even if you have EXECUTE on the session.                                 │
│    Both must be authorized.                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 12.8 Enforcement Points

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      Where permissions are checked                           │
│                                                                              │
│  Browser → g5d (editor)                                                     │
│    │                                                                        │
│    │ 1. WebSocket connect: validate OAuth token, check workspace READ        │
│    │ 3. Chat message: check session MODIFY                                 │
│    │ 4. Chat control: check session EXECUTE                                │
│    │ 5. Search: check workspace READ                                         │
│    ▼                                                                        │
│  g5d (editor) → g5d (worker)                                               │
│    │                                                                        │
│    │ 6. Tool call (write_file): check worker MODIFY + branch MODIFY       │
│    │ 7. Tool call (build/test): check worker EXECUTE                       │
│    │ 8. Tool call (read_file): check worker READ                           │
│    ▼                                                                        │
│  g5d → Control Plane API                                                    │
│    │                                                                        │
│    │ 9. Spawn worker: check workspace CREATE                                 │
│    │ 10. Delete worker: check worker owner                                 │
│    │ 11. Grant access: check resource owner                                │
│                                                                              │
│  Desktop mode:                                                              │
│    Same checks, but the token is always the local user's.                   │
│    Local workers are owned by local user → all checks pass trivially.      │
│    Remote API calls (e.g., cloud worker spawn) carry the same token.       │
│                                                                              │
│  The permission check is a function call, not a network hop.               │
│  g5d embeds the policy engine.                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 12.9 Permissions API

Authorization uses **OAuth-style scopes** assigned to users and organizations.
The canonical data model is in API_AND_STORAGE.md §2.11; the REST endpoints
are in API_AND_STORAGE.md §3.16. This section documents the worker-relevant
aspects only.

**Scope check (fast path):** The `g5d` middleware extracts `scopes` from the
JWT and checks them against the required scope for each endpoint. No DB
query — the JWT is authoritative for its 1-hour lifetime.

```
# Scope management (see API_AND_STORAGE.md §3.16 for full list)
GET    /api/v1/auth/me/scopes                    # Effective scopes for current user
POST   /api/v1/auth/check                        # Check specific scope(s)
  body: { "scopes": ["jobs:execute"], "workspaceId": "..." }

# Per-workspace permission overrides
GET    /api/v1/workspaces/:id/permissions           # List overrides
PUT    /api/v1/workspaces/:id/permissions/:userId   # Set overrides
DELETE /api/v1/workspaces/:id/permissions/:userId   # Revert to org-level

# Worker-relevant internal endpoints
POST   /api/internal/workers/:id/rotate-token     # Rotate service token
```

**Worker authorization:** Workers authenticate with service tokens (not user
JWTs). A service token grants the worker a fixed set of internal scopes:
`tasks:execute`, `drafts:write`, `files:read`. These cannot be customized
per-worker — all workers have identical capabilities.

---

## 13. IDE Integration (Open in IDE)

G5 intentionally does **not** run or proxy language servers. IntelliSense, refactoring,
and diagnostics are provided by the user's IDE via **Open in IDE**.

- The web app exposes an **Open in IDE** action that opens the local workspace
  path managed by g5d.
- g5d provides file materialization, git status, terminal, and search streams;
  the IDE uses its native LSP configuration and toolchain.
- This keeps onboarding simple and avoids shipping per-language server setup
  inside g5d.

## 14. Future Extensions

### 14.1 Explicitly Supported

| Feature | Description | Backend |
|---------|-------------|---------|
| **Persistent workspace volumes** | PVCs for long-running workspaces | Kata + K8s CSI |
| **Volume cloning** | Fast workspace duplication | CSI snapshots |
| **Bazel remote cache** | Native Bazel cache service | GCS-backed |
| **Gradle build cache service** | HTTP cache node | Self-hosted |
| **Air-gapped deployments** | No internet access | S3-compatible storage |

### 14.2 Architecture for Persistent Volumes

```yaml
# Future: PVC-backed workspace
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: workspace-${WORKSPACE_ID}
spec:
  accessModes: ["ReadWriteOnce"]
  storageClassName: fast-ssd
  resources:
    requests:
      storage: 100Gi
  dataSource:
    kind: VolumeSnapshot
    name: workspace-${WORKSPACE_ID}-snapshot-${COMMIT}
```

---

## 15. Summary

This architecture is driven by four optimization criteria (§0):

| Principle | How Achieved |
|-----------|--------------------------------------------------------------|
| **P1: Native editing feel** | CRDT draft layer, Monaco + TextMate grammars, editor has own git checkout, **Open in IDE** for IntelliSense |
| **P2: Agents need real FS + compute** | Workers have full worktrees, toolchain caches, PTYs, shell |
| **P3: Isolated, committed work** | Per-task branches, Kata VMs (cloud), draft→commit flow, checkpoints |
| **P4: Collaborative editing** | Yjs CRDT, server-authoritative drafts, cross-branch comments, persistent chat history |

And the architecture delivers:

| Goal | How Achieved |
|------|--------------------------------------------------------------|
| **One binary** | `g5d` runs as editor or worker, same code, different config |
| **Four surfaces** | CLI, VS Code extension, web app, workers — all share the same API (§11.17) |
| **Local-first** | Desktop runs editor + multiple workers as processes, zero cost |
| **Cloud overflow** | When local slots are full, cloud provisions Kata VM workers |
| **Strong isolation** | Cloud workers: VM-isolated Kata. Local workers: git worktrees |
| **High performance** | Content-addressed caches (local shared, cloud L1/L2 GCS) |
| **Editor intelligence** | Editor runs search, browser connects via WebSocket |
| **Unified chat** | Agent loop on editor, tool calls forwarded to attached worker |
| **Intervention** | Message injection (indirect) + direct task attachment (§10.9) |
| **Decision trail** | All chat sessions persisted, linkable, searchable (§10.14) |
| **Draft model** | Server-side CRDT, base commit + diff, [Commit] / [Fork], materialization with failure handling (§11.18) |
| **Workspace lifecycle** | PrepareBase → CloneRun → Execute → Collect → Cleanup, keyed by (repo, commit, image_digest, env_key) (§11.19) |
| **Portability** | Runs on developer laptop, hosted GCP, customer K8s, or on-prem |
| **Security** | Cloud: per-workspace IAM, Kata isolation. Local: user's own machine |
| **Authorization** | OAuth everywhere, 4-permission RBAC, workers private by default |
| **Reproducibility** | Deterministic base images + versioned caches, immutable prepared bases |
| **Observability** | Structured logging + cache metrics |

**One binary, two roles:**
- **`g5d --role editor`** — Chat host + file sync + worker pool management. Materializes CRDT drafts to disk. CLI, VS Code extension, and browser all connect here. All views come from worker when connected.
- **`g5d --role worker`** — Read-write workspace on git worktree (local) or cloned repo (cloud). Build + test + shell.

**Four client surfaces:** CLI (headless), VS Code extension (primary IDE), web app (collaboration), workers (execution). All share the same API. See §11.17.

**Deterministic workspace lifecycle:** PrepareBase (pinned image + repo snapshot + cache hydration) → CloneRun (worktree/container + draft overlay) → Execute → Collect (commit + push) → Cleanup. See §11.19.

**Local workers first, cloud as overflow.**

**This is the plan of record.**
