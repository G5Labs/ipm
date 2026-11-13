#!/usr/bin/env bash
# Regression for the `full-stack` preview profile beside a developer's own stack
# (ontology g5.runtime.boot.backend.process-local, review R8/R9): the profile's
# stack identity must differ from the default `g5-dev` project, every Compose
# consumer must follow that identity (project, container names, volumes, image
# tag, ports), the published port must not be the developer's UI, and the
# lifetime loop must not rely on `sleep infinity`, which Apple's sleep refuses.
# `full-stack` runs its worker host in the default microvm mode behind the
# prerequisite check (design nested-virt-followup §3.5); the process-mode
# stack is the `full-stack-process` sibling, selected only inside its own
# shell, never by a fallback.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
PROFILE=".g5/preview_profiles/full-stack.yaml"
fail() { echo "✗ $*" >&2; exit 1; }
# One cleanup for every fixture below: a failed assertion exits at once, so
# the exit handler ends the fake hosts and removes the temporary directories.
STUB=""; FIX=""; WH=""; FWD=""; GUARD=""; GUARD_DEFAULT_PID=""; GUARD_EXTRA_PID=""
cleanup() {
    [ -n "$GUARD_DEFAULT_PID" ] && kill "$GUARD_DEFAULT_PID" "$GUARD_EXTRA_PID" 2>/dev/null || true
    for d in "$STUB" "$FIX" "$WH" "$FWD" "$GUARD"; do [ -n "$d" ] && rm -rf "$d"; done
    return 0
}
trap cleanup EXIT

field() { sed -n "s/^$1: *//p" "$PROFILE" | head -1; }
env_value() { sed -n "s/^  $1: *\"\\{0,1\\}\\([^\"]*\\)\"\\{0,1\\}\$/\\1/p" "$PROFILE" | head -1; }

PORT="$(field port)"
COMMAND="$(field command)"
CLEANUP="$(field cleanup_command)"
[ -n "$PORT" ] && [ "$PORT" != "3000" ] || fail "the preview must not publish the developer's UI port (port: $PORT)"
[ "$(env_value G5_PORT_EDGE)" = "$PORT" ] || fail "G5_PORT_EDGE must equal the profile port"
for key in G5_PORT_API G5_PORT_POSTGRES G5_PORT_OAUTH_STUB G5_IMAGE_TAG; do
    [ -n "$(env_value "$key")" ] || fail "profile env must set $key"
done
[ "$(env_value G5_PORT_API)" != "7500" ] || fail "the preview API port must differ from the default"
[ "$(env_value G5_PORT_POSTGRES)" != "5432" ] && [ "$(env_value G5_PORT_POSTGRES)" != "5433" ] || fail "the preview Postgres port must differ from the defaults"
[ "$(env_value G5_PORT_OAUTH_STUB)" != "9876" ] || fail "the preview OAuth stub port must differ from the default"
[ "$(env_value G5_IMAGE_TAG)" != "dev" ] || fail "the preview image tag must differ from the default"
echo "✓ profile publishes its own ports and image tag"

case "$COMMAND" in
    *'G5_COMPOSE_PROJECT="g5-preview-${G5D_PROCESS_INCARNATION:-vm}"'*) ;;
    *) fail "command must set a per-incarnation Compose project: $COMMAND" ;;
esac
case "$COMMAND" in *'G5_COMPOSE_VOLUMES="$G5_COMPOSE_PROJECT"'*) ;; *) fail "command must scope volumes to the project" ;; esac
case "$CLEANUP" in
    *'G5_COMPOSE_PROJECT="g5-preview-${G5D_PROCESS_INCARNATION:-vm}"'*) ;;
    *) fail "cleanup must target the same per-incarnation project: $CLEANUP" ;;
esac
case "$CLEANUP" in *'G5_COMPOSE_VOLUMES="$G5_COMPOSE_PROJECT"'*) ;; *) fail "cleanup must scope volumes to the project" ;; esac
# The preview is the previewed instance's localhost with VM isolation: it runs
# the whole stack, including that instance's own worker host in the default
# microvm mode, after scripts/preview-microvm-host.sh proved /dev/kvm, the
# runtime disk's platform and the reflink root, and it never selects process
# mode (g5.host.isolation: no fallback). Close stops that host (the one
# recorded under the profile's host directory, never the developer's) before
# the containers, so it runs `dev-down`, not the containers-only
# `dev-stack-down`.
case "$COMMAND" in *'G5D_ISOLATION_MODE=microvm; unset G5D_ALLOW_PROCESS_ISOLATION;'*) ;; *) fail "full-stack must pin microvm mode and clear the process acknowledgement in its own shell: $COMMAND" ;; esac
case "$COMMAND" in *'G5D_ISOLATION_MODE=process'*) fail "full-stack must never select process mode: $COMMAND" ;; esac
case "$COMMAND" in *'scripts/preview-microvm-host.sh && just dev-up &&'*) ;; *) fail "command must prove the microvm prerequisites and then run the whole stack with just dev-up: $COMMAND" ;; esac
case "$COMMAND" in *'just dev-up && { node scripts/seed-source-control-preview.mjs ||'*) ;; *) fail "seed the real Source Control examples only after the worker is ready" ;; esac
[ "$(env_value G5D_HOST_MAX_MICROVMS)" = "3" ] || fail "preview needs two fixture slots and one browsing slot"
[ "$(env_value G5_DEV_HOST_DIR)" = "/var/lib/g5-dev-host/root" ] || fail "the microvm host's directory must be the reflink root outside /work"
[ -n "$(env_value G5D_SCRATCH_DISK_MIB)" ] || fail "the microvm profile must bound each inner guest's scratch disk"
node --test scripts/test-source-control-preview.mjs
case "$CLEANUP" in *'G5D_ISOLATION_MODE=microvm; unset G5D_ALLOW_PROCESS_ISOLATION;'*) ;; *) fail "full-stack cleanup must carry the same microvm pin for worker-host-stop: $CLEANUP" ;; esac
case "$CLEANUP" in *'just dev-down'*) ;; *) fail "cleanup must stop the previewed host and its containers with dev-down: $CLEANUP" ;; esac
case "$COMMAND" in *'sleep infinity'*) fail "sleep infinity exits on macOS; use a bounded loop" ;; esac
case "$COMMAND" in *'while :; do sleep 3600; done'*) ;; *) fail "command must keep the service alive with a bounded sleep loop" ;; esac
echo "✓ profile names its own Compose project and volumes and keeps alive portably"

# The process-mode sibling: the same stack identity, ports and seeds with the
# host in explicit process isolation, selected and acknowledged inside its own
# shell (command and cleanup alike, so worker-host-stop finds the host), and
# no microvm prerequisite check.
PROCESS_PROFILE=".g5/preview_profiles/full-stack-process.yaml"
[ -f "$PROCESS_PROFILE" ] || fail "the process-mode profile $PROCESS_PROFILE must exist"
pfield() { sed -n "s/^$1: *//p" "$PROCESS_PROFILE" | head -1; }
[ "$(pfield port)" = "$PORT" ] || fail "the process-mode profile must publish the same preview port"
PCOMMAND="$(pfield command)"; PCLEANUP="$(pfield cleanup_command)"
case "$PCOMMAND" in *'G5_COMPOSE_PROJECT="g5-preview-${G5D_PROCESS_INCARNATION:-vm}"'*) ;; *) fail "process-mode command must use the per-incarnation project: $PCOMMAND" ;; esac
case "$PCOMMAND" in *'G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1'*) ;; *) fail "process-mode command must select process isolation with its acknowledgement: $PCOMMAND" ;; esac
case "$PCOMMAND" in *'just dev-up && { node scripts/seed-source-control-preview.mjs ||'*) ;; *) fail "process-mode command must run dev-up and then seed: $PCOMMAND" ;; esac
case "$PCOMMAND" in *'preview-microvm-host'*) fail "process-mode command must not run the microvm prerequisite check: $PCOMMAND" ;; esac
case "$PCLEANUP" in *'G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1'*) ;; *) fail "process-mode cleanup must carry the same isolation selection for worker-host-stop: $PCLEANUP" ;; esac
case "$PCLEANUP" in *'just dev-down'*) ;; *) fail "process-mode cleanup must run dev-down: $PCLEANUP" ;; esac
for key in G5_PORT_EDGE G5_PORT_API G5_PORT_POSTGRES G5_PORT_OAUTH_STUB G5_IMAGE_TAG G5D_HOST_MAX_MICROVMS; do
    [ "$(sed -n "s/^  $key: *\"\{0,1\}\([^\"]*\)\"\{0,1\}\$/\1/p" "$PROCESS_PROFILE" | head -1)" = "$(env_value "$key")" ] || fail "process-mode profile must carry the same $key"
done
grep -q 'G5_DEV_HOST_DIR' "$PROCESS_PROFILE" && fail "the process-mode profile keeps the checkout's .dev/ host directory"
echo "✓ the process-mode sibling shares the identity and selects process isolation explicitly"

# The containers-only sibling: same identity and ports, no workspace host, no
# isolation selection, and a containers-only Close.
STACK_PROFILE=".g5/preview_profiles/full-stack-no-worker.yaml"
[ -f "$STACK_PROFILE" ] || fail "the containers-only profile $STACK_PROFILE must exist"
sfield() { sed -n "s/^$1: *//p" "$STACK_PROFILE" | head -1; }
[ "$(sfield port)" = "$PORT" ] || fail "the containers-only profile must publish the same preview port"
SCOMMAND="$(sfield command)"; SCLEANUP="$(sfield cleanup_command)"
case "$SCOMMAND" in *'G5_COMPOSE_PROJECT="g5-preview-${G5D_PROCESS_INCARNATION:-vm}"'*) ;; *) fail "containers-only command must use the per-incarnation project: $SCOMMAND" ;; esac
case "$SCOMMAND" in *'just dev-stack-up &&'*) ;; *) fail "containers-only command must run just dev-stack-up: $SCOMMAND" ;; esac
case "$SCOMMAND" in *'G5D_ISOLATION_MODE'*) fail "containers-only command must not select an isolation mode: $SCOMMAND" ;; esac
case "$SCLEANUP" in *'just dev-stack-down'*) ;; *) fail "containers-only cleanup must run dev-stack-down: $SCLEANUP" ;; esac
case "$SCLEANUP" in *'dev-down'*) fail "containers-only cleanup must not stop any worker host: $SCLEANUP" ;; esac
for key in G5_PORT_EDGE G5_PORT_API G5_PORT_POSTGRES G5_PORT_OAUTH_STUB G5_IMAGE_TAG; do
    [ "$(sed -n "s/^  $key: *\"\\{0,1\\}\\([^\"]*\\)\"\\{0,1\\}\$/\\1/p" "$STACK_PROFILE" | head -1)" = "$(env_value "$key")" ] || fail "containers-only profile must carry the same $key"
done
echo "✓ the containers-only profile shares the identity and skips the host"

# The frontend-only sibling: the Vite dev server in mock mode, nothing else.
FRONTEND_PROFILE=".g5/preview_profiles/frontend-only.yaml"
[ -f "$FRONTEND_PROFILE" ] || fail "the frontend-only profile $FRONTEND_PROFILE must exist"
ffield() { sed -n "s/^$1: *//p" "$FRONTEND_PROFILE" | head -1; }
[ "$(ffield port)" = "15174" ] || fail "the frontend profile must publish a port off Vite's own 5174 (a developer's dev server on a process host): $(ffield port)"
[ "$(ffield working_dir)" = "frontend" ] || fail "the frontend profile must run in frontend/"
FCOMMAND="$(ffield command)"
case "$FCOMMAND" in *'npm run dev -- --host 127.0.0.1 --port "$PORT" --strictPort'*) ;; *) fail "frontend command must run Vite on the preview port: $FCOMMAND" ;; esac
case "$FCOMMAND" in *'npm ci'*) ;; *) fail "frontend command must install dependencies first: $FCOMMAND" ;; esac
case "$FCOMMAND" in *docker*|*'just '*|*G5D_ISOLATION_MODE*) fail "frontend command must start no stack or host: $FCOMMAND" ;; esac
grep -q '^  VITE_MOCK: "true"$' "$FRONTEND_PROFILE" || fail "the frontend profile must run in mock mode"
[ -z "$(ffield cleanup_command)" ] || fail "the frontend profile leaves nothing to clean up"
echo "✓ the frontend-only profile runs Vite in mock mode without a stack"

# dev-up publishes the UI last: host build, containers without the edge, host
# start, then the edge, so "up" means a workspace can be opened.
# Only the dev-up recipe body is searched: its text from the header to the
# next recipe header, so no comment or other recipe elsewhere can satisfy a
# lookup, and each step must appear exactly once.
DEV_UP_RECIPE="$(awk '/^dev-up \*FLAGS:/{p=1; next} p && /^[a-zA-Z_-]+( [^:]*)?:/{exit} p' Justfile)"
step_line() { # <exact line> → its line number within the recipe body, or empty
    local n; n="$(printf '%s\n' "$DEV_UP_RECIPE" | grep -nxF -- "$1" | cut -d: -f1)"
    [ "$(printf '%s\n' "$n" | grep -c .)" = 1 ] && printf '%s' "$n"
}
L_WITHDRAW=$(step_line '    G5_DEV_WITHDRAW_EDGE=1 just dev-stack-up'); L_BUILD=$(step_line '    just worker-host-build'); L_STACK=$(step_line '    G5_DEV_DEFER_EDGE=1 just dev-stack-up {{FLAGS}}'); L_HOST=$(step_line '    just worker-host-start'); L_WAIT=$(step_line '    ./scripts/wait-for-worker-host.sh "${G5_WORKER_HOST_API_URL:-http://localhost:${G5_PORT_API:-7500}}" 120 "${STARTED_HOST_ID}"'); L_EDGE=$(step_line '    G5_DEV_EDGE_ONLY=1 just dev-stack-up')
[ -n "$L_WITHDRAW" ] && [ -n "$L_BUILD" ] && [ -n "$L_STACK" ] && [ -n "$L_HOST" ] && [ -n "$L_WAIT" ] && [ -n "$L_EDGE" ] || fail "dev-up must, exactly once each, withdraw the edge, build the host, start the stack without the edge, start the host, prove its registration and then publish the edge"
[ "$L_WITHDRAW" -lt "$L_BUILD" ] && [ "$L_BUILD" -lt "$L_STACK" ] && [ "$L_STACK" -lt "$L_HOST" ] && [ "$L_HOST" -lt "$L_WAIT" ] && [ "$L_WAIT" -lt "$L_EDGE" ] || fail "dev-up steps are out of order: withdraw=$L_WITHDRAW build=$L_BUILD stack=$L_STACK host=$L_HOST wait=$L_WAIT edge=$L_EDGE"
# The registration wait targets the host worker-host-start recorded, read
# from the default instance's credential file; a per-org file (the valid
# slug `demo` included) must never be able to take that file's name.
grep -q 'ENV_FILE="${GEN_ROOT}/infra/docker/demo-worker.env"' scripts/seed-dev-worker-token.sh || fail "the default instance's credential file must stay demo-worker.env"
grep -q 'ENV_FILE="${GEN_ROOT}/infra/docker/org-${G5_WORKER_ORG}-worker.env"' scripts/seed-dev-worker-token.sh || fail "per-org credential files must carry a prefix no slug can produce"
# The edge-only phase must never recreate the API beneath the edge.
grep -q 'up -d --no-build --no-deps edge$' Justfile || fail "the edge-only phase must start the edge with --no-deps"
# The build guard inspects every instance sharing the binary before rebuilding.
BUILD_RECIPE="$(awk '/^worker-host-build org=""/,/^worker-host-start org=""/' Justfile)"
echo "✓ dev-up withdraws the edge first and publishes it only after the started host is registered"

# The shared-binary guard, executed: with a live microvm default host and a
# live microvm per-org host, a process-mode build of the per-org instance
# stops every instance (the slug-less stop, with the default directory, not
# the per-org one the helper exported) before the build runs; a build in the
# same mode stops nothing.
GUARD="$(mktemp -d)"
cp Justfile "$GUARD/Justfile"
mkdir -p "$GUARD/scripts/lib" "$GUARD/stubs" "$GUARD/.dev/g5d-host" "$GUARD/.dev/g5d-host-extra"
cp scripts/lib/stack-identity.sh "$GUARD/scripts/lib/"
# Two live processes that `ps` reports as "g5d host"; a failed assertion
# exits at once, so the trap ends them and removes every fixture directory.
bash -c 'exec -a "g5d host" sleep 120' & GUARD_DEFAULT_PID=$!
bash -c 'exec -a "g5d host" sleep 120' & GUARD_EXTRA_PID=$!
sleep 0.3
echo "$GUARD_DEFAULT_PID" > "$GUARD/.dev/g5d-host/host.pid"; echo microvm > "$GUARD/.dev/g5d-host/host.mode"
echo "$GUARD_EXTRA_PID" > "$GUARD/.dev/g5d-host-extra/host.pid"; echo microvm > "$GUARD/.dev/g5d-host-extra/host.mode"
cat > "$GUARD/scripts/worker-host-env.sh" <<'STUBEOF'
GEN_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export G5D_ISOLATION_MODE="${G5D_ISOLATION_MODE:-microvm}"
if [ -n "${G5_WORKER_ORG:-}" ]; then G5_DEV_HOST_DIR="${GEN_ROOT}/.dev/g5d-host-${G5_WORKER_ORG}"; else G5_DEV_HOST_DIR="${G5_DEV_HOST_DIR:-${GEN_ROOT}/.dev/g5d-host}"; fi
export G5_DEV_HOST_DIR
G5_WORKER_HOST_G5D="${GEN_ROOT}/target/g5-dev/g5d"
g5_worker_host_build() { printf 'build\n' >> "${GUARD_ORDER:?}"; }
STUBEOF
printf '#!/usr/bin/env bash\nprintf "%%s host_dir=%%s\\n" "$*" "${G5_DEV_HOST_DIR:-unset}" >> "${GUARD_ORDER:?}"\n' > "$GUARD/stubs/just"
chmod +x "$GUARD/stubs/just"
run_guard() { # <mode> <org> [default-dir-override]
    : > "$GUARD/order"
    ( cd "$GUARD" && env -u G5_DEV_HOST_DIR ${3:+G5_DEV_HOST_DIR="$3"} GUARD_ORDER="$GUARD/order" G5D_ISOLATION_MODE="$1" G5D_ALLOW_PROCESS_ISOLATION=1 \
        bash -c "export PATH=\"$GUARD/stubs:\$PATH\"; exec \"$(command -v just)\" worker-host-build $2" >/dev/null 2>&1 ) || true
}
run_guard process extra
grep -q '^worker-host-stop host_dir=unset$' "$GUARD/order" || fail "a process build of a per-org instance must stop every instance with the default directory: $(cat "$GUARD/order")"
[ "$(sed -n 1p "$GUARD/order")" = "worker-host-stop host_dir=unset" ] && [ "$(sed -n 2p "$GUARD/order")" = "build" ] || fail "the stop must precede the build: $(cat "$GUARD/order")"
run_guard process ""
[ "$(sed -n 1p "$GUARD/order")" = "worker-host-stop host_dir=unset" ] || fail "a process build of the default instance must stop the live microvm hosts first: $(cat "$GUARD/order")"
run_guard process "" "$GUARD/.dev/g5d-host"
grep -q "^worker-host-stop host_dir=$GUARD/.dev/g5d-host$" "$GUARD/order" || fail "a caller's default directory must reach the stop: $(cat "$GUARD/order")"
run_guard microvm extra
[ "$(cat "$GUARD/order")" = "build" ] || fail "a build in the running mode must stop nothing: $(cat "$GUARD/order")"
kill "$GUARD_DEFAULT_PID" "$GUARD_EXTRA_PID" 2>/dev/null || true
rm -rf "$GUARD"
echo "✓ worker-host-build stops every live host of another mode before replacing the shared binary"

# Registration proof: the API must list a ready, live host; a host that only
# started, or one the control plane keeps rejecting, is not enough.
WH="$(mktemp -d)"
cat > "$WH/curl" <<'STUBEOF'
#!/usr/bin/env bash
url=""; for arg in "$@"; do case "$arg" in http*) url="$arg";; esac; done
case "$url" in
    *test-token) printf '{"data":{"jwt":"fixture"}}\n' ;;
    *"/api/v1/orgs") printf '{"data":[{"id":"org-1","orgSlug":"g5labs"}]}\n' ;;
    *worker-hosts*offset=0) cat "${WH_HOSTS:?}" ;;
    *worker-hosts*offset=50) cat "${WH_HOSTS_PAGE2:-/dev/null}" ;;
    *worker-hosts*) printf '{"data":{"hosts":[]}}\n' ;;
esac
STUBEOF
chmod +x "$WH/curl"
printf '{"data":{"hosts":[{"hostId":"h1","status":"ready","heartbeatState":"live"}]}}\n' > "$WH/ready.json"
printf '{"data":{"hosts":[{"hostId":"h1","status":"provisioning","heartbeatState":"never"}]}}\n' > "$WH/pending.json"
# A full first page of retired or stale hosts (the list keeps every retired
# host, newest first) with the live one on the second page.
jq -nc '{data:{hosts:[range(50) | {hostId:("stale-"+tostring), status:"ready", heartbeatState:"stale"}]}}' > "$WH/page1-stale.json"
PATH="$WH:$PATH" WH_HOSTS="$WH/ready.json" bash scripts/wait-for-worker-host.sh http://localhost:17500 5 >/dev/null || fail "a ready, live host must satisfy the registration wait"
if PATH="$WH:$PATH" WH_HOSTS="$WH/pending.json" bash scripts/wait-for-worker-host.sh http://localhost:17500 1 >/dev/null 2>&1; then
    fail "a host that has not registered must not satisfy the registration wait"
fi
PATH="$WH:$PATH" WH_HOSTS="$WH/page1-stale.json" WH_HOSTS_PAGE2="$WH/ready.json" bash scripts/wait-for-worker-host.sh http://localhost:17500 5 >/dev/null || fail "a live host on the second page must satisfy the registration wait"
if PATH="$WH:$PATH" WH_HOSTS="$WH/page1-stale.json" WH_HOSTS_PAGE2="$WH/pending.json" bash scripts/wait-for-worker-host.sh http://localhost:17500 1 >/dev/null 2>&1; then
    fail "a full page of stale hosts must not satisfy the registration wait"
fi
# A host stopped moments ago still reads as live; only the started host counts.
printf '{"data":{"hosts":[{"hostId":"h-new","status":"ready","heartbeatState":"never"},{"hostId":"h-old","status":"ready","heartbeatState":"live"}]}}\n' > "$WH/old-live.json"
if PATH="$WH:$PATH" WH_HOSTS="$WH/old-live.json" bash scripts/wait-for-worker-host.sh http://localhost:17500 1 h-new >/dev/null 2>&1; then
    fail "the previous host's lingering heartbeat must not satisfy the wait for the new host"
fi
printf '{"data":{"hosts":[{"hostId":"h-new","status":"ready","heartbeatState":"live"},{"hostId":"h-old","status":"ready","heartbeatState":"live"}]}}\n' > "$WH/both-live.json"
PATH="$WH:$PATH" WH_HOSTS="$WH/both-live.json" bash scripts/wait-for-worker-host.sh http://localhost:17500 5 h-new >/dev/null || fail "the started host, once live, must satisfy the wait"
rm -rf "$WH"
echo "✓ the registration wait accepts only the started host, ready and live, on any page"

# The recipes follow the exported identity; the default stays g5-dev. Static
# check: no recipe names the default project literally. Executed check: the
# cleanup recipe runs for real against a recording `docker` stub.
if grep -n 'docker compose -p g5-dev ' Justfile; then
    fail "a Justfile recipe still names the default Compose project literally"
fi
grep -q 'docker compose -p "${G5_COMPOSE_PROJECT:-g5-dev}" -f infra/docker/docker-compose.dev.yml' Justfile \
    || fail "dev recipes must select the project through G5_COMPOSE_PROJECT"
STUB="$(mktemp -d)"
cat > "$STUB/docker" <<'STUBEOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${G5_TEST_DOCKER_RECORD:?}"
exit 0
STUBEOF
chmod +x "$STUB/docker"
run_dev_down() { # <project-or-empty> <record>
    : > "$2"
    if [ -n "$1" ]; then
        PATH="$STUB:$PATH" G5_TEST_DOCKER_RECORD="$2" G5_COMPOSE_PROJECT="$1" just dev-down >/dev/null 2>&1 || true
    else
        PATH="$STUB:$PATH" G5_TEST_DOCKER_RECORD="$2" env -u G5_COMPOSE_PROJECT just dev-down >/dev/null 2>&1 || true
    fi
}
run_dev_down g5-preview-test "$STUB/preview.record"
grep -q -- '^compose -p g5-preview-test -f infra/docker/docker-compose.dev.yml --profile demo down -v' "$STUB/preview.record" \
    || fail "dev-down must tear down the preview's own project: $(cat "$STUB/preview.record")"
grep -q -- '-p g5-dev ' "$STUB/preview.record" && fail "dev-down under a preview identity touched the developer's project: $(cat "$STUB/preview.record")"
run_dev_down "" "$STUB/default.record"
grep -q -- '^compose -p g5-dev -f infra/docker/docker-compose.dev.yml --profile demo down -v' "$STUB/default.record" \
    || fail "dev-down default project must remain g5-dev: $(cat "$STUB/default.record")"
echo "✓ dev-down tears down only the selected stack and defaults to g5-dev"

# The compose file renders the identity into container names, volumes, image
# tag and ports when a Compose binary is available.
if docker compose version >/dev/null 2>&1; then
    render() { # <project> <volumes-prefix-or-empty> <tag> <edge-port>
        if [ -n "$2" ]; then
            env G5_COMPOSE_PROJECT="$1" G5_COMPOSE_VOLUMES="$2" G5_IMAGE_TAG="$3" G5_PORT_EDGE="$4" docker compose -p "$1" -f infra/docker/docker-compose.dev.yml config 2>/dev/null
        else
            env -u G5_COMPOSE_VOLUMES G5_COMPOSE_PROJECT="$1" G5_IMAGE_TAG="$3" G5_PORT_EDGE="$4" docker compose -p "$1" -f infra/docker/docker-compose.dev.yml config 2>/dev/null
        fi
    }
    preview="$(render g5-preview-test g5-preview-test preview 13000)"
    printf '%s\n' "$preview" | grep -q 'container_name: g5-preview-test-api' || fail "container names must follow the project"
    printf '%s\n' "$preview" | grep -q 'name: g5-preview-test_pgdata_dev' || fail "the database volume must follow the stack identity"
    printf '%s\n' "$preview" | grep -q 'g5/api:preview' || fail "the image tag must follow the stack identity"
    printf '%s\n' "$preview" | grep -q 'g5-dev-\|g5-lean_pgdata_dev' && fail "a preview render must not name the developer's resources"
    default="$(render g5-dev "" dev 3000)"
    printf '%s\n' "$default" | grep -q 'container_name: g5-dev-api' || fail "default container names changed"
    printf '%s\n' "$default" | grep -q 'name: g5-lean_pgdata_dev' || fail "default database volume name changed"
    echo "✓ compose renders the preview identity and keeps the defaults"
else
    echo "… docker compose unavailable; render check skipped"
fi

# The health wait resolves the interpolated container names.
expanded="$(G5_COMPOSE_PROJECT=g5-preview-test bash -c 'cname="container_name: ${G5_COMPOSE_PROJECT:-g5-dev}-api"; cname="${cname#*: }"; echo "${cname//\$\{G5_COMPOSE_PROJECT:-g5-dev\}/${G5_COMPOSE_PROJECT:-g5-dev}}"')"
[ "$expanded" = "g5-preview-test-api" ] || fail "health wait name expansion broke: $expanded"
grep -q 'G5_COMPOSE_PROJECT:-g5-dev' scripts/wait-for-healthy.sh || fail "wait-for-healthy.sh must expand the stack identity"
grep -q 'STACK="${G5_COMPOSE_PROJECT:-g5-dev}"' scripts/seed-dev-org.sh || fail "seed-dev-org.sh must follow the stack identity"
echo "✓ health wait and seed follow the stack identity"

# The devcontainer port forwarding follows the stack identity too (review
# R13): run the real script with recording docker/socat/pkill stubs.
FWD="$(mktemp -d)"
cat > "$FWD/docker" <<'STUBEOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${FWD_DOCKER:?}"
case "$2" in g5-dev-*) echo 10.20.0.1;; g5-preview-test-*) echo 10.20.0.2;; esac
STUBEOF
cat > "$FWD/socat" <<'STUBEOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${FWD_SOCAT:?}"
STUBEOF
printf '#!/usr/bin/env bash\nexit 0\n' > "$FWD/pkill"
chmod +x "$FWD/docker" "$FWD/socat" "$FWD/pkill"
run_forward() { # <project-or-empty> <edge-port> <api-port>
    : > "$FWD/docker.log"; : > "$FWD/socat.log"
    if [ -n "$1" ]; then
        PATH="$FWD:$PATH" FWD_DOCKER="$FWD/docker.log" FWD_SOCAT="$FWD/socat.log" G5_COMPOSE_PROJECT="$1" G5_PORT_EDGE="$2" G5_PORT_API="$3" bash scripts/forward-ports.sh >/dev/null
    else
        PATH="$FWD:$PATH" FWD_DOCKER="$FWD/docker.log" FWD_SOCAT="$FWD/socat.log" env -u G5_COMPOSE_PROJECT -u G5_PORT_EDGE -u G5_PORT_API bash scripts/forward-ports.sh >/dev/null
    fi
}
run_forward g5-preview-test 13000 17500
grep -q 'inspect g5-preview-test-edge ' "$FWD/docker.log" && grep -q 'inspect g5-preview-test-api ' "$FWD/docker.log" \
    || fail "forwarding must inspect the preview's containers: $(cat "$FWD/docker.log")"
grep -q 'g5-dev-' "$FWD/docker.log" && fail "forwarding under a preview identity inspected the developer's containers: $(cat "$FWD/docker.log")"
grep -q 'TCP-LISTEN:13000,fork,reuseaddr TCP:10.20.0.2:80' "$FWD/socat.log" || fail "preview edge port must forward to the preview container: $(cat "$FWD/socat.log")"
grep -q 'TCP-LISTEN:17500,fork,reuseaddr TCP:10.20.0.2:7500' "$FWD/socat.log" || fail "preview API port must forward to the preview container: $(cat "$FWD/socat.log")"
run_forward "" 3000 7500
grep -q 'inspect g5-dev-edge ' "$FWD/docker.log" || fail "default forwarding must still target g5-dev: $(cat "$FWD/docker.log")"
grep -q 'TCP-LISTEN:3000,fork,reuseaddr TCP:10.20.0.1:80' "$FWD/socat.log" || fail "default edge forwarding changed: $(cat "$FWD/socat.log")"
rm -rf "$FWD"
echo "✓ port forwarding follows the stack identity and defaults to g5-dev"

# The startup recipe keeps the exported identity through the workspace dotenv
# (review R22): run the real `dev-stack-up` in a fixture checkout with the
# profile's own exports, a recording `docker` that stops at the first Compose
# command, and stubs for the secret bundle and the nested base-image build.
FIX="$(mktemp -d)"
cp Justfile "$FIX/Justfile"
mkdir -p "$FIX/scripts/lib" "$FIX/stubs"
cp scripts/lib/stack-identity.sh "$FIX/scripts/lib/"
cp scripts/dev-workspace-image-env.sh "$FIX/scripts/"
printf 'g5_load_local_stack_secrets() { :; }\n' > "$FIX/scripts/lib/local-stack-secrets.sh"
JUST_BIN="$(command -v just)"
# Nested recipes: the stubbed `just` skips the base-image build and the host
# stop, and runs the real containers-only teardown so `dev-down` is observed
# through it.
printf '#!/usr/bin/env bash\ncase "$1" in docker-build-common-base|worker-host-stop) exit 0;; dev-stack-down) exec %s "$@";; *) exit 99;; esac\n' "$JUST_BIN" > "$FIX/stubs/just"
cat > "$FIX/stubs/docker" <<'STUBEOF'
#!/usr/bin/env bash
{
    printf 'args=%s\n' "$*"
    for key in G5_COMPOSE_PROJECT G5_COMPOSE_VOLUMES G5_IMAGE_TAG G5_PORT_EDGE G5_PORT_API G5_PORT_POSTGRES G5_PORT_OAUTH_STUB G5_ALLOW_PROCESS_ISOLATION_HOSTS; do
        printf '%s=%s\n' "$key" "${!key:-}"
    done
} >> "${FIX_RECORD:?}"
# Image builds, the deferred edge removal and the edge-only start succeed so
# the recipes reach their next step; every other Compose command stops the
# recipe there. FIX_FAIL_EDGE_RM=1 makes the edge removal fail instead.
case "$*" in
    *" rm -sf edge") [ -n "${FIX_FAIL_EDGE_RM:-}" ] && exit 42 || exit 0 ;;
    *" build "*|*"up -d --no-build --no-deps edge") exit 0 ;;
esac
exit 42
STUBEOF
printf '#!/usr/bin/env bash\n[ -n "${FIX_FAIL_HEALTH:-}" ] && exit 1\nexit 0\n' > "$FIX/scripts/wait-for-healthy.sh"
printf '#!/usr/bin/env bash\nexit 0\n' > "$FIX/scripts/wait-for-worker-host.sh"
chmod +x "$FIX/scripts/wait-for-healthy.sh" "$FIX/scripts/wait-for-worker-host.sh"
chmod +x "$FIX/stubs/just" "$FIX/stubs/docker"
profile_exports="${COMMAND#\'}"
profile_exports="${profile_exports%%; if command -v*};"
run_recipe() { # <recipe> <shell prelude> <dotenv-contents> [exported identity...]
    local recipe="$1" prelude="$2"
    printf '%s' "$3" > "$FIX/.env"; shift 3
    : > "$FIX/record"
    (
        cd "$FIX"
        env -u G5_COMPOSE_PROJECT -u G5_COMPOSE_VOLUMES -u G5_IMAGE_TAG -u G5_PORT_EDGE -u G5_PORT_API -u G5_PORT_POSTGRES -u G5_PORT_OAUTH_STUB \
            "$@" FIX_RECORD="$FIX/record" G5_NEW_ORG_DEFAULT_IMAGE_TRACK=probe G5_PROMOTED_BASE_IMAGES=probe=fixture \
            bash -c "$prelude export PATH=\"$FIX/stubs:\$PATH\"; exec \"$JUST_BIN\" $recipe" >/dev/null 2>&1 || true
    )
}
run_stack_up() { run_recipe dev-stack-up "$@"; }
preview_identity=(G5D_PROCESS_INCARNATION=test G5_PORT_EDGE=13000 G5_PORT_API=17500 G5_PORT_POSTGRES=15433 G5_PORT_OAUTH_STUB=19876 G5_IMAGE_TAG=preview)
developer_dotenv=$'G5_COMPOSE_PROJECT=g5-dev\nG5_COMPOSE_VOLUMES=g5-lean\nG5_IMAGE_TAG=dev\nG5_PORT_EDGE=3000\nG5_PORT_API=7500\nG5_PORT_POSTGRES=5433\nG5_PORT_OAUTH_STUB=9876\n'
for dotenv in "" "$(cat .env.example)" "$developer_dotenv"; do
    run_stack_up "$profile_exports" "$dotenv" env "${preview_identity[@]}"
    grep -q '^args=compose -p g5-preview-test ' "$FIX/record" || fail "startup under the preview identity must address the preview project: $(cat "$FIX/record")"
    grep -q '^G5_COMPOSE_VOLUMES=g5-preview-test$' "$FIX/record" || fail "preview volumes must survive the dotenv: $(cat "$FIX/record")"
    grep -q '^G5_IMAGE_TAG=preview$' "$FIX/record" || fail "the preview image tag must survive the dotenv: $(cat "$FIX/record")"
    grep -q '^G5_PORT_POSTGRES=15433$' "$FIX/record" || fail "the preview Postgres port must survive the dotenv: $(cat "$FIX/record")"
    grep -q '^G5_PORT_EDGE=13000$' "$FIX/record" || fail "the preview edge port must survive the dotenv: $(cat "$FIX/record")"
    # The microvm profile selects no process mode, so its previewed API admits
    # no process host; that admission opens only inside the process sibling.
    grep -q '^G5_ALLOW_PROCESS_ISOLATION_HOSTS=false$' "$FIX/record" || fail "the microvm profile's API must not admit process hosts: $(cat "$FIX/record")"
done
process_exports="${PCOMMAND#\'}"
process_exports="${process_exports%%; if command -v*};"
for dotenv in "" "$(cat .env.example)" "$developer_dotenv"; do
    run_stack_up "$process_exports" "$dotenv" env "${preview_identity[@]}"
    grep -q '^args=compose -p g5-preview-test ' "$FIX/record" || fail "the process sibling must address the preview project: $(cat "$FIX/record")"
    # The sibling selects process mode for the previewed host, so the previewed
    # API admits it; nothing else opens that admission.
    grep -q '^G5_ALLOW_PROCESS_ISOLATION_HOSTS=true$' "$FIX/record" || fail "the previewed API must admit the process host the sibling starts: $(cat "$FIX/record")"
done
# Without an exported identity the developer's dotenv keeps working.
run_stack_up "" "$(cat .env.example)" env
grep -q '^args=compose -p g5-dev ' "$FIX/record" || fail "a plain start must address the default project: $(cat "$FIX/record")"
grep -q '^G5_PORT_POSTGRES=5433$' "$FIX/record" || fail "a plain start must honour the developer's dotenv port: $(cat "$FIX/record")"
grep -q '^G5_ALLOW_PROCESS_ISOLATION_HOSTS=false$' "$FIX/record" || fail "a plain start must not admit process hosts: $(cat "$FIX/record")"
# An ambient or dotenv admission is never honoured: only the host's own mode selection opens it.
run_stack_up "" $'G5_ALLOW_PROCESS_ISOLATION_HOSTS=true\n' env G5_ALLOW_PROCESS_ISOLATION_HOSTS=true
grep -q '^G5_ALLOW_PROCESS_ISOLATION_HOSTS=false$' "$FIX/record" || fail "an ambient admission must be ignored: $(cat "$FIX/record")"
run_stack_up "" "" env G5D_ISOLATION_MODE=process
grep -q '^G5_ALLOW_PROCESS_ISOLATION_HOSTS=false$' "$FIX/record" || fail "process mode without the acknowledgement must not open the admission: $(cat "$FIX/record")"
echo "✓ startup keeps the exported stack identity through the workspace dotenv and derives process-host admission"

# The edge is deferred only when asked (dev-up publishes it last): a deferred
# start removes any previous edge first, so a rerun never keeps publishing the
# UI while the API is replaced and the host restarts; the edge-only phase
# starts the edge alone with --no-deps and never rebuilds.
run_stack_up "" "$developer_dotenv" env
grep -q 'up -d --no-build --remove-orphans postgres oauth-stub g5-api g5-daemon edge$' "$FIX/record" || fail "a plain start must bring the edge up with the stack: $(cat "$FIX/record")"
grep -q ' rm -sf edge$' "$FIX/record" && fail "a plain start must not remove the edge: $(cat "$FIX/record")"
run_stack_up "" "$developer_dotenv" env G5_DEV_DEFER_EDGE=1
grep -q ' rm -sf edge$' "$FIX/record" || fail "a deferred start must remove a previous edge: $(cat "$FIX/record")"
grep -q 'up -d --no-build --remove-orphans postgres oauth-stub g5-api g5-daemon$' "$FIX/record" || fail "a deferred start must leave the edge out: $(cat "$FIX/record")"
grep -q 'up -d.* edge' "$FIX/record" && fail "a deferred start must not start the edge: $(cat "$FIX/record")"
# A removal that fails must stop the start before the API is replaced.
run_stack_up "" "$developer_dotenv" env G5_DEV_DEFER_EDGE=1 FIX_FAIL_EDGE_RM=1
grep -q ' rm -sf edge$' "$FIX/record" || fail "the failing removal must have been attempted: $(cat "$FIX/record")"
grep -q 'up -d' "$FIX/record" && fail "a failed edge removal must not be followed by an up: $(cat "$FIX/record")"
# The withdraw phase removes the edge and nothing else.
run_stack_up "" "$developer_dotenv" env G5_DEV_WITHDRAW_EDGE=1
grep -q ' rm -sf edge$' "$FIX/record" || fail "the withdraw phase must remove the edge: $(cat "$FIX/record")"
[ "$(grep -c '^args=' "$FIX/record")" = 1 ] || fail "the withdraw phase must run nothing but the removal: $(cat "$FIX/record")"
run_stack_up "" "$developer_dotenv" env G5_DEV_EDGE_ONLY=1
grep -q 'up -d --no-build --no-deps edge$' "$FIX/record" || fail "the edge-only phase must start the edge with --no-deps: $(cat "$FIX/record")"
grep -q ' build ' "$FIX/record" && fail "the edge-only phase must not build images: $(cat "$FIX/record")"
grep -q 'remove-orphans' "$FIX/record" && fail "the edge-only phase must not touch the other services: $(cat "$FIX/record")"
# An edge that never becomes healthy is removed again rather than left
# published on its port.
run_stack_up "" "$developer_dotenv" env G5_DEV_EDGE_ONLY=1 FIX_FAIL_HEALTH=1
[ "$(grep -c '^args=' "$FIX/record")" = 2 ] && sed -n 1p "$FIX/record" | grep -q 'up -d --no-build --no-deps edge$' && grep -q ' rm -sf edge$' "$FIX/record" || fail "an unhealthy edge must be removed after the failed wait: $(cat "$FIX/record")"
echo "✓ dev-stack-up defers, removes and republishes the edge only when asked"

# Restart and teardown select the stack exactly as startup did (review R24):
# the exported identity first, then the dotenv's, then the defaults.
for recipe in dev-restart-backend dev-restart-frontend dev-down; do
    run_recipe "$recipe" "$profile_exports" "$developer_dotenv" env "${preview_identity[@]}"
    grep -q '^args=compose -p g5-preview-test ' "$FIX/record" || fail "$recipe under the preview identity must address the preview project: $(cat "$FIX/record")"
    grep -q '^G5_COMPOSE_VOLUMES=g5-preview-test$' "$FIX/record" || fail "$recipe must keep the preview volumes: $(cat "$FIX/record")"
    grep -q '^G5_PORT_POSTGRES=15433$' "$FIX/record" || fail "$recipe must keep the preview Postgres port: $(cat "$FIX/record")"
done
custom_dotenv=$'G5_COMPOSE_PROJECT=g5-custom\nG5_COMPOSE_VOLUMES=g5-custom\n'
for recipe in dev-stack-up dev-restart-backend dev-restart-frontend dev-down; do
    run_recipe "$recipe" "" "$custom_dotenv" env
    grep -q '^args=compose -p g5-custom ' "$FIX/record" || fail "$recipe must follow the dotenv's stack identity when none is exported: $(cat "$FIX/record")"
    grep -q '^G5_COMPOSE_VOLUMES=g5-custom$' "$FIX/record" || fail "$recipe must keep the dotenv's volume prefix: $(cat "$FIX/record")"
done
run_recipe dev-down "" "$developer_dotenv" env
grep -q '^args=compose -p g5-dev -f infra/docker/docker-compose.dev.yml --profile demo down -v' "$FIX/record" || fail "a plain dev-down must still tear down g5-dev: $(cat "$FIX/record")"
echo "✓ restart and teardown select the same stack as startup"

# The optional local overlays select the identity for every recipe alike
# (review R26): at their default names and at their configurable paths.
overlay_case() { # <description> <overlay path> [env assignments...]
    local description="$1" overlay="$2"; shift 2
    printf '%s' "$custom_dotenv" > "$overlay"
    for recipe in dev-stack-up dev-restart-backend dev-restart-frontend dev-down demo-down; do
        run_recipe "$recipe" "" "$developer_dotenv" env "$@"
        grep -q '^args=compose -p g5-custom ' "$FIX/record" || fail "$recipe must follow $description: $(cat "$FIX/record")"
        grep -q '^G5_COMPOSE_VOLUMES=g5-custom$' "$FIX/record" || fail "$recipe must keep the volume prefix of $description: $(cat "$FIX/record")"
    done
    rm -f "$overlay"
}
overlay_case "the platform-issue overlay" "$FIX/.env.platform-issue.local"
overlay_case "the GitHub App overlay" "$FIX/.env.github-app.local"
overlay_case "a configured platform-issue overlay path" "$FIX/custom-platform.env" G5_DEV_PLATFORM_ISSUE_ENV_FILE="$FIX/custom-platform.env"
overlay_case "a configured GitHub App overlay path" "$FIX/custom-github.env" G5_DEV_GITHUB_APP_ENV_FILE="$FIX/custom-github.env"
# An exported identity still wins over an overlay.
printf '%s' "$custom_dotenv" > "$FIX/.env.github-app.local"
run_recipe dev-stack-up "$profile_exports" "$developer_dotenv" env "${preview_identity[@]}"
grep -q '^args=compose -p g5-preview-test ' "$FIX/record" || fail "an exported identity must win over an overlay: $(cat "$FIX/record")"
rm -f "$FIX/.env.github-app.local"
echo "✓ optional overlays select the same stack for every recipe"

# The worker host joins the selected stack (review R29): the worker recipes
# resolve the identity before the helper derives the API address from it.
cat > "$FIX/scripts/worker-host-env.sh" <<'STUBEOF'
printf 'G5_PORT_API=%s\n' "${G5_PORT_API:-}" > "${FIX_RECORD:?}"
exit 42
STUBEOF
for recipe in worker-host-start worker-host worker-host-stop; do
    run_recipe "$recipe" "$profile_exports" "$developer_dotenv" env "${preview_identity[@]}"
    grep -q '^G5_PORT_API=17500$' "$FIX/record" || fail "$recipe must hand the helper the selected API port: $(cat "$FIX/record")"
    printf '%s' "$custom_dotenv" > "$FIX/.env.github-app.local"
    run_recipe "$recipe" "" $'G5_PORT_API=7500\n' env
    rm -f "$FIX/.env.github-app.local"
    grep -q '^G5_PORT_API=7500$' "$FIX/record" || fail "$recipe must keep the dotenv's API port when none is exported: $(cat "$FIX/record")"
done
echo "✓ worker recipes join the selected stack"

# dev-up discovers its extra worker organizations from the selected stack's
# API (review R33): run the real recipe with recording `just` and `curl`
# stubs whose roster differs per port.
cat > "$FIX/stubs/just" <<'STUBEOF'
#!/usr/bin/env bash
printf '%s port=%s\n' "$*" "${G5_PORT_API:-}" >> "${FIX_ROSTER:?}"
exit 0
STUBEOF
cat > "$FIX/stubs/curl" <<'STUBEOF'
#!/usr/bin/env bash
url=""
for arg in "$@"; do case "$arg" in http*) url="$arg";; esac; done
printf '%s\n' "$url" >> "${FIX_CURL:?}"
case "$url" in
    *test-token) printf '{"data":{"jwt":"fixture"}}\n' ;;
    *:17500/*) printf '{"data":[{"orgSlug":"g5labs"},{"orgSlug":"preview-org"}]}\n' ;;
    *) printf '{"data":[{"orgSlug":"g5labs"},{"orgSlug":"developer-org"}]}\n' ;;
esac
STUBEOF
chmod +x "$FIX/stubs/just" "$FIX/stubs/curl"
if command -v jq >/dev/null 2>&1; then
    run_dev_up() { # <shell prelude> <dotenv> [exported identity...]
        : > "$FIX/roster"; : > "$FIX/curl"
        FIX_ROSTER="$FIX/roster" FIX_CURL="$FIX/curl" run_recipe dev-up "$@"
    }
    roster_dotenv="$developer_dotenv"$'G5_WORKER_ORGS=preview-org,developer-org\n'
    run_dev_up "$profile_exports" "$roster_dotenv" env "${preview_identity[@]}"
    [ "$(grep -c ':17500/' "$FIX/curl")" = 2 ] || fail "discovery must query the selected stack's API: $(cat "$FIX/curl")"
    grep -q '^worker-host-start preview-org port=17500$' "$FIX/roster" || fail "an org present in the selected stack must get its worker: $(cat "$FIX/roster")"
    grep -q 'developer-org' "$FIX/roster" && fail "an org absent from the selected stack must be skipped: $(cat "$FIX/roster")"
    run_dev_up "" "$roster_dotenv" env
    [ "$(grep -c ':7500/' "$FIX/curl")" = 2 ] || fail "a plain dev-up must query the default API: $(cat "$FIX/curl")"
    grep -q '^worker-host-start developer-org port=7500$' "$FIX/roster" || fail "a plain dev-up must start the developer stack's org worker: $(cat "$FIX/roster")"
    printf 'G5_PORT_API=17500\n' > "$FIX/.env.github-app.local"
    run_dev_up "" "$roster_dotenv" env
    rm -f "$FIX/.env.github-app.local"
    [ "$(grep -c ':17500/' "$FIX/curl")" = 2 ] || fail "an overlay's API port must select the discovery API: $(cat "$FIX/curl")"
    run_dev_up "$profile_exports" "$roster_dotenv" env "${preview_identity[@]}" G5_WORKER_HOST_API_URL=http://localhost:27500
    [ "$(grep -c ':27500/' "$FIX/curl")" = 2 ] || fail "the dedicated worker API override must still win: $(cat "$FIX/curl")"
    echo "✓ dev-up discovers its worker organizations from the selected stack"
else
    echo "… jq unavailable; dev-up roster check skipped"
fi
rm -rf "$FIX"
echo "preview profiles: OK"
