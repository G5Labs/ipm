#!/usr/bin/env bash
# =============================================================================
# stage-local-worker-runtime.sh — Stage the LINUX guest runtime dir for a
# local/CI `g5d host` workspace host.
#
# Why this exists: `g5d host` attaches a read-only runtime block disk
# to every workspace VM at /opt/g5 and fail-closes
# at startup (`backend/g5d/src/cli/host.rs` runtime validation) unless it contains:
#   g5d          — the in-VM `g5d agent-runner` entrypoint
#   node         — the OFFICIAL self-contained Node.js (no libnode.so*) the
#                  vendored `codex` npm launcher runs on
#   claude       — the native Claude Code CLI
#   codex        — wrapper for the vendored OpenAI Codex CLI npm package
#   gh           — GitHub CLI for agent bash when GH_TOKEN is provided
#   gcloud       — Google Cloud CLI for agent bash when cloud credentials are provided
#   kubectl      — Kubernetes CLI for agent bash when kubeconfig/cluster auth is provided
#   cargo/rustc  — Rust unit-test toolchain for agent bash
#   cargo-nextest, just — Rust test runners/recipes used by this codebase
#   npm/pnpm     — client Vitest package managers for agent bash
#   python3      — the portable Python runtime gcloud runs on
# The guest kernel is Linux, so every binary in
# that directory must be a Linux ELF for the host's CPU architecture. On a
# developer Mac (Apple Silicon) these cannot be host binaries — they are
# produced inside `--platform linux/arm64` Docker containers, mirroring the
# signed host image's Packer scripts (infra/packer/g5-worker/scripts/
# 30-build-g5d.sh + 40-install-claude.sh: same NODE_VERSION pin, same
# SHASUMS256 verification, same no-libnode guard, same in-guest smoke gate).
# SOT: g5/g5d/host D16, g5/g5d/isolation macOS notes.
#
# The same script stages the linux-x64 runtime for the CI workspace-VM lane;
# pass --arch x64 there.
#
# Usage:
#   scripts/stage-local-worker-runtime.sh [--arch arm64|x64] [--out DIR]
#                                         [--only-g5d]
#                                         [--g5d-from-image IMAGE]
#
#   --g5d-from-image IMAGE  extract the Linux g5d from an already-built image
#                           (e.g. CI's `g5d-worker:ci`, which carries it at
#                           /usr/local/bin/g5d) instead of building the
#                           Dockerfile.g5d builder stage from scratch.
# Env:
#   NODE_VERSION          node pin (default 22.23.4 — keep equal to the Packer
#                         scripts' pin in 40-install-claude.sh)
#   NODE_SHA256           optional hard pin for the node tarball (else verified
#                         against the official SHASUMS256.txt over TLS)
#   PNPM_VERSION          pnpm pin vendored into the mounted Node closure
#                         (default 11.9.0; keep equal to Packer/CI).
#   CLAUDE_CODE_VERSION   @anthropic-ai/claude-code pin (default read from the
#                         single-source scripts/claude-code-cli-version file,
#                         shared with the Packer image build — #1342 D-13). The
#                         resolved version is always recorded in
#                         <out>/runtime/claude-code.version.
#   CODEX_VERSION         @openai/codex pin (default read from the
#                         single-source scripts/codex-cli-version file, shared
#                         with the Packer image build — #1342 D-13).
#   GH_CLI_VERSION        GitHub CLI pin (default 2.95.0, kept equal to the
#                         Packer variable and infra/ci/cloudbuild-packer-worker.yaml).
#   GH_CLI_SHA256         optional hard pin for the gh tarball (else verified
#                         against GitHub CLI release checksums over TLS).
#   GCLOUD_CLI_VERSION    Google Cloud CLI pin (default 575.0.0, kept equal to
#                         the Packer variable and infra/ci/cloudbuild-packer-worker.yaml).
#   GCLOUD_CLI_SHA256     optional hard pin for the gcloud tarball (else TLS only).
#   KUBECTL_VERSION       kubectl pin (default 1.36.2, kept equal to Packer/CI).
#   KUBECTL_SHA256        optional hard pin for kubectl (else verified against
#                         the Kubernetes release .sha256 over TLS).
#   RUST_TOOLCHAIN        Rust toolchain pin (default 1.97.1, kept equal to Packer).
#   RUSTUP_VERSION        rustup-init pin (default 1.29.0, kept equal to Packer).
#   RUSTUP_INIT_SHA256    optional hard pin for rustup-init (else verified
#                         against rustup-init.sha256 over TLS).
#   JUST_VERSION          just pin (default 1.55.1, kept equal to Packer/CI).
#   JUST_SHA256           optional hard pin for just (else verified against
#                         release SHA256SUMS over TLS).
#   CARGO_NEXTEST_VERSION cargo-nextest pin (default 0.9.138, kept equal to CI).
#   CARGO_NEXTEST_SHA256  optional hard pin for cargo-nextest (else verified
#                         against release .sha256 over TLS).
#   RUNTIME_STAGING_IMAGE image used to download and assemble the mounted
#                         runtime closure (default ubuntu:24.04).
#   RUNTIME_STAGING_PACKAGES_PREINSTALLED
#                         set to 1 only when RUNTIME_STAGING_IMAGE already
#                         contains the required distro packages; the shared
#                         package helper verifies them and performs no APT I/O.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GEN_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# --- args --------------------------------------------------------------------
ARCH=""
# Default to the worker-host env's per-instance host dir (worker-host-env.sh
# exports it; per-org instances use .dev/g5d-host-<slug>), falling back to the
# historical singleton dir for standalone invocations.
OUT_DIR="${G5_DEV_HOST_DIR:-${GEN_ROOT}/.dev/g5d-host}"
ONLY_G5D=0
G5D_FROM_IMAGE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --arch)           ARCH="$2"; shift 2 ;;
        --out)            OUT_DIR="$2"; shift 2 ;;
        --only-g5d)       ONLY_G5D=1; shift ;;
        --g5d-from-image) G5D_FROM_IMAGE="$2"; shift 2 ;;
        -h|--help)        grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

if [ -z "$ARCH" ]; then
    case "$(uname -m)" in
        arm64|aarch64) ARCH=arm64 ;;
        x86_64)        ARCH=x64 ;;
        *) echo "FATAL: unsupported host arch $(uname -m); pass --arch" >&2; exit 1 ;;
    esac
fi
case "$ARCH" in
    arm64) DOCKER_PLATFORM="linux/arm64"; GH_ARCH="arm64"; GCLOUD_ARCH="arm"; KUBECTL_ARCH="arm64" ;;
    x64)   DOCKER_PLATFORM="linux/amd64"; GH_ARCH="amd64"; GCLOUD_ARCH="x86_64"; KUBECTL_ARCH="amd64" ;;
    *) echo "FATAL: --arch must be arm64 or x64 (got: $ARCH)" >&2; exit 2 ;;
esac
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd -P)"
NODE_ARCH="linux-${ARCH}"
NODE_VERSION="${NODE_VERSION:-22.23.4}"
PNPM_VERSION="${PNPM_VERSION:-11.9.0}"
PNPM_VERSION="${PNPM_VERSION#v}"
# Provider CLI pins default from the single-source pin files (#1342 D-13);
# the env overrides remain for deliberate bump testing.
CLAUDE_CODE_VERSION="${CLAUDE_CODE_VERSION:-$(cat "${SCRIPT_DIR}/claude-code-cli-version")}"
CODEX_VERSION="${CODEX_VERSION:-$(cat "${SCRIPT_DIR}/codex-cli-version")}"
GH_CLI_VERSION="${GH_CLI_VERSION:-2.95.0}"
GH_CLI_VERSION="${GH_CLI_VERSION#v}"
GCLOUD_CLI_VERSION="${GCLOUD_CLI_VERSION:-575.0.0}"
GCLOUD_CLI_VERSION="${GCLOUD_CLI_VERSION#v}"
KUBECTL_VERSION="${KUBECTL_VERSION:-1.36.2}"
KUBECTL_VERSION="v${KUBECTL_VERSION#v}"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-1.97.1}"  # tracks the workspace toolchain (safe __cpuid needs >= 1.97)
RUSTUP_VERSION="${RUSTUP_VERSION:-1.29.0}"
JUST_VERSION="${JUST_VERSION:-1.55.1}"
CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION:-0.9.138}"
RUNTIME_STAGING_IMAGE="${RUNTIME_STAGING_IMAGE:-ubuntu:24.04}"
RUNTIME_STAGING_PACKAGES_PREINSTALLED="${RUNTIME_STAGING_PACKAGES_PREINSTALLED:-0}"
case "$RUNTIME_STAGING_PACKAGES_PREINSTALLED" in
    0|1) ;;
    *) echo "FATAL: RUNTIME_STAGING_PACKAGES_PREINSTALLED must be 0 or 1" >&2; exit 2 ;;
esac

# Hard supply-chain pin for the default node version (first verified against
# the official SHASUMS256.txt on 2026-10-22); other versions/arches fall back
# to manifest verification inside the container.
if [ -z "${NODE_SHA256:-}" ] && [ "$NODE_VERSION" = "22.23.4" ] && [ "$ARCH" = "arm64" ]; then
    NODE_SHA256="6287aa46b6c375d816e63ae66d024fbbfd70404825aabdfdf898a970871d2f4b"
fi

RUNTIME_BIN="${OUT_DIR}/runtime/bin"
RUNTIME_ASSETS="${OUT_DIR}/runtime/assets"
# Declared-module catalog + pin (plan P4.2b item 13); the host mounts this
# directory at /opt/g5/modules and hands the pin to `g5d dispatch`.
RUNTIME_MODULES="${OUT_DIR}/runtime/modules"

info() { printf '[stage-runtime] %s\n' "$*"; }
fail() { printf '[stage-runtime] FATAL: %s\n' "$*" >&2; exit 1; }

codex_ca_bundle_staging() {
    if [ ! -s "$RUNTIME_BIN/ca-certificates.crt" ]; then
        info "staging CA bundle for Codex TLS trust store"
        docker run --rm --platform "$DOCKER_PLATFORM" \
            --user 0:0 \
            -e APT_PACKAGES_PREINSTALLED="$RUNTIME_STAGING_PACKAGES_PREINSTALLED" \
            -v "$RUNTIME_BIN":/stage \
            -v "$GEN_ROOT/scripts/apt-install-with-retry.sh":/apt-install-with-retry.sh:ro \
            "$RUNTIME_STAGING_IMAGE" \
            bash -c '
                set -euo pipefail
                /apt-install-with-retry.sh ca-certificates
                install -m 0644 /etc/ssl/certs/ca-certificates.crt /stage/ca-certificates.crt
                test -s /stage/ca-certificates.crt
            '
    fi
}

a25_runtime_smoke_gate() {
    # ubuntu:26.04, NOT 24.04: the locally-staged g5d is built on the 26.04
    # tier (glibc 2.43), and microvm [D12] permits build-old/run-new only — a
    # 24.04 smoke guest is the forbidden direction and would be an invalid
    # oracle the moment the binary references a post-2.39 symbol.
    info "running A2.5-parity smoke gate (ubuntu:26.04, ${DOCKER_PLATFORM})"
    docker run --rm --platform "$DOCKER_PLATFORM" \
        -v "$RUNTIME_BIN":/opt/g5/bin:ro \
        -v "$RUNTIME_ASSETS":/opt/g5/assets:ro \
        -e HOME=/tmp \
        ubuntu:26.04 \
        bash -c '
            set -euo pipefail
            export LD_LIBRARY_PATH=/opt/g5/bin
            export SSL_CERT_FILE=/opt/g5/bin/ca-certificates.crt
            export NODE_EXTRA_CA_CERTS="$SSL_CERT_FILE"
            export REQUESTS_CA_BUNDLE="$SSL_CERT_FILE"
            export CURL_CA_BUNDLE="$SSL_CERT_FILE"
            export CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK=1
            export PATH=/opt/g5/bin:$PATH
            test -s "$SSL_CERT_FILE"
            grep -q "BEGIN CERTIFICATE" "$SSL_CERT_FILE"
            /opt/g5/bin/g5d --version
            /opt/g5/bin/node --version
            /opt/g5/bin/python3 -c "import json, pathlib, subprocess, sys; print(sys.version.split()[0])"
            test -s /opt/g5/assets/harness/AGENTS.md
            /opt/g5/bin/claude --version
            /opt/g5/bin/codex --version
            /opt/g5/bin/gh --version
            /opt/g5/bin/gcloud --version
            /opt/g5/bin/gke-gcloud-auth-plugin --version
            /opt/g5/bin/kubectl version --client=true
            /opt/g5/bin/npm --version
            /opt/g5/bin/pnpm --version
            /opt/g5/bin/corepack --version
            /opt/g5/bin/cargo --version
            /opt/g5/bin/rustc --version
            /opt/g5/bin/cargo-nextest --version
            /opt/g5/bin/cargo nextest --version
            /opt/g5/bin/just --version
        ' || fail "smoke gate failed — the staged runtime cannot execute under the mounted in-VM layout"
}

command -v docker >/dev/null 2>&1 || fail "docker is required (it builds the Linux guest artifacts). Install/start Docker Desktop, or fall back to the dockerized process worker: G5_DEV_WORKER=process just dev-up"
docker info >/dev/null 2>&1 || fail "docker daemon is not running. Start Docker Desktop, or fall back: G5_DEV_WORKER=process just dev-up"

mkdir -p "$RUNTIME_BIN" "$RUNTIME_ASSETS" "$RUNTIME_MODULES"

# --- 1. Guest g5d: build the Dockerfile.g5d artifact stage and extract -------
# Dockerfile.g5d's builder stage produces the canonical Linux g5d.
# The scratch artifact target exports guest executables and catalog,
# avoiding a multi-GB builder image export/unpack. It also avoids needing the
# g5/g5d-base runtime image.
# With --g5d-from-image, reuse an already-built image (CI) instead.
IMAGE_CID=""
if [ -n "$G5D_FROM_IMAGE" ]; then
    GUEST_BUILDER_TAG="$G5D_FROM_IMAGE"
    info "extracting Linux g5d from existing image ${G5D_FROM_IMAGE} (${DOCKER_PLATFORM})"
    IMAGE_CID="$(docker create --platform "$DOCKER_PLATFORM" "$GUEST_BUILDER_TAG")"
    trap 'docker rm -f "$IMAGE_CID" >/dev/null 2>&1 || true' EXIT
    docker cp "$IMAGE_CID:/usr/local/bin/g5d" "$RUNTIME_BIN/g5d"
    # Fake-provider mode resolves the fake CLIs beside the guest g5d and fails
    # closed without them (plan P3.2); the worker image carries them beside g5d.
    for fake in fake-claude fake-codex fake-g5-tools; do
        docker cp "$IMAGE_CID:/usr/local/bin/$fake" "$RUNTIME_BIN/$fake"
    done
    docker cp "$IMAGE_CID:/opt/g5/modules/." "$RUNTIME_MODULES/"


else
    GUEST_EXPORT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/g5d-guest-artifacts.XXXXXX")"
    trap 'rm -rf "$GUEST_EXPORT_DIR"' EXIT
    info "building Linux g5d via Dockerfile.g5d artifact stage (${DOCKER_PLATFORM})"
    # Dockerfile.g5d's builder defaults to the LOCAL-ONLY g5/base-rust-warm:26.04
    # tier (docker_images [D18]/[D5]); a fresh or pruned daemon would otherwise
    # try a public-registry pull and fail before compiling anything.
    bash "$GEN_ROOT/scripts/ensure-base-images.sh" base-rust-warm
    # Dockerfile.g5d declares and consumes `G5_CARGO_PROFILE` (default `release`),
    # so that is the name that must be passed (`CARGO_PROFILE` would be silently
    # ignored). The value is used as `cargo build --profile "$G5_CARGO_PROFILE"`
    # AND to locate the output at `/build/target/<profile>/g5d`, so it must be a
    # profile whose output directory matches its name: the workspace's `g5-dev`
    # qualifies, while cargo's built-in `dev` writes to `target/debug` and the
    # copy would fail after the whole compile.
    docker build --platform "$DOCKER_PLATFORM" --target guest-runtime-artifacts \
        --build-arg "G5_CARGO_PROFILE=${G5_DEV_DOCKER_CARGO_PROFILE:-g5-dev}" \
        --output "type=local,dest=$GUEST_EXPORT_DIR" \
        -f "$GEN_ROOT/infra/docker/Dockerfile.g5d" "$GEN_ROOT"
    install -m 0755 "$GUEST_EXPORT_DIR/bin/g5d" "$RUNTIME_BIN/g5d"
    # Fake-provider mode resolves the fake CLIs beside the guest g5d and fails
    # closed without them (plan P3.2); the artifact stage exports them too.
    for fake in fake-claude fake-codex fake-g5-tools; do
        install -m 0755 "$GUEST_EXPORT_DIR/bin/$fake" "$RUNTIME_BIN/$fake"
    done
    install -m 0644 "$GUEST_EXPORT_DIR/modules/catalog.json" "$RUNTIME_MODULES/catalog.json"
    install -m 0644 "$GUEST_EXPORT_DIR/modules/catalog.sha256" "$RUNTIME_MODULES/catalog.sha256"
    rm -rf "$GUEST_EXPORT_DIR"
    trap - EXIT
fi

chmod 0755 "$RUNTIME_BIN/g5d"
info "guest g5d staged ($(du -h "$RUNTIME_BIN/g5d" | cut -f1))"
[ -s "$RUNTIME_MODULES/catalog.json" ] && [ -s "$RUNTIME_MODULES/catalog.sha256" ] \
    || fail "module catalog was not staged under $RUNTIME_MODULES"
info "module catalog staged (pin $(tr -d '[:space:]' < "$RUNTIME_MODULES/catalog.sha256" | cut -c1-12)…)"

# --- 2. Provided harness -----------------------------------------------------
# Every workspace agent reads the harness: .agents/AGENTS.md, the skills
# under .agents/skills and the ordinary guidance under .agents/guidance. A public
# self-hosted bundle deliberately contains no gated source or g5d binary, so
# release installs extract the harness from the immutable worker image beside
# the g5d binary.
HARNESS="$RUNTIME_ASSETS/harness"
rm -rf "${HARNESS:?}"
mkdir -p "$HARNESS/.agents"
if [ -n "$IMAGE_CID" ]; then
    docker cp "$IMAGE_CID:/usr/local/assets/harness/." "$HARNESS"
    docker rm -f "$IMAGE_CID" >/dev/null 2>&1 || true
    IMAGE_CID=""
    trap - EXIT
else
    cp "$GEN_ROOT/.agents/AGENTS.md" "$HARNESS/AGENTS.md"
    cp -R "$GEN_ROOT/.agents/skills" "$HARNESS/.agents/skills"
    cp -R "$GEN_ROOT/.agents/guidance" "$HARNESS/.agents/guidance"
fi

# --- 3. Harness guard ---------------------------------------------------------
# agent.turn refuses every turn without a complete harness; fail here instead.
[ -s "$HARNESS/AGENTS.md" ] || fail "harness not staged under $HARNESS"
for skill in "$HARNESS"/.agents/skills/*/; do
    [ -s "${skill}SKILL.md" ] || fail "harness skill incomplete: $skill"
done
info "harness staged"

if [ "$ONLY_G5D" = "1" ]; then
    info "--only-g5d: skipping node/provider CLI staging (re-using existing artifacts)"
else
# --- 4. node + python3 + claude + codex + agent shell CLIs in Linux ----------
# The container does the download/verify/extract so every artifact is the
# correct Linux ELF; results land in $RUNTIME_BIN via the bind mount.
# Every helper bind-mounted into the staging containers must be a regular
# file here: Docker silently creates an empty directory for a missing bind
# mount source, so a helper absent from the source tree or the self-hosted
# bundle would turn its gate into a no-op instead of a failure.
for helper in \
    scripts/apt-install-with-retry.sh \
    scripts/check-claude-cli-flags.sh \
    scripts/check-codex-cli-flags.sh \
    infra/packer/g5-worker/scripts/57-install-test-tools.sh \
    infra/packer/g5-worker/scripts/lib/download-with-retry.sh; do
    [ -f "$GEN_ROOT/$helper" ] \
        || fail "$helper missing from the source tree (or the self-hosted bundle); refusing to stage a runtime without it"
done
CLAUDE_PIN="${CLAUDE_CODE_VERSION:-}"
CODEX_PIN=""
if [ -n "${CODEX_VERSION:-}" ]; then
    CODEX_PIN="@${CODEX_VERSION}"
fi
info "staging self-contained node v${NODE_VERSION} (${NODE_ARCH}) + npm/pnpm@${PNPM_VERSION} + claude${CLAUDE_PIN:+@$CLAUDE_PIN} + codex${CODEX_PIN} + gh@${GH_CLI_VERSION} + gcloud@${GCLOUD_CLI_VERSION} + kubectl@${KUBECTL_VERSION} in ${DOCKER_PLATFORM} container"
docker run --rm --platform "$DOCKER_PLATFORM" \
    --user 0:0 \
    -e NODE_VERSION="$NODE_VERSION" \
    -e NODE_ARCH="$NODE_ARCH" \
    -e NODE_SHA256="${NODE_SHA256:-}" \
    -e PNPM_VERSION="$PNPM_VERSION" \
    -e CLAUDE_PIN="$CLAUDE_PIN" \
    -e CODEX_PIN="$CODEX_PIN" \
    -e GH_CLI_VERSION="$GH_CLI_VERSION" \
    -e GH_ARCH="$GH_ARCH" \
    -e GH_CLI_SHA256="${GH_CLI_SHA256:-}" \
    -e GCLOUD_CLI_VERSION="$GCLOUD_CLI_VERSION" \
    -e GCLOUD_ARCH="$GCLOUD_ARCH" \
    -e GCLOUD_CLI_SHA256="${GCLOUD_CLI_SHA256:-}" \
    -e KUBECTL_VERSION="$KUBECTL_VERSION" \
    -e KUBECTL_ARCH="$KUBECTL_ARCH" \
    -e KUBECTL_SHA256="${KUBECTL_SHA256:-}" \
    -e G5_DOWNLOAD_RETRY_ATTEMPTS="${G5_DOWNLOAD_RETRY_ATTEMPTS:-8}" \
    -e G5_DOWNLOAD_RETRY_DELAY_SECONDS="${G5_DOWNLOAD_RETRY_DELAY_SECONDS:-2}" \
    -e G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS="${G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS:-30}" \
    -e G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS="${G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS:-10}" \
    -e G5_DOWNLOAD_MAX_TIME_SECONDS="${G5_DOWNLOAD_MAX_TIME_SECONDS:-300}" \
    -e APT_PACKAGES_PREINSTALLED="$RUNTIME_STAGING_PACKAGES_PREINSTALLED" \
    -v "$RUNTIME_BIN":/stage \
    -v "$GEN_ROOT/scripts/apt-install-with-retry.sh":/apt-install-with-retry.sh:ro \
    `# Claude CLI flag contract checker (docs/lean-provider-cli-contract.md);` \
    `# the packer build runs the same script via a file provisioner.` \
    -v "$GEN_ROOT/scripts/check-claude-cli-flags.sh":/check-claude-cli-flags.sh:ro \
    -v "$GEN_ROOT/scripts/check-codex-cli-flags.sh":/check-codex-cli-flags.sh:ro \
    "$RUNTIME_STAGING_IMAGE" \
    bash -c '
        set -euo pipefail
        export DEBIAN_FRONTEND=noninteractive
        /apt-install-with-retry.sh curl ca-certificates xz-utils jq python3

        download_http_status_retryable() {
            case "$1" in
                408|429|5??) return 0 ;;
                *) return 1 ;;
            esac
        }

        download_curl_status_retryable() {
            local status="$1"
            local http_status="${2:-000}"
            case "$status" in
                5|6|7|18|28|35|52|55|56|92) return 0 ;;
                22) download_http_status_retryable "$http_status" ;;
                *) return 1 ;;
            esac
        }

        download_require_positive_integer() {
            local name="$1"
            local value="$2"
            case "$value" in
                ""|*[!0-9]*|0)
                    echo "FATAL: ${name} must be a positive integer (got: ${value:-<empty>})" >&2
                    return 2
                    ;;
            esac
        }

        download_with_retry() {
            local label="$1"
            local url="$2"
            local output="$3"
            local attempts="${G5_DOWNLOAD_RETRY_ATTEMPTS:-8}"
            local delay="${G5_DOWNLOAD_RETRY_DELAY_SECONDS:-2}"
            local max_delay="${G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS:-30}"
            local connect_timeout="${G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS:-10}"
            local max_time="${G5_DOWNLOAD_MAX_TIME_SECONDS:-300}"
            local attempt=1
            local status=0
            local http_status=""
            local tmp="${output}.tmp"

            download_require_positive_integer G5_DOWNLOAD_RETRY_ATTEMPTS "$attempts" || return 2
            download_require_positive_integer G5_DOWNLOAD_RETRY_DELAY_SECONDS "$delay" || return 2
            download_require_positive_integer G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS "$max_delay" || return 2
            download_require_positive_integer G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS "$connect_timeout" || return 2
            download_require_positive_integer G5_DOWNLOAD_MAX_TIME_SECONDS "$max_time" || return 2

            rm -f "$tmp"
            while [ "$attempt" -le "$attempts" ]; do
                echo "[stage-runtime] downloading ${label} attempt ${attempt}/${attempts}: ${url}"
                if http_status="$(curl -fsSL --connect-timeout "$connect_timeout" --max-time "$max_time" -w "%{http_code}" "$url" -o "$tmp")"; then
                    mv "$tmp" "$output"
                    return 0
                else
                    status=$?
                fi
                rm -f "$tmp"
                if ! download_curl_status_retryable "$status" "${http_status:-000}" || [ "$attempt" -eq "$attempts" ]; then
                    echo "FATAL: failed to download ${label} from ${url} (curl_exit=${status}, http=${http_status:-000}, attempt=${attempt}/${attempts})" >&2
                    return "$status"
                fi
                echo "[stage-runtime] ${label} transient download failure (curl_exit=${status}, http=${http_status:-000}); retrying in ${delay}s" >&2
                sleep "$delay"
                if [ "$delay" -lt "$max_delay" ]; then
                    delay=$((delay * 2))
                    [ "$delay" -le "$max_delay" ] || delay="$max_delay"
                fi
                attempt=$((attempt + 1))
            done
        }

        NODE_DIRNAME="node-v${NODE_VERSION}-${NODE_ARCH}"
        NODE_TARBALL="${NODE_DIRNAME}.tar.xz"
        NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/${NODE_TARBALL}"
        DL="$(mktemp -d)"
        download_with_retry "node ${NODE_VERSION}" "$NODE_URL" "$DL/$NODE_TARBALL"

        # Supply-chain pin: NODE_SHA256 is the hard pin; otherwise verify
        # against the official SHASUMS256.txt (same policy as the Packer image).
        ACTUAL_SHA="$(sha256sum "$DL/$NODE_TARBALL" | awk "{print \$1}")"
        if [ -n "${NODE_SHA256:-}" ]; then
            [ "$ACTUAL_SHA" = "$NODE_SHA256" ] || { echo "FATAL: node sha256 mismatch (pinned). expected=$NODE_SHA256 actual=$ACTUAL_SHA" >&2; exit 1; }
        else
            download_with_retry "node SHASUMS256.txt" "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" "$DL/SHASUMS256.txt"
            EXPECTED_SHA="$(awk -v f="$NODE_TARBALL" "\$2 == f {print \$1}" "$DL/SHASUMS256.txt")"
            [ -n "$EXPECTED_SHA" ] || { echo "FATAL: $NODE_TARBALL not in official SHASUMS256.txt" >&2; exit 1; }
            [ "$ACTUAL_SHA" = "$EXPECTED_SHA" ] || { echo "FATAL: node sha256 mismatch vs manifest. expected=$EXPECTED_SHA actual=$ACTUAL_SHA" >&2; exit 1; }
            echo "[stage-runtime] node sha256 OK: $ACTUAL_SHA (HINT: export NODE_SHA256=$ACTUAL_SHA for a hard pin)"
        fi

        tar -xJf "$DL/$NODE_TARBALL" -C "$DL"
        NODE_EXTRACT="$DL/$NODE_DIRNAME"
        [ -x "$NODE_EXTRACT/bin/node" ] || { echo "FATAL: tarball missing bin/node" >&2; exit 1; }
        install -m 0755 "$NODE_EXTRACT/bin/node" /stage/node
        rm -rf /stage/nodejs
        install -d -m 0755 /stage/nodejs/lib
        cp -a "$NODE_EXTRACT/lib/node_modules" /stage/nodejs/lib/
        "$NODE_EXTRACT/bin/node" "$NODE_EXTRACT/lib/node_modules/npm/bin/npm-cli.js" \
            install --global --prefix /stage/nodejs --no-audit --no-fund "pnpm@${PNPM_VERSION}" >/dev/null
        chmod -R a+rX /stage/nodejs
        cat > /stage/npm <<'\''EOF'\''
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
if [ -z "${npm_config_cache:-}" ]; then
    if [ -n "${G5_NPM_CACHE:-}" ]; then
        export npm_config_cache="$G5_NPM_CACHE"
    else
        export npm_config_cache="${HOME:-/tmp}/.npm"
    fi
fi
mkdir -p "$npm_config_cache" 2>/dev/null || true
exec "$NODE" "$HERE/nodejs/lib/node_modules/npm/bin/npm-cli.js" "$@"
EOF
        cat > /stage/npx <<'\''EOF'\''
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
if [ -z "${npm_config_cache:-}" ]; then
    if [ -n "${G5_NPM_CACHE:-}" ]; then
        export npm_config_cache="$G5_NPM_CACHE"
    else
        export npm_config_cache="${HOME:-/tmp}/.npm"
    fi
fi
mkdir -p "$npm_config_cache" 2>/dev/null || true
exec "$NODE" "$HERE/nodejs/lib/node_modules/npm/bin/npx-cli.js" "$@"
EOF
        cat > /stage/corepack <<'\''EOF'\''
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
if [ -z "${COREPACK_HOME:-}" ]; then
    if [ -n "${G5_NPM_CACHE:-}" ]; then
        export COREPACK_HOME="$G5_NPM_CACHE/corepack"
    else
        export COREPACK_HOME="${HOME:-/tmp}/.cache/corepack"
    fi
fi
mkdir -p "$COREPACK_HOME" 2>/dev/null || true
exec "$NODE" "$HERE/nodejs/lib/node_modules/corepack/dist/corepack.js" "$@"
EOF
        cat > /stage/pnpm <<'\''EOF'\''
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
exec "$NODE" "$HERE/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs" "$@"
EOF
        chmod 0755 /stage/npm /stage/npx /stage/corepack /stage/pnpm

        # Regression guard: the self-contained build must not NEED libnode.so.
        if ldd /stage/node 2>/dev/null | grep -q "libnode\.so"; then
            echo "FATAL: staged node depends on libnode.so — not the self-contained build" >&2
            exit 1
        fi

        # Co-locate node'\''s application-level shared-lib closure (libstdc++
        # etc.), EXCLUDING the glibc core + loader (provided by the guest
        # base_image) — same exclusion list as 40-install-claude.sh.
        staged=0
        while read -r lib; do
            [ -n "$lib" ] && [ -f "$lib" ] || continue
            case "$(basename "$lib")" in
                ld-linux*|ld.so*|libc.so.*|libc-*.so|libm.so.*|libm-*.so|libdl.so.*|libpthread.so.*|librt.so.*|libresolv.so.*|libgcc_s.so.*)
                    continue ;;
            esac
            install -m 0644 "$lib" /stage/
            staged=$((staged + 1))
        done < <(ldd /stage/node | awk "{ for (i=1;i<=NF;i++) if (\$i ~ /^\//) print \$i }")
        echo "[stage-runtime] staged $staged node closure lib(s)"
        /stage/node --version | sed "s/^v//" > /stage/node.version
        /stage/npm --version > /stage/npm.version
        /stage/npx --version > /stage/npx.version
        /stage/pnpm --version > /stage/pnpm.version
        /stage/corepack --version > /stage/corepack.version
        echo "[stage-runtime] staged npm $(cat /stage/npm.version), npx $(cat /stage/npx.version), pnpm $(cat /stage/pnpm.version), corepack $(cat /stage/corepack.version)"

        # gcloud runs on Python inside the guest. The customer base_image only
        # receives /opt/g5/bin, so stage a relocatable Python runtime next to the
        # other mounted binaries instead of depending on the base image.
        PYVER="$(python3 - <<'\''PY'\''
import sys
print(f"{sys.version_info.major}.{sys.version_info.minor}")
PY
)"
        PY_BIN_REAL="$(readlink -f "$(command -v python3)")"
        PY_RUNTIME="/stage/python3-runtime"
        rm -rf "$PY_RUNTIME"
        install -d -m 0755 "$PY_RUNTIME/bin" "$PY_RUNTIME/lib" "$PY_RUNTIME/lib/native"
        install -m 0755 "$PY_BIN_REAL" "$PY_RUNTIME/bin/python${PYVER}"
        cp -a "/usr/lib/python${PYVER}" "$PY_RUNTIME/lib/"
        py_libs=0
        stage_python_libs() {
            subject="$1"
            while read -r lib; do
                [ -n "$lib" ] && [ -f "$lib" ] || continue
                case "$(basename "$lib")" in
                    ld-linux*|ld.so*|libc.so.*|libc-*.so|libm.so.*|libm-*.so|libdl.so.*|libpthread.so.*|librt.so.*)
                        continue ;;
                esac
                if [ ! -e "$PY_RUNTIME/lib/native/$(basename "$lib")" ]; then
                    install -m 0644 "$lib" "$PY_RUNTIME/lib/native/"
                    py_libs=$((py_libs + 1))
                fi
            done < <(ldd "$subject" | awk "{ for (i=1;i<=NF;i++) if (\$i ~ /^\//) print \$i }")
        }
        stage_python_libs "$PY_BIN_REAL"
        SQLITE_EXT="$(find "$PY_RUNTIME/lib/python${PYVER}/lib-dynload" -maxdepth 1 -name "_sqlite3*.so" -print -quit)"
        if [ -z "$SQLITE_EXT" ]; then
            echo "FATAL: staged python runtime missing _sqlite3 extension; gcloud requires sqlite3" >&2
            exit 1
        fi
        stage_python_libs "$SQLITE_EXT"
        cat > /stage/python3 <<EOF
#!/bin/sh
set -eu
HERE="\$(CDPATH= cd -- "\$(dirname -- "\$0")" && pwd)"
export PYTHONHOME="\$HERE/python3-runtime"
if [ -n "\${LD_LIBRARY_PATH:-}" ]; then
    export LD_LIBRARY_PATH="\$HERE/python3-runtime/lib/native:\$LD_LIBRARY_PATH"
else
    export LD_LIBRARY_PATH="\$HERE/python3-runtime/lib/native"
fi
exec "\$HERE/python3-runtime/bin/python${PYVER}" "\$@"
EOF
        chmod 0755 /stage/python3
        /stage/python3 -c "import json, pathlib, sqlite3, subprocess, sys; print(sys.version.split()[0])"
        echo "[stage-runtime] staged python ${PYVER} for gcloud (${py_libs} closure lib(s))"

        # Claude Code: npm install (using the tarball'\''s own npm), then take
        # the platform-native bin/claude.exe directly — no node wrapper.
        export PATH="$NODE_EXTRACT/bin:$PATH"
        NPM_PREFIX="$(mktemp -d)"
        npm config set prefix "$NPM_PREFIX" --global >/dev/null
        echo "[stage-runtime] npm install -g @anthropic-ai/claude-code${CLAUDE_PIN:+@$CLAUDE_PIN}"
        npm install --global --no-audit --no-fund "@anthropic-ai/claude-code${CLAUDE_PIN:+@$CLAUDE_PIN}" >/dev/null

        CC_PKG="$NPM_PREFIX/lib/node_modules/@anthropic-ai/claude-code"
        CLAUDE_NATIVE="$CC_PKG/bin/claude.exe"
        if [ ! -f "$CLAUDE_NATIVE" ]; then
            BINREL="$(jq -r ".bin.claude // empty" "$CC_PKG/package.json" 2>/dev/null || true)"
            [ -n "$BINREL" ] && CLAUDE_NATIVE="$CC_PKG/$BINREL"
        fi
        [ -f "$CLAUDE_NATIVE" ] || { echo "FATAL: claude native binary not found under $CC_PKG/bin" >&2; exit 1; }
        CLAUDE_MAGIC="$(od -An -tx1 -N4 "$CLAUDE_NATIVE" | tr -d "[:space:]")"
        [ "$CLAUDE_MAGIC" = "7f454c46" ] \
            || { echo "FATAL: $CLAUDE_NATIVE is not a Linux ELF executable (magic=$CLAUDE_MAGIC)" >&2; exit 1; }
        install -m 0755 "$CLAUDE_NATIVE" /stage/claude
        jq -r .version "$CC_PKG/package.json" > /stage/claude-code.version
        # Prove the staged binary carries the runner flag surface (same gate as
        # 40-install-claude.sh and Dockerfile.g5d); the version argument is the
        # pin, or empty when CLAUDE_CODE_VERSION was overridden to float.
        sh /check-claude-cli-flags.sh /stage/claude "$CLAUDE_PIN"
        echo "[stage-runtime] staged claude $(cat /stage/claude-code.version)"

        # Codex CLI: mirror 50-install-codex.sh. @openai/codex ships a node
        # launcher plus a platform-native optionalDependency; vendor the npm
        # prefix under /opt/g5/bin/_npm and install a wrapper that re-execs it
        # through the bundled self-contained node.
        NPM_CODEX_PREFIX="$(mktemp -d)"
        npm config set prefix "$NPM_CODEX_PREFIX" --global >/dev/null
        echo "[stage-runtime] npm install -g @openai/codex${CODEX_PIN}"
        npm install --global --no-audit --no-fund "@openai/codex${CODEX_PIN}" >/dev/null
        rm -rf /stage/_npm
        cp -a "$NPM_CODEX_PREFIX" /stage/_npm
        chmod -R a+rX /stage/_npm
        CODEX_PKG="/stage/_npm/lib/node_modules/@openai/codex"
        [ -f "$CODEX_PKG/package.json" ] || { echo "FATAL: codex package.json missing after vendoring: $CODEX_PKG/package.json" >&2; exit 1; }
        CODEX_PLATFORM_PKG="$(/stage/node -e "
const path = require(\"path\");
const pkgPath = process.argv[1];
const key = \"@openai/codex-\" + process.platform + \"-\" + process.arch;
const pkg = require(path.join(pkgPath, \"package.json\"));
if (!pkg.optionalDependencies || !pkg.optionalDependencies[key]) {
  console.error(\"FATAL: @openai/codex does not declare optionalDependency \" + key);
  process.exit(1);
}
try {
  const resolved = require.resolve(key + \"/package.json\", { paths: [pkgPath] });
  console.log(key + \" \" + resolved);
} catch (err) {
  console.error(\"FATAL: @openai/codex optionalDependency \" + key + \" missing from vendored npm closure under \" + pkgPath + \". npm install may have omitted optional dependencies; do not advertise Codex without this native binary.\");
  process.exit(1);
}
" "$CODEX_PKG")"
        echo "[stage-runtime] verified codex optional dependency ${CODEX_PLATFORM_PKG}"
        if [ ! -x /stage/_npm/bin/codex ]; then
            CANDIDATE="$(find /stage/_npm -name codex -type f -executable | head -1)"
            [ -n "$CANDIDATE" ] || { echo "FATAL: codex bin not found under /stage/_npm after vendoring" >&2; exit 1; }
            REL="$(realpath --relative-to=/stage/_npm/bin "$CANDIDATE")"
            ln -s "$REL" /stage/_npm/bin/codex
        fi
        cat > /stage/codex <<'\''EOF'\''
#!/usr/bin/env bash
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
NODE="${G5_RUNTIME_NODE:-$HERE/node}"
exec "$NODE" "$HERE/_npm/bin/codex" "$@"
EOF
        chmod 0755 /stage/codex
        sh /check-codex-cli-flags.sh /stage/codex "${CODEX_PIN#@}"
        /stage/codex --version > /stage/codex.version
        echo "[stage-runtime] staged codex $(cat /stage/codex.version)"

        GH_TARBALL="gh_${GH_CLI_VERSION}_linux_${GH_ARCH}.tar.gz"
        GH_URL="https://github.com/cli/cli/releases/download/v${GH_CLI_VERSION}/${GH_TARBALL}"
        # Pinned-artifact cache baked into the staging image (Dockerfile.g5d-base
        # Layer 8.5, G5Labs/G5#1811); absent cache falls back to the retried download.
        DL_CACHE="${G5_DL_CACHE:-/opt/g5/dl-cache}"
        if [ -f "$DL_CACHE/$GH_TARBALL" ]; then
            echo "[stage-runtime] using cached $GH_TARBALL from $DL_CACHE"
            cp "$DL_CACHE/$GH_TARBALL" "$DL/$GH_TARBALL"
        else
            download_with_retry "gh ${GH_CLI_VERSION}" "$GH_URL" "$DL/$GH_TARBALL"
        fi
        ACTUAL_GH_SHA="$(sha256sum "$DL/$GH_TARBALL" | awk "{print \$1}")"
        if [ -n "${GH_CLI_SHA256:-}" ]; then
            [ "$ACTUAL_GH_SHA" = "$GH_CLI_SHA256" ] || { echo "FATAL: GitHub CLI sha256 mismatch (pinned). expected=$GH_CLI_SHA256 actual=$ACTUAL_GH_SHA" >&2; exit 1; }
        else
            if [ -f "$DL_CACHE/gh_${GH_CLI_VERSION}_checksums.txt" ]; then
                echo "[stage-runtime] using cached gh_${GH_CLI_VERSION}_checksums.txt from $DL_CACHE"
                cp "$DL_CACHE/gh_${GH_CLI_VERSION}_checksums.txt" "$DL/gh-checksums.txt"
            else
                download_with_retry "gh checksums" "https://github.com/cli/cli/releases/download/v${GH_CLI_VERSION}/gh_${GH_CLI_VERSION}_checksums.txt" "$DL/gh-checksums.txt"
            fi
            EXPECTED_GH_SHA="$(awk -v f="$GH_TARBALL" "\$2 == f {print \$1}" "$DL/gh-checksums.txt")"
            [ -n "$EXPECTED_GH_SHA" ] || { echo "FATAL: $GH_TARBALL not in GitHub CLI release checksums" >&2; exit 1; }
            [ "$ACTUAL_GH_SHA" = "$EXPECTED_GH_SHA" ] || { echo "FATAL: GitHub CLI sha256 mismatch vs release checksums. expected=$EXPECTED_GH_SHA actual=$ACTUAL_GH_SHA" >&2; exit 1; }
            echo "[stage-runtime] gh sha256 OK: $ACTUAL_GH_SHA (HINT: export GH_CLI_SHA256=$ACTUAL_GH_SHA for a hard pin)"
        fi
        tar -xzf "$DL/$GH_TARBALL" -C "$DL"
        GH_EXTRACT="$DL/gh_${GH_CLI_VERSION}_linux_${GH_ARCH}"
        [ -x "$GH_EXTRACT/bin/gh" ] || { echo "FATAL: GitHub CLI tarball missing bin/gh" >&2; exit 1; }
        install -m 0755 "$GH_EXTRACT/bin/gh" /stage/gh
        /stage/gh --version | awk "NR == 1 {print \$3}" > /stage/gh.version
        echo "[stage-runtime] staged gh $(cat /stage/gh.version)"

        GCLOUD_TARBALL="google-cloud-cli-${GCLOUD_CLI_VERSION}-linux-${GCLOUD_ARCH}.tar.gz"
        GCLOUD_URL="https://storage.googleapis.com/cloud-sdk-release/${GCLOUD_TARBALL}"
        download_with_retry "gcloud ${GCLOUD_CLI_VERSION}" "$GCLOUD_URL" "$DL/$GCLOUD_TARBALL"
        ACTUAL_GCLOUD_SHA="$(sha256sum "$DL/$GCLOUD_TARBALL" | awk "{print \$1}")"
        if [ -n "${GCLOUD_CLI_SHA256:-}" ]; then
            [ "$ACTUAL_GCLOUD_SHA" = "$GCLOUD_CLI_SHA256" ] || { echo "FATAL: gcloud sha256 mismatch (pinned). expected=$GCLOUD_CLI_SHA256 actual=$ACTUAL_GCLOUD_SHA" >&2; exit 1; }
        else
            echo "[stage-runtime] gcloud sha256: $ACTUAL_GCLOUD_SHA (HINT: export GCLOUD_CLI_SHA256=$ACTUAL_GCLOUD_SHA for a hard pin)"
        fi
        printf "%s\n" "$ACTUAL_GCLOUD_SHA" > /stage/gcloud.sha256
        tar -xzf "$DL/$GCLOUD_TARBALL" -C "$DL"
        [ -x "$DL/google-cloud-sdk/bin/gcloud" ] || { echo "FATAL: Google Cloud CLI archive missing bin/gcloud" >&2; exit 1; }
        rm -rf /stage/google-cloud-sdk
        cp -a "$DL/google-cloud-sdk" /stage/google-cloud-sdk
        chmod -R a+rX /stage/google-cloud-sdk
        cat > /stage/gcloud <<'\''EOF'\''
#!/bin/sh
set -eu
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK="${CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK:-1}"
export CLOUDSDK_PYTHON="$HERE/python3"
exec "$HERE/google-cloud-sdk/bin/gcloud" "$@"
EOF
        chmod 0755 /stage/gcloud
        CLOUDSDK_CORE_DISABLE_PROMPTS=1 CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK=1 \
            /stage/gcloud components install gke-gcloud-auth-plugin --quiet
        chmod -R a+rX /stage/google-cloud-sdk
        [ -x /stage/google-cloud-sdk/bin/gke-gcloud-auth-plugin ] || { echo "FATAL: gcloud component install did not produce bin/gke-gcloud-auth-plugin" >&2; exit 1; }
        cat > /stage/gke-gcloud-auth-plugin <<'\''EOF'\''
#!/bin/sh
set -eu
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export CLOUDSDK_PYTHON="$HERE/python3"
exec "$HERE/google-cloud-sdk/bin/gke-gcloud-auth-plugin" "$@"
EOF
        chmod 0755 /stage/gke-gcloud-auth-plugin
        /stage/gcloud --version | awk "/^Google Cloud SDK / {print \$4; exit}" > /stage/gcloud.version
        /stage/gke-gcloud-auth-plugin --version | awk "NR == 1 {print; exit}" > /stage/gke-gcloud-auth-plugin.version
        echo "[stage-runtime] staged gcloud $(cat /stage/gcloud.version)"

        KUBECTL_URL="https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/${KUBECTL_ARCH}/kubectl"
        download_with_retry "kubectl ${KUBECTL_VERSION}" "$KUBECTL_URL" "$DL/kubectl"
        ACTUAL_KUBECTL_SHA="$(sha256sum "$DL/kubectl" | awk "{print \$1}")"
        if [ -n "${KUBECTL_SHA256:-}" ]; then
            [ "$ACTUAL_KUBECTL_SHA" = "$KUBECTL_SHA256" ] || { echo "FATAL: kubectl sha256 mismatch (pinned). expected=$KUBECTL_SHA256 actual=$ACTUAL_KUBECTL_SHA" >&2; exit 1; }
        else
            download_with_retry "kubectl checksum" "${KUBECTL_URL}.sha256" "$DL/kubectl.sha256"
            EXPECTED_KUBECTL_SHA="$(awk "{print \$1}" "$DL/kubectl.sha256")"
            [ -n "$EXPECTED_KUBECTL_SHA" ] || { echo "FATAL: kubectl checksum file was empty" >&2; exit 1; }
            [ "$ACTUAL_KUBECTL_SHA" = "$EXPECTED_KUBECTL_SHA" ] || { echo "FATAL: kubectl sha256 mismatch vs release checksum. expected=$EXPECTED_KUBECTL_SHA actual=$ACTUAL_KUBECTL_SHA" >&2; exit 1; }
            echo "[stage-runtime] kubectl sha256 OK: $ACTUAL_KUBECTL_SHA (HINT: export KUBECTL_SHA256=$ACTUAL_KUBECTL_SHA for a hard pin)"
        fi
        printf "%s\n" "$ACTUAL_KUBECTL_SHA" > /stage/kubectl.sha256
        install -m 0755 "$DL/kubectl" /stage/kubectl
        /stage/kubectl version --client=true | awk "match(\$0, /v[0-9]+\\.[0-9]+\\.[0-9]+/) {print substr(\$0, RSTART + 1, RLENGTH - 1); exit}" > /stage/kubectl.version
        echo "[stage-runtime] staged kubectl $(cat /stage/kubectl.version)"

        # The agent runner clears ambient env before spawning sidecars, so Codex
        # needs a deterministic trust store inside the mounted runtime closure.
        install -m 0644 /etc/ssl/certs/ca-certificates.crt /stage/ca-certificates.crt
        [ -s /stage/ca-certificates.crt ] || { echo "FATAL: staged CA bundle is empty" >&2; exit 1; }
    '
# claude-code.version belongs next to the runtime dir, not inside the mount.
mv "$RUNTIME_BIN/claude-code.version" "$OUT_DIR/runtime/claude-code.version"

info "staging Rust test tools (Rust ${RUST_TOOLCHAIN}, just ${JUST_VERSION}, cargo-nextest ${CARGO_NEXTEST_VERSION}) in ${DOCKER_PLATFORM} container"
docker run --rm --platform "$DOCKER_PLATFORM" \
    --user 0:0 \
    -e RUNTIME_BIN=/stage \
    -e RUST_TOOLCHAIN="$RUST_TOOLCHAIN" \
    -e RUSTUP_VERSION="$RUSTUP_VERSION" \
    -e RUSTUP_INIT_SHA256="${RUSTUP_INIT_SHA256:-}" \
    -e JUST_VERSION="$JUST_VERSION" \
    -e JUST_SHA256="${JUST_SHA256:-}" \
    -e CARGO_NEXTEST_VERSION="$CARGO_NEXTEST_VERSION" \
    -e CARGO_NEXTEST_SHA256="${CARGO_NEXTEST_SHA256:-}" \
    -e G5_DOWNLOAD_RETRY_ATTEMPTS="${G5_DOWNLOAD_RETRY_ATTEMPTS:-8}" \
    -e G5_DOWNLOAD_RETRY_DELAY_SECONDS="${G5_DOWNLOAD_RETRY_DELAY_SECONDS:-2}" \
    -e G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS="${G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS:-30}" \
    -e G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS="${G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS:-10}" \
    -e G5_DOWNLOAD_MAX_TIME_SECONDS="${G5_DOWNLOAD_MAX_TIME_SECONDS:-300}" \
    -e APT_PACKAGES_PREINSTALLED="$RUNTIME_STAGING_PACKAGES_PREINSTALLED" \
    -e G5_DOWNLOAD_LIB=/g5-download-with-retry.sh \
    -v "$RUNTIME_BIN":/stage \
    -v "$GEN_ROOT/infra/packer/g5-worker/scripts/57-install-test-tools.sh":/install-test-tools.sh:ro \
    -v "$GEN_ROOT/scripts/apt-install-with-retry.sh":/apt-install-with-retry.sh:ro \
    `# 57-install-test-tools.sh sources this helper. It has TWO consumers: the` \
    `# packer build uploads it via a file provisioner, and this container must` \
    `# mount it — otherwise the script dies with "No such file or directory".` \
    -v "$GEN_ROOT/infra/packer/g5-worker/scripts/lib/download-with-retry.sh":/g5-download-with-retry.sh:ro \
    "$RUNTIME_STAGING_IMAGE" \
    bash -c '
        set -euo pipefail
        export DEBIAN_FRONTEND=noninteractive
        /apt-install-with-retry.sh curl ca-certificates
        bash /install-test-tools.sh
    '
fi  # ONLY_G5D

codex_ca_bundle_staging

# --- 5. Layout guards (host-side, cheap) --------------------------------------
for asset in g5d node claude codex gh gcloud gke-gcloud-auth-plugin kubectl python3 npm npx corepack pnpm cargo rustc cargo-nextest just; do
    [ -e "$RUNTIME_BIN/$asset" ] || fail "runtime dir incomplete: missing $RUNTIME_BIN/$asset"
done
[ -x "$RUNTIME_BIN/google-cloud-sdk/bin/gcloud" ] \
    || fail "runtime dir incomplete: missing $RUNTIME_BIN/google-cloud-sdk/bin/gcloud (Google Cloud SDK closure)"
[ -x "$RUNTIME_BIN/google-cloud-sdk/bin/gke-gcloud-auth-plugin" ] \
    || fail "runtime dir incomplete: missing $RUNTIME_BIN/google-cloud-sdk/bin/gke-gcloud-auth-plugin (GKE auth plugin closure)"
[ -s "$RUNTIME_BIN/ca-certificates.crt" ] \
    || fail "runtime dir incomplete: missing $RUNTIME_BIN/ca-certificates.crt (TLS trust store for in-VM Codex)"
[ -e "$RUNTIME_BIN/_npm/bin/codex" ] \
    || fail "runtime dir incomplete: missing $RUNTIME_BIN/_npm/bin/codex (Codex npm launcher closure)"
[ -f "$RUNTIME_BIN/nodejs/lib/node_modules/npm/bin/npm-cli.js" ] \
    || fail "runtime dir incomplete: missing $RUNTIME_BIN/nodejs/lib/node_modules/npm/bin/npm-cli.js (npm launcher closure)"
[ -f "$RUNTIME_BIN/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs" ] \
    || fail "runtime dir incomplete: missing $RUNTIME_BIN/nodejs/lib/node_modules/pnpm/bin/pnpm.cjs (vendored pnpm launcher closure)"
[ -d "$RUNTIME_BIN/rust/rustup/toolchains" ] \
    || fail "runtime dir incomplete: missing $RUNTIME_BIN/rust/rustup/toolchains (Rust toolchain closure)"
if ls "$RUNTIME_BIN"/libnode.so* >/dev/null 2>&1; then
    fail "libnode.so* present in $RUNTIME_BIN — the distro node shim was staged; the host will refuse to boot"
fi

# --- 6. Smoke gate (A2.5 parity): exact in-VM resolution under ubuntu:24.04 ---
# Mount the staged dir RO at /opt/g5/bin (the guest mount point) and prove the
# provider runtimes execute from it with LD_LIBRARY_PATH=/opt/g5/bin — the same
# resolution the in-VM agent-runner uses. Fail closed on any miss.
a25_runtime_smoke_gate

# Assemble the same independently read-only runtime disk used by worker images.
# This runs once per staged runtime, never during a workspace Start.
# The disk also carries this machine's Cloud Hypervisor platform for inner
# hosts (stage-runtime-platform.sh). The files are COPIED under $OUT_DIR
# (platform-input/, beside runtime/) and that directory is bind-mounted, never
# the machine's own paths: on a Docker-out-of-Docker caller (the CI builder,
# whose daemon launches sibling containers) a bind source names the daemon
# host's filesystem, where /opt/g5-host does not exist and a missing file
# source becomes an empty directory that breaks the container (Codex review);
# $OUT_DIR is already the directory the runtime mount proves shareable.
# Sources are the G5D_* overrides the caller resolved (inside a workspace VM,
# worker-host-env.sh points them at the mounted /opt/g5/platform) or the
# host's /opt/g5-host and PATH passt. All five or none: a partial platform
# fails here, before the disk build, as the stager would.
# --- platform input (begin) ---
PLATFORM_INPUT="$OUT_DIR/platform-input"
PLATFORM_MOUNTS=()
rm -rf "$PLATFORM_INPUT"
if [ "$(uname -s)" = Linux ]; then
    platform_vmm="${G5D_CLOUD_HYPERVISOR_BIN:-/opt/g5-host/bin/cloud-hypervisor}"
    platform_kernel="${G5D_GUEST_KERNEL:-/opt/g5-host/boot/vmlinux}"
    platform_initramfs="${G5D_GUEST_INITRAMFS:-/opt/g5-host/boot/initramfs.cpio}"
    platform_passt="${G5D_PASST_BIN:-$(command -v passt 2>/dev/null || echo /usr/bin/passt)}"
    platform_config="$(dirname "$platform_kernel")/kernel.config"
    platform_present=0
    for f in "$platform_vmm" "$platform_kernel" "$platform_initramfs" "$platform_config" "$platform_passt"; do
        [ -f "$f" ] && platform_present=$((platform_present + 1))
    done
    if [ "$platform_present" = 5 ]; then
        # The source's own digest list is verified where it lives (the host
        # layout lists bin/… and boot/… relative to its parent; the runtime
        # disk's /opt/g5/platform lists flat names relative to itself), then a
        # list in the host layout is regenerated for the copy, which is what
        # the stager verifies in the container (Codex review).
        platform_source_dir="$(dirname "$platform_kernel")"
        if [ -f "$platform_source_dir/SHA256SUMS" ]; then
            if [ "$(basename "$platform_source_dir")" = boot ]; then
                (cd "$platform_source_dir/.." && sha256sum --check --status boot/SHA256SUMS) \
                    || fail "the platform digest list $platform_source_dir/SHA256SUMS does not match its files"
            else
                (cd "$platform_source_dir" && sha256sum --check --status SHA256SUMS) \
                    || fail "the platform digest list $platform_source_dir/SHA256SUMS does not match its files"
            fi
        fi
        mkdir -p "$PLATFORM_INPUT/bin" "$PLATFORM_INPUT/boot"
        install -m 0755 "$platform_vmm" "$PLATFORM_INPUT/bin/cloud-hypervisor"
        install -m 0644 "$platform_kernel" "$PLATFORM_INPUT/boot/vmlinux"
        install -m 0644 "$platform_initramfs" "$PLATFORM_INPUT/boot/initramfs.cpio"
        install -m 0644 "$platform_config" "$PLATFORM_INPUT/boot/kernel.config"
        install -m 0755 "$platform_passt" "$PLATFORM_INPUT/passt"
        [ -f "$platform_source_dir/versions.env" ] && install -m 0644 "$platform_source_dir/versions.env" "$PLATFORM_INPUT/boot/versions.env"
        (cd "$PLATFORM_INPUT" && sha256sum bin/cloud-hypervisor boot/vmlinux boot/initramfs.cpio boot/kernel.config > boot/SHA256SUMS)
        PLATFORM_MOUNTS=(-v "$PLATFORM_INPUT:/platform-input:ro"
            -e G5D_CLOUD_HYPERVISOR_BIN=/platform-input/bin/cloud-hypervisor
            -e G5D_GUEST_KERNEL=/platform-input/boot/vmlinux
            -e G5D_GUEST_INITRAMFS=/platform-input/boot/initramfs.cpio
            -e G5D_PASST_BIN=/platform-input/passt)
        info "platform input for inner hosts: $platform_vmm, $platform_kernel, $platform_passt"
    elif [ "$platform_present" != 0 ]; then
        fail "incomplete Cloud Hypervisor platform: need $platform_vmm $platform_kernel $platform_initramfs $platform_config $platform_passt (all five, or none for a process-only runtime)"
    fi
fi
# --- platform input (end) ---
info "assembling trusted bootstrap and runtime block disk"
docker run --rm --platform "$DOCKER_PLATFORM" \
    --user 0:0 --entrypoint /bin/bash \
    -e APT_PACKAGES_PREINSTALLED=0 \
    -v "$OUT_DIR/runtime:/runtime" \
    ${PLATFORM_MOUNTS[@]+"${PLATFORM_MOUNTS[@]}"} \
    -v "$GEN_ROOT/scripts:/g5-runtime-build/scripts:ro" \
    "${G5D_FROM_IMAGE:-ubuntu:26.04}" -eu -c '
        # ca-certificates: the bootstrap stager copies the CA bundle of this
        # container for the platform-owned Git; a bare Ubuntu image has none and
        # git only recommends it (first workspace-VM microvm preview, 2026-10-28).
        /g5-runtime-build/scripts/apt-install-with-retry.sh e2fsprogs busybox-static python3 patchelf ca-certificates
        command -v git >/dev/null || /g5-runtime-build/scripts/apt-install-with-retry.sh git
        bash /g5-runtime-build/scripts/build-runtime-disk.sh /runtime
    '
rm -rf "$PLATFORM_INPUT"

info "OK — Linux ${ARCH} runtime staged at $RUNTIME_BIN"
info "  g5d:    $(du -h "$RUNTIME_BIN/g5d" | cut -f1)"
info "  node:   v${NODE_VERSION}"
info "  npm:    $(cat "$RUNTIME_BIN/npm.version" 2>/dev/null || "$RUNTIME_BIN/npm" --version 2>/dev/null || echo '(pre-existing)')"
info "  npx:    $(cat "$RUNTIME_BIN/npx.version" 2>/dev/null || "$RUNTIME_BIN/npx" --version 2>/dev/null || echo '(pre-existing)')"
info "  pnpm:   $(cat "$RUNTIME_BIN/pnpm.version" 2>/dev/null || "$RUNTIME_BIN/pnpm" --version 2>/dev/null || echo '(pre-existing)')"
info "  python: $("$RUNTIME_BIN/python3" --version 2>/dev/null || echo '(unrunnable)')"
info "  claude: $(cat "$OUT_DIR/runtime/claude-code.version" 2>/dev/null || echo '(pre-existing)')"
info "  codex:  $(cat "$RUNTIME_BIN/codex.version" 2>/dev/null || "$RUNTIME_BIN/codex" --version 2>/dev/null || echo '(pre-existing)')"
info "  gh:     $(cat "$RUNTIME_BIN/gh.version" 2>/dev/null || "$RUNTIME_BIN/gh" --version 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  gcloud: $(cat "$RUNTIME_BIN/gcloud.version" 2>/dev/null || "$RUNTIME_BIN/gcloud" --version 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  gke-auth: $(cat "$RUNTIME_BIN/gke-gcloud-auth-plugin.version" 2>/dev/null || "$RUNTIME_BIN/gke-gcloud-auth-plugin" --version 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  kubectl: $(cat "$RUNTIME_BIN/kubectl.version" 2>/dev/null || "$RUNTIME_BIN/kubectl" version --client=true 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  rustup: $(cat "$RUNTIME_BIN/rustup.version" 2>/dev/null || echo '(pre-existing)')"
info "  cargo: $(cat "$RUNTIME_BIN/cargo.version" 2>/dev/null || "$RUNTIME_BIN/cargo" --version 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  rustc:  $(cat "$RUNTIME_BIN/rustc.version" 2>/dev/null || "$RUNTIME_BIN/rustc" --version 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  nextest: $(cat "$RUNTIME_BIN/cargo-nextest.version" 2>/dev/null || "$RUNTIME_BIN/cargo-nextest" --version 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  just:   $(cat "$RUNTIME_BIN/just.version" 2>/dev/null || "$RUNTIME_BIN/just" --version 2>/dev/null | sed -n '1p' || echo '(pre-existing)')"
info "  harness: $HARNESS"
