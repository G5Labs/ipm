#!/usr/bin/env bash
set -euo pipefail

GEN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

fail() {
    echo "dev build profile contract: $*" >&2
    exit 1
}

assert_contains() {
    local file="$1"
    local text="$2"
    grep -Fq -- "$text" "$file" || fail "$file is missing: $text"
}

assert_count() {
    local expected="$1"
    local file="$2"
    local text="$3"
    local actual
    actual="$(grep -Fc -- "$text" "$file" || true)"
    [[ "$actual" == "$expected" ]] || \
        fail "$file contains '$text' $actual times; expected $expected"
}

assert_not_contains() {
    local file="$1"
    local text="$2"
    if grep -Fq -- "$text" "$file"; then
        fail "$file unexpectedly contains: $text"
    fi
}

assert_contains "$GEN_ROOT/Cargo.toml" '[profile.g5-dev]'
assert_contains "$GEN_ROOT/Cargo.toml" 'inherits = "dev"'
assert_contains "$GEN_ROOT/Cargo.toml" 'debug = 0'
assert_contains "$GEN_ROOT/Cargo.toml" 'incremental = true'

for dockerfile in infra/docker/Dockerfile.g5-api infra/docker/Dockerfile.g5d; do
    assert_contains "$GEN_ROOT/$dockerfile" 'ARG G5_CARGO_PROFILE=release'
    assert_contains "$GEN_ROOT/$dockerfile" 'ARG G5_DOCKER_CARGO_BUILD_JOBS=2'
    assert_contains "$GEN_ROOT/$dockerfile" \
        'ENV CARGO_BUILD_JOBS="${G5_DOCKER_CARGO_BUILD_JOBS}"'
    assert_contains "$GEN_ROOT/$dockerfile" 'cargo build --profile "$G5_CARGO_PROFILE"'
    if grep -Fq -- 'cargo build --release' "$GEN_ROOT/$dockerfile"; then
        fail "$dockerfile bypasses the selected Cargo profile"
    fi
done

# The API and workspace-host images build concurrently under Compose. Their
# dummy-crate warmup stages must not mutate one shared Cargo target cache: one image can
# otherwise make the other accept placeholder local-crate metadata as fresh.
assert_count 2 "$GEN_ROOT/infra/docker/Dockerfile.g5-api" \
    'id=cargo-target-g5-api-build,target=/build/target,sharing=locked'
assert_count 2 "$GEN_ROOT/infra/docker/Dockerfile.g5d" \
    'id=cargo-target-g5d-build,target=/build/target,sharing=locked'
assert_not_contains "$GEN_ROOT/infra/docker/Dockerfile.g5-api" 'id=cargo-target-g5-build'
assert_not_contains "$GEN_ROOT/infra/docker/Dockerfile.g5d" 'id=cargo-target-g5-build'

assert_contains "$GEN_ROOT/infra/docker/Dockerfile.g5-api" \
    'cp "/build/target/${G5_CARGO_PROFILE}/g5-api" /usr/local/bin/g5-api'
assert_contains "$GEN_ROOT/infra/docker/Dockerfile.g5d" \
    'cp "/build/target/${G5_CARGO_PROFILE}/g5d" /usr/local/bin/g5d'
assert_contains "$GEN_ROOT/infra/docker/Dockerfile.g5d" \
    'FROM scratch AS guest-runtime-artifacts'
# Fake-provider mode fails closed without the fakes beside the guest g5d, so
# the guest artifact export and both staging paths carry them (plan P3.2).
assert_contains "$GEN_ROOT/infra/docker/Dockerfile.g5d" \
    'cp "/usr/local/bin/$fake" "/opt/g5-guest-artifacts/bin/$fake"'
assert_contains "$GEN_ROOT/scripts/stage-local-worker-runtime.sh" \
    'install -m 0755 "$GUEST_EXPORT_DIR/bin/$fake" "$RUNTIME_BIN/$fake"'
assert_contains "$GEN_ROOT/scripts/stage-local-worker-runtime.sh" \
    'docker cp "$IMAGE_CID:/usr/local/bin/$fake" "$RUNTIME_BIN/$fake"'

assert_count 1 "$GEN_ROOT/infra/docker/docker-compose.dev.yml" \
    'G5_CARGO_PROFILE: ${G5_DEV_DOCKER_CARGO_PROFILE:-g5-dev}'
assert_count 2 "$GEN_ROOT/infra/docker/docker-compose.test.yml" \
    'G5_CARGO_PROFILE: "${G5_CI_DOCKER_CARGO_PROFILE:-g5-dev}"'
assert_contains "$GEN_ROOT/scripts/stage-local-worker-runtime.sh" \
    '--build-arg "G5_CARGO_PROFILE=${G5_DEV_DOCKER_CARGO_PROFILE:-g5-dev}"'
assert_contains "$GEN_ROOT/scripts/stage-local-worker-runtime.sh" \
    '--target guest-runtime-artifacts'
assert_contains "$GEN_ROOT/scripts/stage-local-worker-runtime.sh" \
    '--output "type=local,dest=$GUEST_EXPORT_DIR"'
python3 "$GEN_ROOT/scripts/test_stage_local_worker_runtime.py"

assert_contains "$GEN_ROOT/scripts/worker-host-env.sh" \
    'G5_DEV_HOST_CARGO_PROFILE="${G5_DEV_HOST_CARGO_PROFILE:-g5-dev}"'
assert_contains "$GEN_ROOT/scripts/worker-host-env.sh" \
    'G5_DEV_HOST_CARGO_TARGET_DIR="${G5_DEV_HOST_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-${GEN_ROOT}/target}}"'
assert_contains "$GEN_ROOT/scripts/worker-host-env.sh" \
    'G5_WORKER_HOST_G5D="${G5_DEV_HOST_CARGO_TARGET_DIR}/${G5_DEV_HOST_CARGO_PROFILE}/g5d"'
assert_contains "$GEN_ROOT/scripts/worker-host-env.sh" \
    'cargo build --target-dir "$G5_DEV_HOST_CARGO_TARGET_DIR" --profile "$G5_DEV_HOST_CARGO_PROFILE" --jobs "$jobs"'
assert_contains "$GEN_ROOT/scripts/worker-host-env.sh" \
    'printf '\''G5_DEV_HOST_CARGO_PROFILE=%s\n'\'' "$G5_DEV_HOST_CARGO_PROFILE"'
assert_not_contains "$GEN_ROOT/scripts/worker-host-env.sh" 'target/release/g5d'

dev_stack_up_body="$(sed -n '/^dev-stack-up /,/^dev-up /p' "$GEN_ROOT/Justfile")"
dev_up_body="$(sed -n '/^dev-up /,/^dev-down/p' "$GEN_ROOT/Justfile")"
# Compose startup is shared with workspace previews through dev-stack-up;
# dev-up delegates its flags before starting the native worker host.
dev_stack_up_body="$(sed -n '/^dev-stack-up /,/^dev-up /p' "$GEN_ROOT/Justfile")"
grep -Fq -- 'just dev-stack-up {{FLAGS}}' <<<"$dev_up_body" || \
    fail "dev-up does not delegate its flags to dev-stack-up"
grep -Fq -- 'build $EXTRA oauth-stub g5-api edge' <<<"$dev_stack_up_body" || \
    fail "dev-stack-up does not use the bounded control-plane build set"
if grep -Fq -- 'G5_DEV_WORKER=process' <<<"$dev_up_body
$dev_stack_up_body"; then
    fail "dev-up still exposes the retired process-worker fallback"
fi

live_host_reuse_body="$(sed -n \
    '/# A live pid file is not enough:/,/CURRENT_FINGERPRINT=/p' \
    "$GEN_ROOT/Justfile")"
for expected in \
    'g5_worker_host_build' \
    'g5_worker_host_check_host_vm_exec' \
    'CURRENT_FINGERPRINT='; do
    grep -Fq -- "$expected" <<<"$live_host_reuse_body" || \
        fail "live worker-host reuse path is missing: $expected"
done
build_line="$(grep -nF -- 'g5_worker_host_build' <<<"$live_host_reuse_body" | head -1 | cut -d: -f1)"
fingerprint_line="$(grep -nF -- 'CURRENT_FINGERPRINT=' <<<"$live_host_reuse_body" | head -1 | cut -d: -f1)"
if (( build_line >= fingerprint_line )); then
    fail "live worker-host reuse must rebuild before fingerprinting the host binary"
fi
assert_contains "$GEN_ROOT/Justfile" 'g5_worker_host_check_host_vm_exec || fail'
assert_contains "$GEN_ROOT/Justfile" 'g5_worker_host_exec_as_root "$G5_WORKER_HOST_G5D" host'
assert_contains "$GEN_ROOT/Justfile" '"$G5_WORKER_HOST_G5D" host </dev/null'
assert_contains "$GEN_ROOT/Justfile" 'HOST_BINARY_CURRENT=0'
assert_contains "$GEN_ROOT/Justfile" 'HOST_BINARY_CURRENT=1'
assert_contains "$GEN_ROOT/Justfile" 'if [[ "$HOST_BINARY_CURRENT" != "1" ]]; then'
assert_not_contains "$GEN_ROOT/Justfile" 'target/release/g5d'
assert_contains "$GEN_ROOT/infra/docker/Dockerfile.ci-builder" \
    'curl jq python3 nodejs docker.io'
assert_contains "$GEN_ROOT/infra/docker/Dockerfile.ci-builder" 'node --version'
assert_contains "$GEN_ROOT/infra/docker/Dockerfile.ci-builder" \
    'ARG G5_CI_BUILDER_RELEASE_WARMUP=1'
assert_contains "$GEN_ROOT/infra/docker/Dockerfile.ci-builder" \
    'if [ "$G5_CI_BUILDER_RELEASE_WARMUP" = "1" ]; then'
assert_count 2 "$GEN_ROOT/infra/docker/docker-compose.test.yml" 'image: ${G5_TEST_API_IMAGE:-g5/api:test}'
test_migrate_body="$(sed -n '/^  db-migrate:/,/^  db-bootstrap-runtime-users:/p' \
    "$GEN_ROOT/infra/docker/docker-compose.test.yml")"
grep -Fq -- 'image: ${G5_TEST_API_IMAGE:-g5/api:test}' <<<"$test_migrate_body" || \
    fail "test db-migrate does not reuse the g5-api image"
grep -Fq -- 'pull_policy: never' <<<"$test_migrate_body" || \
    fail "test db-migrate may pull instead of reusing the local g5-api image"
if grep -Fq -- 'build:' <<<"$test_migrate_body"; then
    fail "test db-migrate still declares a duplicate g5-api build"
fi
assert_contains "$GEN_ROOT/Justfile" \
    'for service in oauth-stub client g5-api g5d-host; do'

ci_docker_body="$(sed -n '/^ci-docker /,/^# ═.*Database/p' "$GEN_ROOT/Justfile")"
for expected in \
    'CI_CONTAINER_WORKSPACE="/workspace"' \
    'CI_CONTAINER_CARGO_HOME="/opt/g5-cargo-home"' \
    'CI_CONTAINER_TARGET="/opt/g5-rust-target-cache/target"' \
    'CI_CACHE_LAYOUT_VERSION="ci-builder-lean-paths-v3"' \
    'CI_CACHE_LAYOUT_MARKER=".g5-local-cache-layout"' \
    '--build-arg "G5_CI_BUILDER_RELEASE_WARMUP=${G5_CI_DOCKER_RELEASE_WARMUP:-0}"' \
    'prepare_local_ci_cache_layout()' \
    '-v "${CI_TARGET_VOLUME}:${CI_CONTAINER_TARGET}"' \
    'sh -c '\''printf "%s\n" "$G5_CI_LOCAL_CACHE_LAYOUT" > "${CARGO_TARGET_DIR}/${G5_CI_LOCAL_CACHE_MARKER}"'\''' \
    '-v $HOST_SRC:$CI_CONTAINER_WORKSPACE' \
    '-v g5-ci-cargo-registry:$CI_CONTAINER_CARGO_HOME/registry' \
    '-v g5-ci-cargo-git:$CI_CONTAINER_CARGO_HOME/git' \
    '-v ${CI_TARGET_VOLUME}:$CI_CONTAINER_TARGET' \
    '-e CARGO_HOME=$CI_CONTAINER_CARGO_HOME' \
    '-e CARGO_TARGET_DIR=$CI_CONTAINER_TARGET'; do
    grep -Fq -- "$expected" <<<"$ci_docker_body" || \
        fail "ci-docker does not reuse the CI builder cache path: $expected"
done
for stale_path in '/build/target' '/usr/local/cargo/registry' '/usr/local/cargo/git'; do
    if grep -Fq -- "$stale_path" <<<"$ci_docker_body"; then
        fail "ci-docker still hides the CI builder cache behind: $stale_path"
    fi
done

echo "dev build profile contract: ok"
