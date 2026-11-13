#!/usr/bin/env bash
# Executable contract for the optional registry mirror in
# scripts/ensure-base-images.sh (G5_BASE_IMAGE_REGISTRY; deployment
# g5.quality.delivery.ci.base-image-mirror).
#
# Runs the REAL script against a PATH-shadowed `docker` that keeps a scratch
# image table, and asserts:
#   1. no registry configured  -> no pull, no push, local build (unchanged path)
#   2. mirror miss             -> local build, then push of <prefix>/g5-<tier>:<key>
#   3. mirror hit              -> pull + tag, no build, no push
#   4. mirror hit with a wrong freshness label -> falls back to a local build
#   5. the child key changes when the parent recipe changes (lineage folded in)
#
# Run by the ci-rust `image-build-script-contracts` step; also runnable
# locally (needs only bash + coreutils; no Docker).
set -euo pipefail
cd "$(dirname "$0")/.."

SCRIPT="$PWD/scripts/ensure-base-images.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

mkdir -p "$TEST_ROOT/bin" "$TEST_ROOT/images" "$TEST_ROOT/ctx/infra/docker"
# Minimal tier recipes; only their bytes matter to the freshness digest.
printf 'FROM ubuntu\nRUN echo base\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base"
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo node\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-node"

# Fake docker: images live as files under $FAKE_IMAGES/<name-with-slashes-encoded>
# holding "label=<sha>" and "layers=<ids>". `build` stamps the label from
# --label and synthesizes layers as parent-layers + a fresh id; `pull`
# succeeds only for names present in $FAKE_REGISTRY; `push` copies into it.
cat >"$TEST_ROOT/bin/docker" <<'FAKE'
#!/usr/bin/env bash
set -u
enc() { printf '%s' "$1" | tr '/:' '__'; }
log() { printf '%s\n' "$*" >> "$FAKE_LOG"; }
case "${1:-}:${2:-}" in
  image:inspect)
    shift 2
    fmt=""; name=""
    while [ $# -gt 0 ]; do
      case "$1" in
        -f) fmt="$2"; shift 2 ;;
        *) name="$1"; shift ;;
      esac
    done
    f="$FAKE_IMAGES/$(enc "$name")"
    [ -f "$f" ] || exit 1
    case "$fmt" in
      *dockerfile_sha*) sed -n 's/^label=//p' "$f" ;;
      *tier.key*) sed -n 's/^key=//p' "$f" ;;
      *RootFS*) sed -n 's/^layers=//p' "$f" | tr ',' '\n' ;;
      *.Id*) cksum < "$f" | cut -d' ' -f1 ;;
    esac
    exit 0 ;;
  build:*)
    shift 1
    label=""; tkey=""; tag=""; parent=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --label)
          case "$2" in
            g5.tier.dockerfile_sha=*) label="${2#g5.tier.dockerfile_sha=}" ;;
            g5.tier.key=*) tkey="${2#g5.tier.key=}" ;;
          esac
          shift 2 ;;
        --tag) tag="$2"; shift 2 ;;
        --build-arg) parent="${2#BASE_IMAGE=}"; shift 2 ;;
        *) shift ;;
      esac
    done
    layers=""
    if [ -n "$parent" ] && [ -f "$FAKE_IMAGES/$(enc "$parent")" ]; then
      layers="$(sed -n 's/^layers=//p' "$FAKE_IMAGES/$(enc "$parent")"),"
    fi
    printf 'label=%s\nkey=%s\nlayers=%slayer-%s\n' "$label" "$tkey" "$layers" "$RANDOM$RANDOM" > "$FAKE_IMAGES/$(enc "$tag")"
    log "build $tag"
    exit 0 ;;
  pull:*)
    f="$FAKE_REGISTRY/$(enc "$2")"
    log "pull $2"
    [ -f "$f" ] || exit 1
    cp "$f" "$FAKE_IMAGES/$(enc "$2")"
    exit 0 ;;
  tag:*)
    case "$2" in *"${FAKE_TAG_FAIL:-$'\n'}"*) exit 1 ;; esac
    cp "$FAKE_IMAGES/$(enc "$2")" "$FAKE_IMAGES/$(enc "$3")"
    exit 0 ;;
  push:*)
    log "push $2"
    cp "$FAKE_IMAGES/$(enc "$2")" "$FAKE_REGISTRY/$(enc "$2")"
    exit 0 ;;
  manifest:inspect)
    log "manifest $3"
    [ -f "$FAKE_REGISTRY/$(enc "$3")" ] || exit 1
    exit 0 ;;
  *) exit 0 ;;
esac
FAKE
chmod +x "$TEST_ROOT/bin/docker"

REG="example.test/proj/g5-ci"
run_script() {
  # $1 = case name; remaining = extra env assignments (VAR=value)
  local name="$1"; shift
  FAKE_LOG="$TEST_ROOT/$name.log"
  : > "$FAKE_LOG"
  OUT="$TEST_ROOT/$name.out"
  set +e
  env PATH="$TEST_ROOT/bin:$PATH" \
      FAKE_IMAGES="$TEST_ROOT/images" FAKE_REGISTRY="$TEST_ROOT/registry" FAKE_LOG="$FAKE_LOG" \
      G5_BASE_LOCK_DIR="$TEST_ROOT/lock" G5_BASE_BUILD_CONTEXT="$TEST_ROOT/ctx" \
      G5_BASE_DOCKERFILE_DIR="$TEST_ROOT/ctx/infra/docker" "$@" \
      bash "$SCRIPT" ${G5_BASE_TIERS_OVERRIDE:-base-node} >"$OUT" 2>&1
  STATUS=$?
  set -e
}
reset_local() { rm -rf "$TEST_ROOT/images"; mkdir -p "$TEST_ROOT/images"; }
reset_registry() { rm -rf "$TEST_ROOT/registry"; mkdir -p "$TEST_ROOT/registry"; }
fail() { echo "FAIL: $*" >&2; echo "--- output ---" >&2; cat "$OUT" >&2; echo "--- docker log ---" >&2; cat "$FAKE_LOG" >&2; exit 1; }
expect_log() { grep -qx "$1" "$FAKE_LOG" || fail "expected docker call '$1' in $2"; }
expect_no_log() { grep -q "^$1" "$FAKE_LOG" && fail "unexpected docker call '$1' in $2"; return 0; }

# 1. No registry: the legacy local-only path, no pull/push at all.
reset_local; reset_registry
run_script no-registry
[ "$STATUS" -eq 0 ] || fail "no-registry run exited $STATUS"
expect_log "build g5/base:26.04" no-registry
expect_log "build g5/base-node:26.04" no-registry
expect_no_log "pull" no-registry
expect_no_log "push" no-registry

# 2. Registry configured, mirror empty: the leaf misses, so its parent is
#    ensured (built) and both tiers are built and pushed.
reset_local; reset_registry
run_script miss G5_BASE_IMAGE_REGISTRY="$REG/"
[ "$STATUS" -eq 0 ] || fail "miss run exited $STATUS"
expect_log "build g5/base:26.04" miss
expect_log "build g5/base-node:26.04" miss
grep -q "^push $REG/g5-base:" "$FAKE_LOG" || fail "miss run did not push g5-base"
grep -q "^push $REG/g5-base-node:" "$FAKE_LOG" || fail "miss run did not push g5-base-node"
grep -q "^pull $REG/g5-base:" "$FAKE_LOG" || fail "miss run did not try the mirror first"
BASE_KEY="$(sed -n "s#^push $REG/g5-base:##p" "$FAKE_LOG")"
NODE_KEY="$(sed -n "s#^push $REG/g5-base-node:##p" "$FAKE_LOG")"
[ "${#BASE_KEY}" -eq 32 ] || fail "mirror key has unexpected length: '$BASE_KEY'"
grep -q "/g5-base:26.04" "$FAKE_LOG" && fail "mirror name must not reuse the mutable local tag"

# 3. Fresh worker, mirror populated: the requested leaf is pulled by its key
#    and tagged; never built, never pushed, and its PARENT is never pulled:
#    the key proves the lineage, so a pulled leaf needs no parent locally.
reset_local
run_script hit G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "hit run exited $STATUS"
expect_log "pull $REG/g5-base-node:$NODE_KEY" hit
expect_no_log "pull $REG/g5-base:" hit
expect_no_log "build" hit
expect_no_log "push" hit
grep -q "Base tier pulled from mirror: g5/base-node:26.04" "$OUT" || fail "hit run did not report the mirror pull"

# 4. Mirror entry with a wrong freshness label: rejected, local build instead.
#    Under STRICT lineage, requested parent-first (`base base-node`, as the
#    oracle's callers do): the mirrored child still matches its key, but with
#    the locally rebuilt parent PRESENT it no longer layers on it and is
#    rebuilt — the fleet single-base premise the oracle's callers opt into.
reset_local
printf 'label=deadbeef\nkey=x\nlayers=layer-x\n' > "$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base:$BASE_KEY" | tr '/:' '__')"
G5_BASE_TIERS_OVERRIDE="base base-node" run_script bad-label G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_STRICT_LINEAGE=1
[ "$STATUS" -eq 0 ] || fail "bad-label run exited $STATUS"
expect_log "pull $REG/g5-base:$BASE_KEY" bad-label
expect_log "build g5/base:26.04" bad-label
grep -q "WARN: mirrored base tier .* carries freshness label 'deadbeef'" "$OUT" || fail "bad-label run did not explain the rejection"
expect_log "pull $REG/g5-base-node:$NODE_KEY" bad-label
expect_log "build g5/base-node:26.04" bad-label
grep -q "WARN: mirrored base tier .* is not layered on the current g5/base:26.04" "$OUT" || fail "bad-label run did not reject the child on parent mismatch"

# 5. Parent recipe change changes the CHILD key even though the child recipe
#    is untouched (lineage is part of the key).
reset_local; reset_registry
printf 'FROM ubuntu\nRUN echo base v2\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base"
run_script parent-change G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "parent-change run exited $STATUS"
NODE_KEY_2="$(sed -n "s#^push $REG/g5-base-node:##p" "$FAKE_LOG")"
[ -n "$NODE_KEY_2" ] || fail "parent-change run did not push g5-base-node"
[ "$NODE_KEY_2" != "$NODE_KEY" ] || fail "child mirror key did not change with the parent recipe"

# 6. Tier whitelist: only listed tiers touch the mirror; the rest build
#    locally with no pull or push (the CI image's miss path lists
#    "base base-rust" so the workspace-warmed tier is never mirrored).
reset_local; reset_registry
run_script whitelist G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_TIERS="base"
[ "$STATUS" -eq 0 ] || fail "whitelist run exited $STATUS"
grep -q "^pull $REG/g5-base:" "$FAKE_LOG" || fail "whitelisted base must consult the mirror"
grep -q "^push $REG/g5-base:" "$FAKE_LOG" || fail "whitelisted base must be pushed"
grep -q "g5-base-node" "$FAKE_LOG" && fail "base-node is outside the whitelist and must not touch the mirror"
expect_log "build g5/base-node:26.04" whitelist

# 7. Tiers current locally but absent from the mirror are published without a
#    rebuild (the deploy pipeline's publish-base-tiers runs after the same
#    tiers were built for the image, so "already present" must still push).
#    Both tiers are requested: a current leaf alone would not touch its parent.
reset_registry
G5_BASE_TIERS_OVERRIDE="base base-node" run_script publish-current G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "publish-current run exited $STATUS"
expect_no_log "build" publish-current
grep -q "^push $REG/g5-base:" "$FAKE_LOG" || fail "publish-current run did not push the current g5-base"
grep -q "^push $REG/g5-base-node:" "$FAKE_LOG" || fail "publish-current run did not push the current g5-base-node"
# ...and a second run finds the mirror populated and pushes nothing.
G5_BASE_TIERS_OVERRIDE="base base-node" run_script publish-again G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "publish-again run exited $STATUS"
expect_no_log "build" publish-again
expect_no_log "push" publish-again
grep -q "Base tier already mirrored:" "$OUT" || fail "publish-again run did not report the mirror hit"

# 8. Producer mode (G5_BASE_IMAGE_MIRROR_PULL=0): a fresh worker with a
#    populated mirror never pulls, builds locally, and pushes over whatever the
#    mirror holds, so a trusted producer can never be fed a foreign tier.
reset_local
run_script producer G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PULL=0
[ "$STATUS" -eq 0 ] || fail "producer run exited $STATUS"
expect_no_log "pull" producer
expect_no_log "manifest" producer
expect_log "build g5/base:26.04" producer
expect_log "build g5/base-node:26.04" producer
grep -q "^push $REG/g5-base:" "$FAKE_LOG" || fail "producer run did not push g5-base"
grep -q "^push $REG/g5-base-node:" "$FAKE_LOG" || fail "producer run did not push g5-base-node"
grep -q "publish-only for this run" "$OUT" || fail "producer run did not report the publish-only mirror"

# 9. Lineage labels are authoritative. (a) A mirror entry whose recipe label
#    matches but whose `g5.tier.key` label names another lineage is rejected
#    and rebuilt. (b) A local image in the same state is rebuilt. (c) A local
#    image with NO key label is accepted only when it is the very image the
#    mirror holds under this key.
reset_local; reset_registry
run_script seed-9 G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "seed-9 run exited $STATUS"
NODE_KEY_9="$(sed -n "s#^push $REG/g5-base-node:##p" "$FAKE_LOG")"
[ -n "$NODE_KEY_9" ] || fail "seed-9 run did not push g5-base-node"
# (a) corrupt the mirror entry's key label only.
node_entry="$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base-node:$NODE_KEY_9" | tr '/:' '__')"
sed -i.bak 's/^key=.*/key=someother-lineage/' "$node_entry" && rm -f "$node_entry.bak"
reset_local
run_script wrong-key-mirror G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "wrong-key-mirror run exited $STATUS"
expect_log "pull $REG/g5-base-node:$NODE_KEY_9" wrong-key-mirror
grep -q "WARN: mirrored base tier .* carries lineage key 'someother-lineage'" "$OUT" || fail "wrong-key-mirror run did not reject the foreign lineage"
expect_log "build g5/base-node:26.04" wrong-key-mirror
# (b) the local image now carries a wrong key label: rebuilt, not trusted.
node_local="$TEST_ROOT/images/$(printf '%s' "g5/base-node:26.04" | tr '/:' '__')"
sed -i.bak 's/^key=.*/key=someother-lineage/' "$node_local" && rm -f "$node_local.bak"
run_script wrong-key-local G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "wrong-key-local run exited $STATUS"
grep -q "Base tier present but carries lineage key 'someother-lineage'" "$OUT" || fail "wrong-key-local run did not explain the rebuild"
# The mirror entry was replaced by the previous run, so the untrusted local image is replaced from the mirror, not rebuilt.
grep -q "Base tier pulled from mirror: g5/base-node:26.04" "$OUT" || fail "wrong-key-local run must replace the untrusted local image from the mirror"
# (c) an unlabelled local image equal to the mirror's image for this key is
#     current (a tier pulled before keys were labelled); one that is not, is
#     rebuilt.
reset_local; reset_registry
run_script seed-9c G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "seed-9c run exited $STATUS"
for f in "$TEST_ROOT/images/$(printf '%s' "g5/base-node:26.04" | tr '/:' '__')" "$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base-node:$NODE_KEY_9" | tr '/:' '__')"; do
  sed -i.bak 's/^key=.*/key=/' "$f" && rm -f "$f.bak"
done
cp "$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base-node:$NODE_KEY_9" | tr '/:' '__')" "$TEST_ROOT/images/$(printf '%s' "$REG/g5-base-node:$NODE_KEY_9" | tr '/:' '__')"
run_script unlabelled-same G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "unlabelled-same run exited $STATUS"
expect_no_log "build" unlabelled-same
grep -q "Base tier already present and current: g5/base-node:26.04" "$OUT" || fail "unlabelled-same run did not accept the mirror's own image"
printf 'label=%s\nkey=\nlayers=layer-other\n' "$(sed -n 's/^label=//p' "$node_local")" > "$node_local"
run_script unlabelled-other G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "unlabelled-other run exited $STATUS"
grep -q "Base tier present but carries no lineage key" "$OUT" || fail "unlabelled-other run did not explain the rebuild"

# 10. Under strict lineage, a mirrored child rejected only because the LOCAL
#     parent was stale is re-validated once that parent is refreshed from the
#     mirror, and is then accepted without a build: the stale parent never
#     costs a child cook.
reset_local; reset_registry
run_script seed-10 G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "seed-10 run exited $STATUS"
BASE_KEY_10="$(sed -n "s#^push $REG/g5-base:##p" "$FAKE_LOG")"
NODE_KEY_10="$(sed -n "s#^push $REG/g5-base-node:##p" "$FAKE_LOG")"
reset_local
# A local base with the current recipe label but a stale key and foreign
# layers: not current, so ensuring it pulls the mirror's base.
printf 'label=%s\nkey=stale\nlayers=layer-stale\n' "$(sed -n 's/^label=//p' "$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base:$BASE_KEY_10" | tr '/:' '__')")" > "$TEST_ROOT/images/$(printf '%s' "g5/base:26.04" | tr '/:' '__')"
run_script stale-parent G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_STRICT_LINEAGE=1
[ "$STATUS" -eq 0 ] || fail "stale-parent run exited $STATUS"
expect_log "pull $REG/g5-base-node:$NODE_KEY_10" stale-parent
grep -q "WARN: mirrored base tier .* is not layered on the current g5/base:26.04" "$OUT" || fail "stale-parent run did not reject the child against the stale parent first"
expect_log "pull $REG/g5-base:$BASE_KEY_10" stale-parent
expect_no_log "build" stale-parent
grep -q "Base tier current on the refreshed g5/base:26.04: g5/base-node:26.04" "$OUT" || fail "stale-parent run did not re-validate the child after refreshing its parent"

# 11. Default (keyed) lineage, the guest's mode: a parent present from
#     ANOTHER same-key build (different layer ids) never makes a mirrored
#     child rebuild, on a first dev-up or a second one with base already local.
reset_local; reset_registry
run_script seed-11 G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "seed-11 run exited $STATUS"
BASE_KEY_11="$(sed -n "s#^push $REG/g5-base:##p" "$FAKE_LOG")"
NODE_KEY_11="$(sed -n "s#^push $REG/g5-base-node:##p" "$FAKE_LOG")"
reset_local
# A local base of the same recipe and key but other layers (another build).
printf 'label=%s\nkey=%s\nlayers=layer-otherbuild\n' "$(sed -n 's/^label=//p' "$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base:$BASE_KEY_11" | tr '/:' '__')")" "$BASE_KEY_11" > "$TEST_ROOT/images/$(printf '%s' "g5/base:26.04" | tr '/:' '__')"
G5_BASE_TIERS_OVERRIDE="base-node base" run_script keyed-parent G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "keyed-parent run exited $STATUS"
expect_log "pull $REG/g5-base-node:$NODE_KEY_11" keyed-parent
expect_no_log "build" keyed-parent
grep -q "is not layered" "$OUT" && fail "keyed lineage must not compare layers with a same-key parent"
grep -q "Base tier pulled from mirror: g5/base-node:26.04" "$OUT" || fail "keyed-parent run did not accept the mirrored child"
grep -q "Base tier already present and current: g5/base:26.04" "$OUT" || fail "keyed-parent run did not keep the same-key local base"
# Second dev-up: everything local and current, nothing pulled or built.
G5_BASE_TIERS_OVERRIDE="base-node base" run_script second-dev-up G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "second-dev-up run exited $STATUS"
expect_no_log "build" second-dev-up
expect_no_log "pull" second-dev-up

# 12. Producer mode with tiers already current locally publishes every tier it
#     was asked for, without a build (the deploy asks for all four explicitly).
G5_BASE_TIERS_OVERRIDE="base base-node" run_script producer-prebuilt G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PULL=0
[ "$STATUS" -eq 0 ] || fail "producer-prebuilt run exited $STATUS"
expect_no_log "build" producer-prebuilt
expect_no_log "pull" producer-prebuilt
grep -q "^push $REG/g5-base:" "$FAKE_LOG" || fail "producer-prebuilt run did not push g5-base"
grep -q "^push $REG/g5-base-node:" "$FAKE_LOG" || fail "producer-prebuilt run did not push g5-base-node"

# 13. Push disabled (a workspace guest behind the read-only door): a miss is
#     built locally and nothing is pushed or even checked on the mirror.
reset_local; reset_registry
run_script no-push G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0
[ "$STATUS" -eq 0 ] || fail "no-push run exited $STATUS"
grep -q "^pull $REG/g5-base-node:" "$FAKE_LOG" || fail "no-push run did not consult the mirror"
expect_log "build g5/base-node:26.04" no-push
expect_no_log "push" no-push
expect_no_log "manifest" no-push

# 14. Publication also moves `g5-base-rust-warm-newest:<parent key>` (the
#     dependency tier only: toolchain tiers never get a newest name), and a
#     guest that opts in (G5_BASE_IMAGE_FALLBACK_NEWEST=1, push off, as dev-up
#     behind the door) takes it when its exact key is not published yet: no
#     build, no push, cargo rebuilds the recipe change on top. The newest name
#     never matches the `g5-<tier>:<key>` extractions above.
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo rust 1.0\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust"
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo cook v1\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust-warm"
reset_local; reset_registry
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script seed-14 G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "seed-14 run exited $STATUS"
RUST_KEY_14="$(sed -n "s#^push $REG/g5-base-rust:##p" "$FAKE_LOG")"
WARM_KEY_14="$(sed -n "s#^push $REG/g5-base-rust-warm:##p" "$FAKE_LOG")"
[ "${#WARM_KEY_14}" -eq 32 ] || fail "seed-14 warm key has unexpected length: '$WARM_KEY_14'"
expect_log "push $REG/g5-base-rust-warm-newest:$RUST_KEY_14" seed-14
expect_no_log "push $REG/g5-base-rust-newest" seed-14
expect_no_log "push $REG/g5-base-newest" seed-14
grep -q "Base tier published as newest on its parent: $REG/g5-base-rust-warm-newest:$RUST_KEY_14" "$OUT" || fail "seed-14 run did not report the newest name"
# The warm recipe changes (a lockfile bump); delivery has not published the new key.
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo cook v2\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust-warm"
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script fallback-on G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1
[ "$STATUS" -eq 0 ] || fail "fallback-on run exited $STATUS"
grep -q "^pull $REG/g5-base-rust-warm:" "$FAKE_LOG" || fail "fallback-on run did not try the exact key first"
expect_log "pull $REG/g5-base-rust-warm-newest:$RUST_KEY_14" fallback-on
expect_no_log "build" fallback-on
expect_no_log "push" fallback-on
grep -q "using the newest tier on the same parent: g5/base-rust-warm:26.04 <- $REG/g5-base-rust-warm-newest:$RUST_KEY_14 (lineage $WARM_KEY_14)" "$OUT" || fail "fallback-on run did not report the fallback"
# A second dev-up on the same guest: the fallback image is stale by label, the
# exact key is still missing, the fallback is taken again, nothing is built.
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script fallback-again G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1
[ "$STATUS" -eq 0 ] || fail "fallback-again run exited $STATUS"
expect_no_log "build" fallback-again
expect_log "pull $REG/g5-base-rust-warm-newest:$RUST_KEY_14" fallback-again
# Delivery publishes the new key: the exact tier is taken and the fallback is not.
G5_BASE_TIERS_OVERRIDE="base base-rust base-rust-warm" run_script publish-14 G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PULL=0
[ "$STATUS" -eq 0 ] || fail "publish-14 run exited $STATUS"
WARM_KEY_14B="$(sed -n "s#^push $REG/g5-base-rust-warm:##p" "$FAKE_LOG")"
[ -n "$WARM_KEY_14B" ] && [ "$WARM_KEY_14B" != "$WARM_KEY_14" ] || fail "publish-14 run did not publish the changed warm key"
expect_log "push $REG/g5-base-rust-warm-newest:$RUST_KEY_14" publish-14
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script exact-after-publish G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1
[ "$STATUS" -eq 0 ] || fail "exact-after-publish run exited $STATUS"
expect_log "pull $REG/g5-base-rust-warm:$WARM_KEY_14B" exact-after-publish
expect_no_log "pull $REG/g5-base-rust-warm-newest" exact-after-publish
expect_no_log "build" exact-after-publish

# 15. The fallback is opt-in, and a toolchain change is never bridged: without
#     the flag a miss builds as before; with the flag, a changed base-rust
#     recipe changes the warm tier's parent key, so the newest name of the NEW
#     parent is asked for (absent) and base-rust and the warm tier are built;
#     base-rust itself never asks for a newest name. Once delivery publishes
#     the new toolchain and its warm tier, a fresh guest pulls both exact keys.
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo cook v3\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust-warm"
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script fallback-off G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0
[ "$STATUS" -eq 0 ] || fail "fallback-off run exited $STATUS"
expect_no_log "pull $REG/g5-base-rust-warm-newest" fallback-off
expect_log "build g5/base-rust-warm:26.04" fallback-off
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo rust 2.0\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust"
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script toolchain-change G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1
[ "$STATUS" -eq 0 ] || fail "toolchain-change run exited $STATUS"
grep -q "^pull $REG/g5-base-rust-warm-newest:" "$FAKE_LOG" || fail "toolchain-change run must ask for the newest name of the new parent key"
grep -q "^pull $REG/g5-base-rust-warm-newest:$RUST_KEY_14$" "$FAKE_LOG" && fail "toolchain-change run must not take the newest tier of the OLD toolchain"
expect_no_log "pull $REG/g5-base-rust-newest" toolchain-change
expect_log "build g5/base-rust:26.04" toolchain-change
expect_log "build g5/base-rust-warm:26.04" toolchain-change
grep -q "using the newest tier" "$OUT" && fail "toolchain-change run must not report a fallback"
G5_BASE_TIERS_OVERRIDE="base base-rust base-rust-warm" run_script publish-15 G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PULL=0
[ "$STATUS" -eq 0 ] || fail "publish-15 run exited $STATUS"
RUST_KEY_15="$(sed -n "s#^push $REG/g5-base-rust:##p" "$FAKE_LOG")"
WARM_KEY_15="$(sed -n "s#^push $REG/g5-base-rust-warm:##p" "$FAKE_LOG")"
[ "$RUST_KEY_15" != "$RUST_KEY_14" ] || fail "publish-15 run did not publish the new toolchain key"
expect_log "push $REG/g5-base-rust-warm-newest:$RUST_KEY_15" publish-15
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm base-rust" run_script fresh-after-toolchain G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1
[ "$STATUS" -eq 0 ] || fail "fresh-after-toolchain run exited $STATUS"
expect_log "pull $REG/g5-base-rust-warm:$WARM_KEY_15" fresh-after-toolchain
expect_log "pull $REG/g5-base-rust:$RUST_KEY_15" fresh-after-toolchain
expect_no_log "build" fresh-after-toolchain
expect_no_log "pull $REG/g5-base-rust-warm-newest" fresh-after-toolchain

# 16. Strict lineage (CI's single-base oracle) ignores the fallback even when set.
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo cook v4\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust-warm"
reset_local
G5_BASE_TIERS_OVERRIDE="base base-rust base-rust-warm" run_script fallback-strict G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1 G5_BASE_IMAGE_STRICT_LINEAGE=1
[ "$STATUS" -eq 0 ] || fail "fallback-strict run exited $STATUS"
expect_no_log "pull $REG/g5-base-rust-warm-newest" fallback-strict
expect_log "build g5/base-rust-warm:26.04" fallback-strict

# 17. A fallback whose local tagging fails (only tags of newest names fail
#     here) is not taken: with a stale local warm tier present (its own
#     lineage key), the tier is built, never accepted on the stale image.
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script seed-17 G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1
[ "$STATUS" -eq 0 ] || fail "seed-17 run exited $STATUS"
expect_no_log "build" seed-17
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo cook v5\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust-warm"
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script tag-fails G5_BASE_IMAGE_REGISTRY="$REG" G5_BASE_IMAGE_MIRROR_PUSH=0 G5_BASE_IMAGE_FALLBACK_NEWEST=1 FAKE_TAG_FAIL=-newest:
[ "$STATUS" -eq 0 ] || fail "tag-fails run exited $STATUS"
expect_log "pull $REG/g5-base-rust-warm-newest:$RUST_KEY_15" tag-fails
grep -q "using the newest tier" "$OUT" && fail "tag-fails run must not report a fallback it could not tag"
expect_log "build g5/base-rust-warm:26.04" tag-fails

# 18. A mirrored entry the run rejects (wrong freshness label) is replaced by
#     the local build in the default pull-and-push mode, so later callers get
#     a valid tier instead of rejecting the same entry again.
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo node v4\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-node"
reset_local; reset_registry
run_script seed-18 G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "seed-18 run exited $STATUS"
NODE_KEY_18="$(sed -n "s#^push $REG/g5-base-node:##p" "$FAKE_LOG")"
node_entry_18="$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base-node:$NODE_KEY_18" | tr '/:' '__')"
sed -i.bak 's/^label=.*/label=corrupt/' "$node_entry_18" && rm -f "$node_entry_18.bak"
reset_local
run_script replace-rejected G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "replace-rejected run exited $STATUS"
expect_log "pull $REG/g5-base-node:$NODE_KEY_18" replace-rejected
grep -q "WARN: mirrored base tier .* carries freshness label 'corrupt'" "$OUT" || fail "replace-rejected run did not reject the corrupt entry"
expect_log "build g5/base-node:26.04" replace-rejected
expect_log "push $REG/g5-base-node:$NODE_KEY_18" replace-rejected
grep -q "Base tier already mirrored" "$OUT" && fail "replace-rejected run must not keep the rejected entry"
reset_local
run_script after-replace G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "after-replace run exited $STATUS"
expect_no_log "build" after-replace
grep -q "Base tier pulled from mirror: g5/base-node:26.04" "$OUT" || fail "after-replace run did not accept the replaced entry"

# 19. A publishing run that hits the exact warm key restores a missing newest
#     name (an earlier run's newest push failed) and leaves an existing one alone.
printf 'ARG BASE_IMAGE\nFROM ${BASE_IMAGE}\nRUN echo cook v6\n' > "$TEST_ROOT/ctx/infra/docker/Dockerfile.base-rust-warm"
reset_local; reset_registry
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script seed-19 G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "seed-19 run exited $STATUS"
RUST_KEY_19="$(sed -n "s#^push $REG/g5-base-rust:##p" "$FAKE_LOG")"
rm -f "$TEST_ROOT/registry/$(printf '%s' "$REG/g5-base-rust-warm-newest:$RUST_KEY_19" | tr '/:' '__')"
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script restore-newest G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "restore-newest run exited $STATUS"
expect_no_log "build" restore-newest
expect_log "push $REG/g5-base-rust-warm-newest:$RUST_KEY_19" restore-newest
grep -q "Base tier newest name restored from the mirrored tier" "$OUT" || fail "restore-newest run did not report the restore"
reset_local
G5_BASE_TIERS_OVERRIDE="base-rust-warm" run_script newest-present G5_BASE_IMAGE_REGISTRY="$REG"
[ "$STATUS" -eq 0 ] || fail "newest-present run exited $STATUS"
expect_no_log "push" newest-present
expect_log "manifest $REG/g5-base-rust-warm-newest:$RUST_KEY_19" newest-present

echo "PASS: ensure-base-images.sh registry mirror contract (19 cases)"
