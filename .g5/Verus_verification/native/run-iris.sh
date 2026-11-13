#!/usr/bin/env bash
# Native IRIS runner for the IPM Verus uplift.
#
#   run-iris.sh up                      export the pinned product, start the container, wait until healthy
#   run-iris.sh exec LABEL SCRIPT.cos   run an ObjectScript script in USER and save a receipt.
#                                       The script must end with: write !,"NATIVE-END",!  then halt.
#   run-iris.sh purge-journals          delete IRIS journal files inside the container to save disk
#   run-iris.sh down                    remove the container
#
# Every exec writes results/native/<UTC>-<LABEL>/ with the script, raw log, command and receipt.json,
# and inputs/ with a snapshot of native/cases/*.json (the expectations) taken before the session starts.
# A watchdog stops the session when host free space drops below FLOOR_MB and records outcome RESOURCE.
# A run that is stopped, times out or exits abnormally is an incomplete check, never a pass.
set -euo pipefail

PRODUCT_REV=571e04a19aba9f9b4bc0ea23fbe09182038f4e55
PRODUCT_TREE=9840c894a84a289d470f4447fa5f54f303741050
BASE_IMAGE=containers.intersystems.com/intersystems/iris-community:2026.1
IMAGE=ipm-verus-uplift-20261113:571e04a1
NAME=${IRIS_CONTAINER:-ipm-verus-uplift-iris}
ORAS_NAME=${ORAS_CONTAINER:-ipm-verus-uplift-oras}
ORAS_IMAGE=ghcr.io/project-zot/zot-linux-amd64@sha256:b68f727fe35c1eb45b2ae91519dfb0e33192d69b5104882c3e7809b7146c4a02
NET=${IRIS_NETWORK:-ipm-verus-uplift-net}
WORK=${IPM_UPLIFT_WORK:-/tmp/ipm-verus-uplift}
EXPORT="$WORK/product-571e04a1"
FLOOR_MB=${FLOOR_MB:-1500}
TIMEOUT_S=${TIMEOUT_S:-1800}

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
results="$here/../results/native"

free_mb() { df --output=avail -m / | tail -1 | tr -d ' '; }

export_product() {
    rm -rf "$EXPORT"
    mkdir -p "$EXPORT"
    git -C "$repo" archive "$PRODUCT_REV" | tar -x -C "$EXPORT"
    chmod -R a+rwX "$EXPORT"
    # Native scripts are visible in the container at /home/irisowner/zpm/.native/
    mkdir -p "$EXPORT/.native"
    cp -r "$here/scripts/." "$EXPORT/.native/" 2>/dev/null || true
    [ -d "$here/fixtures" ] && cp -r "$here/fixtures" "$EXPORT/.native/"
    chmod -R a+rwX "$EXPORT/.native"
}

cmd_up() {
    [ "$(git -C "$repo" rev-parse "$PRODUCT_REV^{tree}")" = "$PRODUCT_TREE" ] || { echo "tree mismatch" >&2; exit 1; }
    docker image inspect "$IMAGE" >/dev/null 2>&1 || {
        echo "image $IMAGE missing; build it with: git archive $PRODUCT_REV | tar -x -C DIR && chmod -R a+rX DIR && docker build -t $IMAGE DIR" >&2
        exit 1
    }
    export_product
    # ORAS registry used by the integration tests, reachable as http://oras:5000 like docker-compose.yml.
    docker network inspect "$NET" >/dev/null 2>&1 || docker network create "$NET" >/dev/null
    docker rm -f "$ORAS_NAME" >/dev/null 2>&1 || true
    docker run -d --name "$ORAS_NAME" --network "$NET" --network-alias oras \
        -v "$EXPORT/tests/oras/zot-config.json:/etc/zot/config.json:ro" \
        -v "$EXPORT/tests/oras/htpasswd:/etc/zot/htpasswd:ro" \
        "$ORAS_IMAGE" >/dev/null
    printf '[config]\nglobals=0,0,1024,0,0,0\nroutines=128\n' > "$WORK/merge.cpf"
    chmod a+r "$WORK/merge.cpf"
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker run -d --name "$NAME" --memory 10g --network "$NET" \
        -e ISC_CPF_MERGE_FILE=/home/irisowner/merge.cpf \
        -v "$WORK/merge.cpf:/home/irisowner/merge.cpf:ro" \
        -v "$EXPORT:/home/irisowner/zpm/" \
        "$IMAGE" \
        -a "iris session iris -U%SYS '##class(Security.Users).UnExpireUserPasswords(\"*\")'" >/dev/null
    for _ in $(seq 1 100); do
        s=$(docker inspect -f '{{.State.Health.Status}}' "$NAME" 2>/dev/null || true)
        [ "$s" = healthy ] && { echo "up: $NAME healthy"; return 0; }
        sleep 3
    done
    echo "container did not become healthy" >&2
    exit 1
}

cmd_exec() {
    local label="$1" script="$2"
    [ -f "$script" ] || { echo "no script $script" >&2; exit 2; }
    local run_id
    run_id="$(date -u +%Y%m%dT%H%M%SZ)-$label"
    local out="$results/$run_id"
    mkdir -p "$out"
    cp "$script" "$out/input.cos"
    # Snapshot the expectations as they are when the run starts, so a verdict can show they preceded it.
    if [ -d "$here/cases" ]; then
        mkdir -p "$out/inputs"
        cp "$here"/cases/*.json "$out/inputs/" 2>/dev/null || true
    fi
    # Refresh native scripts inside the export so the container sees the current helper classes.
    mkdir -p "$EXPORT/.native" && cp -r "$here/scripts/." "$EXPORT/.native/"
    [ -d "$here/fixtures" ] && cp -r "$here/fixtures" "$EXPORT/.native/"
    chmod -R a+rwX "$EXPORT/.native"
    local cmd="docker exec -i $NAME iris session IRIS -U USER < input.cos"
    echo "$cmd" > "$out/command.txt"
    local start free0
    start=$(date -u +%FT%TZ); free0=$(free_mb)
    if [ "$free0" -lt "$FLOOR_MB" ]; then
        outcome=RESOURCE; rc=-1
        echo "not started: free ${free0}MB below floor ${FLOOR_MB}MB" > "$out/raw.log"
    else
        set +e
        timeout "$TIMEOUT_S" docker exec -i "$NAME" iris session IRIS -U USER < "$script" > "$out/raw.log" 2>&1 &
        local pid=$!
        local stopped=""
        while kill -0 "$pid" 2>/dev/null; do
            if [ "$(free_mb)" -lt "$FLOOR_MB" ]; then
                stopped="disk"; kill "$pid" 2>/dev/null
                docker exec "$NAME" bash -c 'pkill -f "irisdb.*-U USER" || true' >/dev/null 2>&1
                break
            fi
            sleep 2
        done
        wait "$pid"; rc=$?
        set -e
        # A script must print NATIVE-END as its last action; without it the run did not finish.
        if [ -n "$stopped" ]; then outcome=RESOURCE
        elif [ "$rc" = 124 ]; then outcome=TIMEOUT
        elif ! grep -aq '^NATIVE-END' "$out/raw.log"; then outcome=INCOMPLETE
        elif [ "$rc" = 0 ]; then outcome=COMPLETED
        else outcome=ERROR
        fi
    fi
    local end; end=$(date -u +%FT%TZ)
    local digest; digest=$(docker image inspect "$IMAGE" -f '{{.Id}}' 2>/dev/null || echo unknown)
    local base; base=$(docker image inspect "$BASE_IMAGE" -f '{{index .RepoDigests 0}}' 2>/dev/null || echo unknown)
    local script_sha; script_sha=$(sha256sum "$script" | cut -d' ' -f1)
    local helpers_sha; helpers_sha=$( (cd "$here/scripts" && find . -type f -print0 | sort -z | xargs -0 sha256sum) | sha256sum | cut -d' ' -f1) || true
    local expect_sha; expect_sha=$( (cd "$out/inputs" 2>/dev/null && find . -type f -print0 | sort -z | xargs -0 sha256sum) | sha256sum | cut -d' ' -f1) || true
    cat > "$out/receipt.json" <<EOF
{
  "run_id": "$run_id",
  "label": "$label",
  "process_outcome": "$outcome",
  "exit_code": $rc,
  "started_utc": "$start",
  "ended_utc": "$end",
  "free_mb_at_start": $free0,
  "floor_mb": $FLOOR_MB,
  "timeout_s": $TIMEOUT_S,
  "product_rev": "$PRODUCT_REV",
  "product_tree": "$PRODUCT_TREE",
  "image": "$IMAGE",
  "image_id": "$digest",
  "base_image": "$base",
  "container": "$NAME",
  "script_sha256": "$script_sha",
  "helpers_sha256": "$helpers_sha",
  "expectations_snapshot_sha256": "$expect_sha",
  "substantive_verdict": "see verdict.json written by the check's evaluator; process_outcome alone is not a verdict"
}
EOF
    echo "$run_id $outcome rc=$rc log=$out/raw.log"
}

cmd_purge() {
    docker exec -i "$NAME" iris session IRIS -U %SYS <<'EOF' >/dev/null 2>&1 || true
do ##class(%SYS.Journal.System).SwitchFile()
do ##class(%SYS.Journal.File).PurgeAll()
halt
EOF
    echo "journals purged; container size: $(docker ps -s --filter name=$NAME --format '{{.Size}}')"
}

case "${1:-}" in
    up) cmd_up ;;
    exec) shift; cmd_exec "$@" ;;
    purge-journals) cmd_purge ;;
    down) docker rm -f "$NAME" "$ORAS_NAME"; docker network rm "$NET" 2>/dev/null || true ;;
    *) sed -n '2,12p' "$0"; exit 2 ;;
esac
