#!/usr/bin/env bash
# =============================================================================
# Automated sccache gating/failure-mode contract
# (test_contract/deploy_build_caching I4; docker_images D17).
#
# Hermetic docker-build tests of the sccache contract carried by
# Dockerfile.g5-api / Dockerfile.g5d, using a harness image that embeds the
# SAME gating + preflight logic (Section D asserts the real Dockerfiles still
# carry each load-bearing line AND that the structure holds invocation by
# invocation, so harness and product cannot drift silently):
#
#   A. disabled mode  — empty bucket arg ⇒ no binary installed, no wrapper
#                       exported; build succeeds.
#   B. checksum fail  — DETERMINISTIC fixture bytes (no network) against the
#                       pinned sha ⇒ sha256sum itself reports FAILED, install
#                       is skipped with WARN, unverified bytes never
#                       installed, build succeeds. A download failure cannot
#                       satisfy this case: the FAILED line only exists when
#                       checksum verification actually ran.
#   C. backend down   — bucket set but unreachable ⇒ strict preflight fails,
#                       WARN, compile runs UNWRAPPED and produces objects;
#                       build succeeds (the fail-open contract, end to end).
#   D. sync + wiring  — STRUCTURAL, not string-existence:
#                       * every `cargo` invocation in both product
#                         Dockerfiles is immediately preceded by
#                         `sccache_preflight` in its `&&` chain (removing the
#                         preflight from any single invocation fails here);
#                       * BOTH `build-api` and `build-g5d` steps in
#                         infra/ci/cloudbuild-deploy.yaml carry the conditional
#                         SCCACHE_ARGS wiring AND consume it in their own
#                         `docker build` (removing it from either step fails
#                         here);
#                       * the product pin values (version + both arch sha256)
#                         equal this script's pins exactly;
#                       * the complete arch-case lines (amd64, arm64, and the
#                         clearing warn/skip default) survive verbatim.
#
# TARGETARCH is passed EXPLICITLY (mapped from the host arch): Cloud Build's
# legacy docker builder does not populate BuildKit's automatic TARGETARCH
# (live failure 8d230b48: `WARN: unsupported TARGETARCH ''` broke case C), and
# the harness must behave identically under both builders. The product deploy
# builds run with DOCKER_BUILDKIT=1 (infra/ci/cloudbuild-deploy.yaml) where the
# automatic value exists.
#
# Needs: docker daemon; network egress (debian apt for A–C, plus the pinned
# GitHub release tarball for C only). No GCP credentials, no GCS access, no
# state. G5_GATING_SKIP_DOCKER=1 runs ONLY Section D (for fast local
# mutation-testing of the sync checks) and prints a distinct non-canonical
# summary line so a partial run can never impersonate the full contract.
# =============================================================================
set -euo pipefail

# Use BuildKit so the automatic TARGETARCH build-arg is populated — exactly as
# the real deploy builds run (infra/ci/cloudbuild-deploy.yaml sets DOCKER_BUILDKIT=1).
# Without it the legacy builder leaves TARGETARCH empty, the arch case falls
# to the unsupported-arch branch, sccache never installs, and section C fails
# (observed in the g5-ci-rust step, build 8d230b48). TARGETARCH is ALSO passed
# explicitly (host-mapped, fail-loud below) so the suite behaves identically
# even where only the legacy builder is available.
export DOCKER_BUILDKIT=1

GEN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCCACHE_VERSION="v0.17.0"
SHA_AMD64="67c4a96dd237c1f518f6b36083f270f9976d516f1e57fce891755ea782e50006"
SHA_ARM64="821a86343191aa1cbab74bd42f9e93c9a63bf85e4742945f40d3ae84193c1c77"

fail() { echo "FAIL: $*" >&2; exit 1; }
note() { echo "== $*"; }

case "$(uname -m)" in
  x86_64|amd64)  HOST_TARGETARCH=amd64 ;;
  aarch64|arm64) HOST_TARGETARCH=arm64 ;;
  *) fail "unsupported host architecture '$(uname -m)' for the gating harness" ;;
esac

workdir="$(mktemp -d)"
trap 'rm -rf "${workdir}"' EXIT

# Harness Dockerfile: real gating/install/preflight logic + assertions. The
# ONLY divergence from the product install block is the SCCACHE_TEST_FIXTURE
# branch, which substitutes deterministic local bytes for the download so the
# checksum path can be exercised without any network dependence; the checksum
# line itself is byte-identical to the product (Section D pins it there).
cat > "${workdir}/Dockerfile" <<'EOF'
FROM debian:bookworm-slim
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    curl ca-certificates gcc libc6-dev && rm -rf /var/lib/apt/lists/*

ARG SCCACHE_GCS_BUCKET=""
ARG SCCACHE_GCS_KEY_PREFIX="sccache/deploy"
ARG SCCACHE_VERSION="v0.17.0"
ARG SCCACHE_SHA256_AMD64=""
ARG SCCACHE_SHA256_ARM64=""
ARG SCCACHE_TEST_FIXTURE=""
ARG TARGETARCH
RUN if [ -n "$SCCACHE_GCS_BUCKET" ]; then \
        case "$TARGETARCH" in \
            amd64) sccache_arch=x86_64;  sccache_sha="$SCCACHE_SHA256_AMD64" ;; \
            arm64) sccache_arch=aarch64; sccache_sha="$SCCACHE_SHA256_ARM64" ;; \
            *)     sccache_arch=;        sccache_sha=; \
                   echo "WARN: unsupported TARGETARCH '$TARGETARCH'; compiles will run uncached" ;; \
        esac && \
        if [ -n "$sccache_arch" ]; then \
            sccache_dir="sccache-${SCCACHE_VERSION}-${sccache_arch}-unknown-linux-musl" && \
            if { if [ -n "$SCCACHE_TEST_FIXTURE" ]; then \
                     printf '%s' "$SCCACHE_TEST_FIXTURE" > /tmp/sccache.tar.gz; \
                 else \
                     curl -fsSL --connect-timeout 10 --max-time 120 --retry 3 --retry-connrefused \
                       -o /tmp/sccache.tar.gz \
                       "https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/${sccache_dir}.tar.gz"; \
                 fi; } \
                && echo "${sccache_sha}  /tmp/sccache.tar.gz" | sha256sum -c -; then \
                tar -xzf /tmp/sccache.tar.gz -C /tmp && \
                install -m 0755 "/tmp/${sccache_dir}/sccache" /usr/local/bin/sccache && \
                sccache --version; \
            else \
                echo "WARN: sccache download or checksum verification failed; compiles will run uncached"; \
            fi; \
            rm -rf /tmp/sccache.tar.gz "/tmp/${sccache_dir}"; \
        fi; \
    else \
        echo "sccache disabled (SCCACHE_GCS_BUCKET empty)"; \
    fi
ENV SCCACHE_GCS_BUCKET="${SCCACHE_GCS_BUCKET}"
ENV SCCACHE_GCS_KEY_PREFIX="${SCCACHE_GCS_KEY_PREFIX}"
ENV SCCACHE_GCS_RW_MODE="READ_WRITE"
ENV SCCACHE_IDLE_TIMEOUT="0"

# Per-invocation preflight (mirrors the product Dockerfiles) + two "builds".
RUN sccache_preflight() { \
        export RUSTC_WRAPPER=; \
        [ -n "$SCCACHE_GCS_BUCKET" ] || return 0; \
        command -v sccache >/dev/null 2>&1 || return 0; \
        sccache --show-stats 2>/dev/null || true; \
        sccache --stop-server >/dev/null 2>&1 || true; \
        if sccache --start-server >/dev/null 2>&1; then \
            export RUSTC_WRAPPER=sccache; \
        else \
            echo "WARN: sccache backend unavailable; compiling uncached"; \
        fi; \
    } && \
    echo 'int main(){return 0;}' > /tmp/t.c && \
    sccache_preflight && \
    ${RUSTC_WRAPPER} gcc -c /tmp/t.c -o /tmp/t1.o && \
    sccache_preflight && \
    ${RUSTC_WRAPPER} gcc -c /tmp/t.c -o /tmp/t2.o && \
    [ -f /tmp/t1.o ] && [ -f /tmp/t2.o ] && \
    if [ -n "$SCCACHE_GCS_BUCKET" ] && command -v sccache >/dev/null 2>&1; then \
        [ -z "$RUSTC_WRAPPER" ] && echo "HARNESS: degraded-uncached-build-succeeded"; \
        true; \
    elif [ -n "$SCCACHE_GCS_BUCKET" ]; then \
        echo "HARNESS: no-binary-uncached-build-succeeded"; \
    else \
        [ -z "$RUSTC_WRAPPER" ] || { echo "HARNESS: wrapper-leaked"; exit 1; }; \
        ! command -v sccache >/dev/null 2>&1 || { echo "HARNESS: binary-present-when-disabled"; exit 1; }; \
        echo "HARNESS: disabled-clean"; \
    fi
EOF

# build() passes ONLY the caller's args — no SHA injection. Each section
# supplies exactly the SHAs it needs, so there are never duplicate
# --build-arg values whose resolution is docker-dedup-order-dependent (the
# earlier injection silently overrode section B's intentionally-bad SHAs,
# making the checksum-failure test pass or fail for the wrong reason).
# --no-cache is load-bearing: these assertions grep RUN stdout (install
# version, WARN lines), and a cached layer prints "CACHED" instead of
# re-emitting that output — which would make a real pass look like a failure
# or hide a false-pass. TARGETARCH is explicit (see header). Deterministic
# execution every run. --progress=plain is capability-probed: BuildKit needs
# it so RUN stdout lands in the captured log, while some classic-builder CLIs
# (whose output is already plain) reject the flag outright.
PROGRESS_ARGS=()
if docker build --help 2>/dev/null | grep -q -- --progress; then
  PROGRESS_ARGS=(--progress=plain)
fi
build() { # $1 log-file, rest: extra --build-arg flags
  local log="$1"; shift
  docker build --no-cache ${PROGRESS_ARGS[@]+"${PROGRESS_ARGS[@]}"} \
    --build-arg "TARGETARCH=${HOST_TARGETARCH}" "$@" \
    -f "${workdir}/Dockerfile" "${workdir}" >"${log}" 2>&1
}

if [ "${G5_GATING_SKIP_DOCKER:-0}" != "1" ]; then

note "A. disabled mode"
# No bucket -> no download -> SHAs irrelevant; assert nothing installs.
build "${workdir}/a.log" || { cat "${workdir}/a.log"; fail "disabled-mode build failed"; }
grep -q "sccache disabled (SCCACHE_GCS_BUCKET empty)" "${workdir}/a.log" || fail "A: missing disabled marker"
grep -q "HARNESS: disabled-clean" "${workdir}/a.log" || fail "A: disabled assertions did not pass"

note "B. checksum failure is fail-soft and never installs (deterministic fixture bytes)"
# Deterministic fixture bytes replace the download entirely: GitHub
# availability cannot decide this case, and a download failure cannot
# impersonate a checksum failure — the sha256sum FAILED line below only
# exists when verification ran against real bytes. ONLY bad SHAs are passed.
build "${workdir}/b.log" \
    --build-arg "SCCACHE_GCS_BUCKET=g5-gating-test-bucket" \
    --build-arg "SCCACHE_TEST_FIXTURE=g5-deterministic-checksum-fixture-bytes" \
    --build-arg "SCCACHE_SHA256_AMD64=0000000000000000000000000000000000000000000000000000000000000000" \
    --build-arg "SCCACHE_SHA256_ARM64=0000000000000000000000000000000000000000000000000000000000000000" \
  || { cat "${workdir}/b.log"; fail "checksum-failure build failed (must be fail-soft)"; }
grep -q "/tmp/sccache.tar.gz: FAILED" "${workdir}/b.log" || { cat "${workdir}/b.log"; fail "B: sha256sum did not report FAILED — checksum verification never ran"; }
grep -q "computed checksum did NOT match" "${workdir}/b.log" || { cat "${workdir}/b.log"; fail "B: sha256sum mismatch warning missing"; }
grep -q "WARN: sccache download or checksum verification failed" "${workdir}/b.log" || { cat "${workdir}/b.log"; fail "B: fail-soft WARN not emitted"; }
grep -q "HARNESS: no-binary-uncached-build-succeeded" "${workdir}/b.log" || { cat "${workdir}/b.log"; fail "B: build did not complete in the no-binary uncached path"; }
grep -q "sccache 0.17" "${workdir}/b.log" && fail "B: sccache reported a version — an unverified binary WAS installed (false-pass)"

note "C. unreachable backend degrades (strict preflight fail-open, end to end)"
# GOOD SHAs and a REAL download so the binary DOES install (verifying the
# checksum path accepts real bytes for this host's arch); the backend bucket
# is unreachable, so the strict start-server preflight must fail and the
# compile must degrade to uncached but succeed.
build "${workdir}/c.log" \
    --build-arg "SCCACHE_GCS_BUCKET=g5-nonexistent-bucket-gating-test" \
    --build-arg "SCCACHE_SHA256_AMD64=${SHA_AMD64}" \
    --build-arg "SCCACHE_SHA256_ARM64=${SHA_ARM64}" \
  || { cat "${workdir}/c.log"; fail "unreachable-backend build failed (must degrade)"; }
grep -q "sccache 0.17" "${workdir}/c.log" || { cat "${workdir}/c.log"; fail "C: sccache binary did not install (good-SHA path broken) — degradation not actually exercised"; }
grep -q "WARN: sccache backend unavailable; compiling uncached" "${workdir}/c.log" || fail "C: missing degradation WARN"
grep -q "HARNESS: degraded-uncached-build-succeeded" "${workdir}/c.log" || fail "C: degraded build did not complete"

else
note "A–C SKIPPED (G5_GATING_SKIP_DOCKER=1) — running sync/wiring checks only"
fi

note "D. harness↔product sync + deploy-yaml wiring (structural)"

# D1. Load-bearing line sync: the real Dockerfiles carry every gating /
# preflight / COMPLETE arch-case line the harness models (including the sha
# selection on both arches and the clearing warn/skip default — a regression
# in either would silently disable caching on that arch).
# The leaves no longer download sccache — the base-rust tier bakes the pinned
# binary (first on PATH) and the leaves keep only the per-RUN preflight, which
# now also arms the LOCAL-DISK backend against the tier's baked cache when no
# GCS bucket is configured (docker_images D18/D20). The load-bearing lines are
# therefore the preflight's two arming branches and its strict start sequence.
for df in infra/docker/Dockerfile.g5-api infra/docker/Dockerfile.g5d; do
  for anchor in \
    'sccache_preflight() {' \
    'if [ -n "$SCCACHE_GCS_BUCKET" ]; then' \
    'sccache --stop-server' \
    'if sccache --start-server' \
    'WARN: sccache backend unavailable; compiling uncached' \
    'elif [ -n "$(find "${SCCACHE_DIR:-/opt/g5-sccache}" -type f -print -quit 2>/dev/null)" ]; then' \
    'export RUSTC_WRAPPER=sccache CARGO_INCREMENTAL=0;' ; do
    grep -qF "$anchor" "${GEN_ROOT}/${df}" || fail "D: ${df} lost load-bearing line: ${anchor}"
  done
done
# The tier that now owns the binary must keep the download sha-verified and the
# unsupported-arch guard fail-closed.
for anchor in \
  'echo "${SCCACHE_SHA256}  /tmp/sccache.tar.gz" | sha256sum -c -' \
  'FATAL: no pinned sccache sha256 for arch' ; do
  grep -qF "$anchor" "${GEN_ROOT}/infra/docker/Dockerfile.base-rust" || fail "D: base-rust lost load-bearing line: ${anchor}"
done

# D2. Pin synchronization: the product ARG pins must equal this script's pins
# EXACTLY. A stale or malformed product pin would make production silently
# skip installation while the harness still passed with its own valid pins.
# Pins now live in the tier Dockerfile (unquoted ARG form).
tier_df="infra/docker/Dockerfile.base-rust"
product_version="$(sed -n 's/^ARG SCCACHE_VERSION=\(.*\)$/\1/p' "${GEN_ROOT}/${tier_df}")"
product_amd64="$(sed -n 's/^ARG SCCACHE_SHA256_X86_64=\(.*\)$/\1/p' "${GEN_ROOT}/${tier_df}")"
product_arm64="$(sed -n 's/^ARG SCCACHE_SHA256_AARCH64=\(.*\)$/\1/p' "${GEN_ROOT}/${tier_df}")"
[ "$product_version" = "$SCCACHE_VERSION" ] || fail "D: ${tier_df} SCCACHE_VERSION '${product_version}' != harness '${SCCACHE_VERSION}'"
[ "$product_amd64" = "$SHA_AMD64" ] || fail "D: ${tier_df} SCCACHE_SHA256_X86_64 '${product_amd64}' != harness pin"
[ "$product_arm64" = "$SHA_ARM64" ] || fail "D: ${tier_df} SCCACHE_SHA256_AARCH64 '${product_arm64}' != harness pin"

# D3. Per-invocation preflight structure: join each RUN command's
# continuation lines, split its body on `&&`, and require the segment
# immediately preceding EVERY `cargo …` segment to be exactly
# `sccache_preflight`. String-existence cannot satisfy this: removing the
# preflight from any one invocation fails that invocation's check.
for df in infra/docker/Dockerfile.g5-api infra/docker/Dockerfile.g5d; do
  awk -v df="${df}" '
    function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
    /^RUN /   { inrun = 1; buf = "" }
    inrun {
      line = $0
      sub(/^RUN /, "", line)
      cont = (line ~ /\\[ \t]*$/)
      sub(/\\[ \t]*$/, "", line)
      buf = buf " " line
      if (!cont) { bodies[++n] = buf; inrun = 0 }
    }
    END {
      cargo_total = 0
      for (i = 1; i <= n; i++) {
        m = split(bodies[i], segs, /&&/)
        prev = ""
        for (j = 1; j <= m; j++) {
          seg = trim(segs[j])
          if (seg ~ /^cargo[ \t]/) {
            cargo_total++
            if (prev != "sccache_preflight") {
              printf "FAIL: D: %s cargo invocation not immediately preceded by sccache_preflight: %s\n", df, seg > "/dev/stderr"
              exit 1
            }
          }
          if (seg != "") prev = seg
        }
      }
      if (cargo_total < 2) {
        printf "FAIL: D: %s expected at least 2 preflighted cargo invocations, found %d (structure check lost its subject)\n", df, cargo_total > "/dev/stderr"
        exit 1
      }
    }
  ' "${GEN_ROOT}/${df}" || exit 1
done

# D4. Deploy-yaml wiring, scoped PER STEP: both Rust-compiling steps must
# each define the conditional, populate SCCACHE_ARGS inside it, and consume
# the array in their own `docker build`. A global grep would let one step
# lose its wiring while the other still satisfied the string search.
grep -qF '_SCCACHE_GCS_BUCKET: ""' "${GEN_ROOT}/infra/ci/cloudbuild-deploy.yaml" || fail "D: deploy substitution must default empty"
deploy_step_body() { # $1 step id
  awk -v id="$1" '
    $0 == "  - id: " id { grab = 1; print; next }
    grab && /^  - id: / { grab = 0 }
    grab { print }
  ' "${GEN_ROOT}/infra/ci/cloudbuild-deploy.yaml"
}
for step in build-api build-g5d; do
  body="$(deploy_step_body "${step}")"
  [ -n "$body" ] || fail "D: deploy step ${step} not found in infra/ci/cloudbuild-deploy.yaml"
  printf '%s\n' "$body" | grep -qF 'if [[ -n "${_SCCACHE_GCS_BUCKET}" ]]; then' \
    || fail "D: ${step} lost the non-empty-substitution conditional"
  # Enabled-mode wiring is --network=host (BuildKit rejects the legacy
  # cloudbuild network mode) plus the token shim: host-net metadata serves the
  # worker's untrusted SA, so a sidecar on the cloudbuild network mints the
  # build-SA token and feeds it via SCCACHE_GCS_CREDENTIALS_URL. Both must be
  # present, per step, or an enabled build silently loses cache/auth.
  printf '%s\n' "$body" | grep -qF 'SCCACHE_ARGS+=(--network=host --build-arg="SCCACHE_GCS_BUCKET=${_SCCACHE_GCS_BUCKET}")' \
    || fail "D: ${step} lost the --network=host SCCACHE_ARGS population inside the conditional"
  printf '%s\n' "$body" | grep -qF 'SCCACHE_GCS_CREDENTIALS_URL=http://' \
    || fail "D: ${step} lost the token-shim SCCACHE_GCS_CREDENTIALS_URL wiring"
  printf '%s\n' "$body" | grep -qF 'docker run -d --name sccache-tokshim --network=cloudbuild' \
    || fail "D: ${step} lost the cloudbuild-network token shim sidecar"
  printf '%s\n' "$body" | grep -qF "trap 'docker rm -f sccache-tokshim" \
    || fail "D: ${step} lost the EXIT trap that cleans up the token shim"
  printf '%s\n' "$body" | grep -qF '"$${SCCACHE_ARGS[@]}"' \
    || fail "D: ${step} docker build no longer consumes SCCACHE_ARGS"
done

if [ "${G5_GATING_SKIP_DOCKER:-0}" = "1" ]; then
  echo "sccache gating contract: sync-only ok (docker sections SKIPPED — not the full contract)"
else
  echo "sccache gating contract: ok"
fi
