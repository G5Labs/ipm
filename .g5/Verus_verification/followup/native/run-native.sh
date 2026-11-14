#!/usr/bin/env bash
# Native IRIS runner for the follow-up review. Uses only the names this session was assigned.
#
#   run-native.sh up                      export the pinned product, start this session's IRIS container
#   run-native.sh exec LABEL SCRIPT.cos   run SCRIPT in USER; write followup/results/native/<UTC>-<LABEL>/
#   run-native.sh down                    remove this session's container and network
#
# Required environment: IRIS_CONTAINER, IRIS_NETWORK, IPM_UPLIFT_WORK. There is no default, so the
# runner cannot touch another workload's container. The image is the original uplift's product image,
# used read-only. No ORAS registry is started: the follow-up cases load from the filesystem and the
# remote registry repository is disabled inside the session.
#
# Each exec snapshots, before the session starts, the helper classes, fixtures and expectation files
# into inputs/ and records their hashes in receipt.json. The run is COMPLETED only if the script
# printed NATIVE-END, exited 0 and did not time out.
set -euo pipefail

: "${IRIS_CONTAINER:?IRIS_CONTAINER must be set}"
: "${IRIS_NETWORK:?IRIS_NETWORK must be set}"
: "${IPM_UPLIFT_WORK:?IPM_UPLIFT_WORK must be set}"

PRODUCT_REV=571e04a19aba9f9b4bc0ea23fbe09182038f4e55
PRODUCT_TREE=9840c894a84a289d470f4447fa5f54f303741050
IMAGE=ipm-verus-uplift-20261113:571e04a1
NAME=$IRIS_CONTAINER
NET=$IRIS_NETWORK
WORK=$IPM_UPLIFT_WORK
EXPORT="$WORK/product-571e04a1"
TIMEOUT_S=${TIMEOUT_S:-1800}
FLOOR_MB=${FLOOR_MB:-1500}

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(git -C "$here" rev-parse --show-toplevel)"
results="$here/../results/native"

orig="$here/../../native"

stage() {
    mkdir -p "$EXPORT/.followup"
    rm -rf "$EXPORT/.followup/"*
    cp -r "$here/scripts" "$here/fixtures" "$EXPORT/.followup/"
    chmod -R a+rwX "$EXPORT/.followup"
    # NATIVE_ORIGINAL=1: also stage the original replay driver and fixtures at /home/irisowner/zpm/.native/,
    # where the original replay scripts expect them (committed sources, read-only).
    if [ "${NATIVE_ORIGINAL:-0}" = 1 ]; then
        rm -rf "$EXPORT/.native" && mkdir -p "$EXPORT/.native"
        cp -r "$orig/scripts/." "$EXPORT/.native/" && cp -r "$orig/fixtures" "$EXPORT/.native/"
        chmod -R a+rwX "$EXPORT/.native"
    fi
}

cmd_up() {
    [ "$(git -C "$repo" rev-parse "$PRODUCT_REV^{tree}")" = "$PRODUCT_TREE" ] || { echo "tree mismatch" >&2; exit 1; }
    docker image inspect "$IMAGE" >/dev/null 2>&1 || { echo "image $IMAGE missing" >&2; exit 1; }
    mkdir -p "$WORK"
    rm -rf "$EXPORT"
    mkdir -p "$EXPORT"
    git -C "$repo" archive "$PRODUCT_REV" | tar -x -C "$EXPORT"
    chmod -R a+rwX "$EXPORT"
    stage
    docker network inspect "$NET" >/dev/null 2>&1 || docker network create "$NET" >/dev/null
    if docker container inspect "$NAME" >/dev/null 2>&1; then
        [ "$(docker inspect -f '{{index .Config.Labels "ipm-quality-followup"}}' "$NAME")" = 1 ] || { echo "$NAME exists and is not ours" >&2; exit 1; }
        docker rm -f "$NAME" >/dev/null
    fi
    docker run -d --name "$NAME" --label ipm-quality-followup=1 --memory 4g --network "$NET" \
        -v "$EXPORT:/home/irisowner/zpm/" "$IMAGE" \
        -a "iris session iris -U%SYS '##class(Security.Users).UnExpireUserPasswords(\"*\")'" >/dev/null
    for _ in $(seq 1 100); do
        s=$(docker inspect -f '{{.State.Health.Status}}' "$NAME" 2>/dev/null || true)
        [ "$s" = healthy ] && { echo "up: $NAME healthy on $NET"; return 0; }
        sleep 3
    done
    echo "container did not become healthy" >&2
    exit 1
}

ours() {
    docker container inspect "$NAME" >/dev/null 2>&1 && [ "$(docker inspect -f '{{index .Config.Labels "ipm-quality-followup"}}' "$NAME")" = 1 ]
}

cmd_exec() {
    local label="$1" script="$2"
    [ -f "$script" ] || { echo "no script $script" >&2; exit 2; }
    ours || { echo "$NAME is missing or not labeled ipm-quality-followup=1" >&2; exit 1; }
    local run_id out
    run_id="$(date -u +%Y%m%dT%H%M%SZ)-$label"
    out="$results/$run_id"
    mkdir -p "$out/inputs"
    cp "$script" "$out/input.cos"
    cp -r "$here/scripts" "$here/cases" "$out/inputs/"
    local orig_helpers_sha=""
    if [ "${NATIVE_ORIGINAL:-0}" = 1 ]; then
        mkdir -p "$out/inputs/original-scripts"
        cp -r "$orig/scripts/." "$out/inputs/original-scripts/"
        cp "$orig/cases/replay-binding.json" "$orig"/cases/b1-cases-*.json "$out/inputs/"
        orig_helpers_sha=$( (cd "$orig/scripts" && find . -type f -print0 | sort -z | xargs -0 sha256sum) | sha256sum | cut -d' ' -f1)
    fi
    (cd "$here" && find fixtures -type f -print0 | sort -z | xargs -0 sha256sum) > "$out/inputs/fixtures.sha256"
    local helpers_sha cases_sha fixtures_sha
    helpers_sha=$( (cd "$here/scripts" && find . -type f -print0 | sort -z | xargs -0 sha256sum) | sha256sum | cut -d' ' -f1)
    cases_sha=$( (cd "$here/cases" && find . -type f -print0 | sort -z | xargs -0 sha256sum) | sha256sum | cut -d' ' -f1)
    fixtures_sha=$(sha256sum "$out/inputs/fixtures.sha256" | cut -d' ' -f1)
    stage
    echo "docker exec -i $NAME iris session IRIS -U USER < input.cos" > "$out/command.txt"
    local start free0 rc outcome
    start=$(date -u +%FT%TZ); free0=$(df --output=avail -m / | tail -1 | tr -d ' ')
    if [ "$free0" -lt "$FLOOR_MB" ]; then
        outcome=RESOURCE; rc=-1; echo "not started: free ${free0}MB" > "$out/raw.log"
    else
        set +e
        timeout "$TIMEOUT_S" docker exec -i "$NAME" iris session IRIS -U USER < "$script" > "$out/raw.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" = 124 ]; then outcome=TIMEOUT
        elif ! grep -aq '^NATIVE-END' "$out/raw.log"; then outcome=INCOMPLETE
        elif [ "$rc" = 0 ]; then outcome=COMPLETED
        else outcome=ERROR
        fi
    fi
    cat > "$out/receipt.json" <<EOF
{
  "run_id": "$run_id",
  "label": "$label",
  "process_outcome": "$outcome",
  "exit_code": $rc,
  "started_utc": "$start",
  "ended_utc": "$(date -u +%FT%TZ)",
  "product_rev": "$PRODUCT_REV",
  "image": "$IMAGE",
  "image_id": "$(docker image inspect "$IMAGE" -f '{{.Id}}')",
  "container": "$NAME",
  "network": "$NET",
  "script_sha256": "$(sha256sum "$script" | cut -d' ' -f1)",
  "helpers_sha256_at_start": "$helpers_sha",
  "expectations_sha256_at_start": "$cases_sha",
  "fixtures_sha256_at_start": "$fixtures_sha",
  "original_helpers_sha256_at_start": "$orig_helpers_sha",
  "original_helpers_git_revision": "$(git -C "$repo" log -1 --format=%H -- .g5/Verus_verification/native/scripts)",
  "substantive_verdict": "see the evaluator's verdict file; process_outcome alone is not a verdict"
}
EOF
    echo "$run_id $outcome rc=$rc"
}

case "${1:-}" in
    up) cmd_up ;;
    exec) shift; cmd_exec "$@" ;;
    down)
        if ours; then
            docker rm -f "$NAME" >/dev/null
        fi
        # The network name is assigned to this session. Remove it only when nothing is attached to it.
        if docker network inspect "$NET" >/dev/null 2>&1 && [ "$(docker network inspect -f '{{len .Containers}}' "$NET")" = 0 ]; then
            docker network rm "$NET" >/dev/null
        fi
        echo "down: $NAME" ;;
    *) sed -n '2,12p' "$0"; exit 2 ;;
esac
