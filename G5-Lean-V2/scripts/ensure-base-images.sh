#!/usr/bin/env bash
# Ensure the G5 common base image tiers exist as LOCAL Docker tags, building
# them in dependency order, before a leaf image that is `FROM g5/base*:<tag>`
# is built.
#
# Contract: software_architecture/integration-stack/docker_images [D18] (one
# common base `g5/base:26.04`, with `g5/base-rust:26.04` and
# `g5/base-node:26.04` layered as named toolchain tiers) and [D5]'s rule that
# these development base tags are LOCAL-ONLY unless explicitly published, so a
# pipeline that builds a leaf FROM one of them must build/tag the base locally
# first and must not fall through to pulling a public-registry base.
#
# The base/base-node Dockerfiles COPY nothing from the build context (git is
# fetched by checksum; Node comes from NodeSource), so those tiers are
# context-agnostic. `base-rust-warm` cargo-chef-cooks the workspace dependencies,
# so callers requesting that tier must provide the Cargo workspace as context.
#
# Usage:
#   ensure-base-images.sh <tier> [<tier> ...]   # tiers: base base-rust base-rust-warm base-node
#
# Requesting `base-rust`/`base-node` implies `base`; `base-rust-warm` implies
# both `base` and `base-rust`. The script always builds ancestors first.
#
# Environment:
#   G5_BASE_IMAGE_TAG        Tag for every tier (default: 26.04). Matches the
#                            `g5/base*:26.04` ARG defaults in the leaf images.
#   G5_BASE_DOCKERFILE_DIR   Directory holding Dockerfile.base* relative to the
#                            build CWD (default: infra/docker). Terraform builds from
#                            the repo root, so it passes `infra/docker`.
#   G5_BASE_BUILD_CONTEXT    Docker build context (default: .).
#   G5_BASE_IMAGE_REGISTRY   Optional registry repository prefix (for example
#                            us-central1-docker.pkg.dev/PROJECT/g5-ci) that
#                            mirrors each tier as `<prefix>/g5-<tier>:<key>`,
#                            where <key> is a content key covering the tier's
#                            own freshness inputs AND its parent's key. A tier
#                            that is missing or stale locally is pulled from the
#                            mirror before a local build is attempted, and
#                            before its parents are touched: the key proves the
#                            whole lineage, so a pulled leaf needs no parent
#                            locally (dev-up asks for its leaves before base
#                            and never pulls base-rust). When a parent IS
#                            present locally the pulled tier must still layer
#                            on it (the single-base oracle's premise), so
#                            callers that run that oracle ask for the chain
#                            parent-first. A tier built locally is labelled
#                            with its key and pushed back best-effort so the
#                            next caller pulls instead of building. The local
#                            `g5/<tier>:<tag>` names stay the only names the
#                            leaf images resolve; the mirror is a cache.
#                            Unset by default, which keeps the dev tags
#                            local-only exactly as before.
#   G5_BASE_IMAGE_MIRROR_TIERS
#                            Optional space-separated subset of tiers the
#                            mirror applies to (default: every tier). The CI
#                            image's miss path sets "base base-rust" so the
#                            multi-gigabyte workspace-warmed tier is never
#                            pushed or pulled; it is rebuilt locally.
#   G5_BASE_IMAGE_FALLBACK_NEWEST
#                            "1" lets the dependency tier (base-rust-warm, the
#                            only tier whose content is derived objects cargo
#                            can rebuild on top of) be taken, when the mirror
#                            does not hold its exact key yet (a merged recipe
#                            change delivery has not published), from the
#                            mirror's newest build of it on the SAME parent
#                            lineage, `<prefix>/g5-base-rust-warm-newest:<parent
#                            key>`, instead of being built locally. Every
#                            publication moves that name. Toolchain tiers
#                            (base, base-rust, base-node) never fall back: a
#                            stale toolchain is not something a later build
#                            repairs. Off by default; the workspace guest's
#                            dev-up sets it (ontology
#                            g5.runtime.cache.tier-fallback). Never crosses a
#                            parent change, and never applies under
#                            G5_BASE_IMAGE_STRICT_LINEAGE.
#   G5_BASE_IMAGE_STRICT_LINEAGE
#                            "1" additionally requires a tier whose parent is
#                            present locally to layer on that parent by digest
#                            (the fleet single-base oracle's premise,
#                            `verify-base-digest-identity.sh`), rebuilding it
#                            on the local parent otherwise. Only callers that
#                            run that oracle (`just ci-docker`) set it; by
#                            default a tier is current on its recipe label and
#                            content key alone, so a workspace guest with a
#                            same-key parent from another build never rebuilds.
#   G5_BASE_IMAGE_MIRROR_PUSH
#                            "0" never pushes a tier back to the mirror. A
#                            workspace guest sets it: its registry is the host
#                            cache door, which serves reads only, so a push
#                            would only fail (and docker would first try). The
#                            default "1" pushes what the mirror lacks.
#   G5_BASE_IMAGE_MIRROR_PULL
#                            "0" makes this run the mirror's trusted producer:
#                            it never pulls a tier from the mirror (what it
#                            lacks locally it builds) and always pushes what
#                            it has, replacing whatever the mirror held under
#                            the key. The deploy pipeline publishes the tiers
#                            workspace guests pull this way, into a repository
#                            only it may write. Default "1": pull on a miss,
#                            push only what the mirror lacks.
set -euo pipefail

if [[ $# -eq 0 || "${1:-}" == "--help" ]]; then
  echo "Usage: ensure-base-images.sh <tier> [<tier> ...]   (tiers: base base-rust base-rust-warm base-node)" >&2
  [[ "${1:-}" == "--help" ]] && exit 0
  exit 2
fi

TAG="${G5_BASE_IMAGE_TAG:-26.04}"
DF_DIR="${G5_BASE_DOCKERFILE_DIR:-infra/docker}"
CONTEXT="${G5_BASE_BUILD_CONTEXT:-.}"
REGISTRY="${G5_BASE_IMAGE_REGISTRY:-}"
REGISTRY="${REGISTRY%/}"
MIRROR_TIERS="${G5_BASE_IMAGE_MIRROR_TIERS:-}"
MIRROR_PULL="${G5_BASE_IMAGE_MIRROR_PULL:-1}"
MIRROR_PUSH="${G5_BASE_IMAGE_MIRROR_PUSH:-1}"
STRICT_LINEAGE="${G5_BASE_IMAGE_STRICT_LINEAGE:-0}"
FALLBACK_NEWEST="${G5_BASE_IMAGE_FALLBACK_NEWEST:-0}"

# True when the mirror applies to $1: no whitelist, or the tier is listed.
tier_mirrored() {
  [[ -z "$MIRROR_TIERS" ]] && return 0
  local t
  for t in $MIRROR_TIERS; do
    [[ "$t" == "$1" ]] && return 0
  done
  return 1
}

# Per-tier mirror keys, recorded as each tier is ensured so a child's key can
# fold in its parent's. Plain variables (not an associative array) because this
# script must run on macOS's bash 3.2.
set_tier_key() { eval "G5_TIER_KEY_${1//-/_}=\"\$2\""; }
get_tier_key() { eval "printf '%s' \"\${G5_TIER_KEY_${1//-/_}:-}\""; }

# The mirror name for a tier, or empty when no registry is configured or the
# tier is outside G5_BASE_IMAGE_MIRROR_TIERS.
mirror_ref_of() {
  local tier="$1" key="$2"
  [[ -n "$REGISTRY" ]] || return 0
  tier_mirrored "$tier" || return 0
  echo "${REGISTRY}/g5-${tier}:${key}"
}

# The mirror's "newest build of this tier on this parent" name,
# `<prefix>/g5-<tier>-newest:<parent key>`, or empty for every tier but the
# dependency tier, or outside the mirror. Only base-rust-warm has it: its
# content is derived objects on top of its parent, which cargo rebuilds where
# the recipe moved on; a toolchain tier's content (rustc, node, git) is not
# repaired by anything built on it, and a child built on a stale parent would
# carry the current key while layering on the wrong toolchain. Publication
# moves the name (publish_tier); a guest whose exact key is not published yet
# may take it (G5_BASE_IMAGE_FALLBACK_NEWEST). Keyed by the parent's key so the
# fallback never crosses a parent change.
newest_ref_of() {
  local tier="$1" parent_tier
  [[ "$tier" == base-rust-warm ]] || return 0
  [[ -n "$REGISTRY" ]] || return 0
  tier_mirrored "$tier" || return 0
  parent_tier="$(parent_tier_of "$tier")"
  [[ -n "$parent_tier" ]] || return 0
  compute_tier_key "$parent_tier"
  echo "${REGISTRY}/g5-${tier}-newest:$(get_tier_key "$parent_tier")"
}

# The G5 parent tier each tier layers on (empty ⇒ no G5 ancestor; `base` is
# FROM the external ubuntu tag).
parent_tier_of() {
  case "$1" in
    base-rust|base-node) echo base ;;
    base-rust-warm)      echo base-rust ;;
    *)                   echo "" ;;
  esac
}
parent_ref_of() {
  local parent
  parent="$(parent_tier_of "$1")"
  [[ -n "$parent" ]] && echo "g5/${parent}:${TAG}" || echo ""
}

# Per-tier memos (plain variables: this script must run on macOS's bash 3.2).
set_tier_dfsha() { eval "G5_TIER_DFSHA_${1//-/_}=\"\$2\""; }
get_tier_dfsha() { eval "printf '%s' \"\${G5_TIER_DFSHA_${1//-/_}:-}\""; }
set_tier_done()  { eval "G5_TIER_DONE_${1//-/_}=1"; }
tier_done()      { eval "[ \"\${G5_TIER_DONE_${1//-/_}:-}\" = 1 ]"; }

# Compute (and memoize) a tier's freshness digest and content key from its
# recipe alone, without building or pulling anything. The digest covers the
# tier's Dockerfile, every input it copies in or cooks, and the topology
# names; the key folds the PARENT's key into it, so the key of a leaf proves
# the recipe of its whole ancestry. Keys being recipe-only is what lets a
# requested leaf be looked up in the mirror before its parents exist locally.
compute_tier_key() {
  local tier="$1"
  [[ -n "$(get_tier_key "$tier")" ]] && return 0
  local dockerfile dfpath
  dockerfile="$(dockerfile_of "$tier")"
  dfpath="${DF_DIR}/${dockerfile}"
  if [[ ! -f "$dfpath" ]]; then
    echo "ERROR: Dockerfile not found for tier '$tier' at '$dfpath'" >&2
    exit 2
  fi
  local -a hash_inputs=("$dfpath")
  local extra
  for extra in $(extra_inputs_of "$tier"); do
    if [[ -f "${CONTEXT}/${extra}" ]]; then
      hash_inputs+=("${CONTEXT}/${extra}")
    else
      echo "WARN: tier '$tier' input '${extra}' not found under context '${CONTEXT}'; freshness label will not cover it" >&2
    fi
  done
  # Hash with explicit per-file boundaries (path NUL content NUL): a bare cat
  # concatenation lets content migrate across file boundaries (or files swap
  # order) without changing the digest.
  local dfsha
  dfsha="$({ for f in "${hash_inputs[@]}"; do printf '%s\0' "$f"; cat "$f"; printf '\0'; done; topology_names "$tier"; } | sha256_stream)"
  local parent_tier parent_key=""
  parent_tier="$(parent_tier_of "$tier")"
  if [[ -n "$parent_tier" ]]; then
    compute_tier_key "$parent_tier"
    parent_key="$(get_tier_key "$parent_tier")"
  fi
  local key
  key="$(printf 'parent:%s\nself:%s\n' "$parent_key" "$dfsha" | sha256_stream | cut -c1-32)"
  set_tier_dfsha "$tier" "$dfsha"
  set_tier_key "$tier" "$key"
}

# True when $child's bottom image layers are exactly $parent's RootFS layers,
# i.e. $child was built on the CURRENT local $parent. This is the fleet
# single-base oracle's premise (test_contract/base_image_consolidation [D2],
# `verify-base-digest-identity.sh`): every image on a host derives from one
# base tier by layer digest, not merely by recipe. It is checked whenever the
# parent is present locally; a tier pulled by key without its parent (the
# guest's leaf-first pull) has no local parent to compare against and is
# accepted on its key alone.
tier_built_on_current_parent() {
  local child="$1" parent="$2" pl cl np
  pl="$(docker image inspect -f '{{range .RootFS.Layers}}{{println .}}{{end}}' "$parent" 2>/dev/null)" || return 1
  cl="$(docker image inspect -f '{{range .RootFS.Layers}}{{println .}}{{end}}' "$child" 2>/dev/null)" || return 1
  np="$(printf '%s' "$pl" | grep -c .)"
  [ "$np" -gt 0 ] || return 1
  [ "$(printf '%s' "$cl" | sed -n "1,${np}p")" = "$(printf '%s' "$pl")" ]
}

# Does $1 carry this tier's lineage? The content key ($2) folds the parent's
# recipe into the tier's own, so a populated `g5.tier.key` label must equal
# it; an image labelled with a different key is a build of another lineage,
# whatever else it claims. An image with NO key label (built or mirrored
# before keys were labelled) is accepted only when it is the very image the
# mirror holds under this key ($3). Prints the reason when not.
image_has_lineage() {
  local ref="$1" key="$2" mirror="${3:-}" stored_key
  stored_key="$(docker image inspect -f '{{index .Config.Labels "g5.tier.key"}}' "$ref" 2>/dev/null || true)"
  if [[ -n "$stored_key" ]]; then
    if [[ "$stored_key" == "$key" ]]; then return 0; fi
    echo "carries lineage key '$stored_key', expected '$key'"
    return 1
  fi
  if [[ -n "$mirror" ]]; then
    local ref_id mirror_id
    ref_id="$(docker image inspect -f '{{.Id}}' "$ref" 2>/dev/null || true)"
    mirror_id="$(docker image inspect -f '{{.Id}}' "$mirror" 2>/dev/null || true)"
    if [[ -n "$ref_id" && "$ref_id" == "$mirror_id" ]]; then return 0; fi
  fi
  echo "carries no lineage key"
  return 1
}

# Is the local image $1 the current build of its tier? Current means: its
# freshness label is this run's recipe digest ($2), it has this run's lineage
# ($3, see image_has_lineage; $4 is the mirror ref), and, under strict
# lineage only, when its parent tier ($5, a local ref or empty) is present
# locally, it layers on exactly that parent. Prints the reason when not.
image_is_current() {
  local ref="$1" dfsha="$2" key="$3" mirror="${4:-}" parent="${5:-}" stored_sha reason
  stored_sha="$(docker image inspect -f '{{index .Config.Labels "g5.tier.dockerfile_sha"}}' "$ref" 2>/dev/null || true)"
  if [[ -z "$stored_sha" ]]; then echo "carries no freshness label"; return 1; fi
  if [[ "$stored_sha" != "$dfsha" ]]; then echo "its Dockerfile or inputs changed since build"; return 1; fi
  if ! reason="$(image_has_lineage "$ref" "$key" "$mirror")"; then echo "$reason"; return 1; fi
  if [[ "$STRICT_LINEAGE" == 1 && -n "$parent" ]] && docker image inspect "$parent" >/dev/null 2>&1 \
     && ! tier_built_on_current_parent "$ref" "$parent"; then
    echo "is not layered on the current $parent"
    return 1
  fi
  return 0
}

# `sha256sum` is GNU coreutils; a stock macOS host only has `shasum`. This
# script gates both `just dev-up` and `just ci-docker`, so it must not abort
# under `set -e` on a Mac without coreutils.
sha256_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  else
    shasum -a 256 | cut -d' ' -f1
  fi
}

# Build inputs each tier consumes BEYOND its own Dockerfile, relative to
# $CONTEXT. These participate in the freshness label so a change to a COPYed or
# cooked input invalidates the tier even when the Dockerfile is untouched.
# `base` and `base-node` COPY nothing from the context (see the header note), so
# their Dockerfile alone is the complete input set.
extra_inputs_of() {
  case "$1" in
    base|base-node|base-rust) ;;
    base-rust-warm)
      # The warm tier cargo-chef-cooks the whole workspace, so its real inputs
      # are the dependency graph: the workspace manifests plus EVERY member
      # crate manifest. Discover the crate list rather than enumerating it. An
      # enumerated list stops covering new crates the moment the workspace is
      # restructured — which is precisely how a stale warm tier would go
      # unnoticed after a crate split.
      echo "Cargo.toml Cargo.lock .cargo/config.toml"
      # Build scripts and any toolchain file are cargo-chef recipe inputs too:
      # a build.rs edit changes the cook without touching a manifest. (Target
      # TOPOLOGY — names of bins/tests/benches/examples — is folded into the
      # freshness digest separately, names-only, by topology_names below:
      # hashing those files' CONTENTS here would rebuild the warm tier on
      # every test edit and destroy the acceleration it exists to provide.)
      ( cd "$CONTEXT" 2>/dev/null && LC_ALL=C ls -1 rust-toolchain rust-toolchain.toml */build.rs backend/*/build.rs tests/e2e/build.rs 2>/dev/null ) || true
      # LC_ALL=C so this listing is byte-identical across hosts: `ls` sorts by the
      # active collation, and the freshness hash below concatenates the manifests
      # in the order they arrive. Two hosts with different LC_COLLATE would
      # otherwise derive different hashes for identical inputs and each force a
      # full cargo-chef cook.
      ( cd "$CONTEXT" 2>/dev/null && LC_ALL=C ls -1 */Cargo.toml backend/*/Cargo.toml tests/e2e/Cargo.toml 2>/dev/null ) || true
      ;;
  esac
}

# Names-only target-topology digest input for the warm tier: cargo-chef's
# recipe changes when a target APPEARS or DISAPPEARS (src/bin, tests, benches,
# examples, main/lib presence), independent of those files' contents.
topology_names() {
  case "$1" in
    base-rust-warm)
      ( cd "$CONTEXT" 2>/dev/null && LC_ALL=C ls -1 \
          */src/main.rs */src/lib.rs */src/bin/*.rs \
          */tests/*.rs */benches/*.rs */examples/*.rs \
          backend/*/src/main.rs backend/*/src/lib.rs backend/*/src/bin/*.rs \
          backend/*/tests/*.rs backend/*/benches/*.rs backend/*/examples/*.rs \
          tests/e2e/src/main.rs tests/e2e/src/lib.rs tests/e2e/src/bin/*.rs \
          tests/e2e/tests/*.rs tests/e2e/benches/*.rs tests/e2e/examples/*.rs 2>/dev/null \
        | sed 's/^/topo:/' ) || true
      ;;
  esac
}

dockerfile_of() {
  case "$1" in
    base)                echo "Dockerfile.base" ;;
    base-rust|base-node) echo "Dockerfile.${1}" ;;
    base-rust-warm)      echo "Dockerfile.base-rust-warm" ;;
  esac
}

# Push a tier that is current locally to the mirror when the mirror lacks it.
# Best-effort: the key is content-derived, so a concurrent push of the same key
# is an identical image, and a failed push only means the next caller builds
# locally again. Called for freshly built tiers AND for tiers found current
# locally: a sibling build may have built the tier without a mirror, and a
# publication run must still publish it.
publish_tier() {
  local ref="$1" mirror="$2" newest="${3:-}" replace="${4:-0}"
  [[ -n "$mirror" ]] || return 0
  [[ "$MIRROR_PUSH" == 1 ]] || return 0
  # An entry this run pulled and rejected (wrong freshness or lineage label)
  # is replaced by the local build ($replace), never left for every later
  # caller to reject again.
  if [[ "$MIRROR_PULL" == 1 && "$replace" != 1 ]] && docker manifest inspect "$mirror" >/dev/null 2>&1; then
    echo "Base tier already mirrored: $mirror"
    return 0
  fi
  if docker tag "$ref" "$mirror" && docker push "$mirror" >/dev/null 2>&1; then
    echo "Base tier pushed to mirror: $mirror"
  else
    echo "WARN: could not push base tier to mirror $mirror; later callers will build it locally" >&2
    return 0
  fi
  # A fresh publication is the newest build on this parent: move the name a
  # guest falls back to while its exact key is not published yet.
  [[ -n "$newest" ]] || return 0
  if docker tag "$ref" "$newest" && docker push "$newest" >/dev/null 2>&1; then
    echo "Base tier published as newest on its parent: $newest"
  else
    echo "WARN: could not move $newest; guests keep falling back to the previous newest tier" >&2
  fi
}

# A publishing run that pulled $mirror as a hit has nothing to push, but the
# newest name may be missing when an earlier run's exact push succeeded and
# its newest push failed. Restore it then; a name that exists is left alone,
# so a superseded deploy running late never moves it back to an older tier.
restore_newest_name() {
  local ref="$1" mirror="$2" newest="${3:-}"
  [[ "$MIRROR_PUSH" == 1 && -n "$newest" ]] || return 0
  docker manifest inspect "$newest" >/dev/null 2>&1 && return 0
  if docker tag "$ref" "$newest" && docker push "$newest" >/dev/null 2>&1; then
    echo "Base tier newest name restored from the mirrored tier: $newest"
  else
    echo "WARN: could not restore $newest; guests keep falling back to the previous newest tier" >&2
  fi
}

# Take the mirror's newest build of $tier on the current parent as the local
# $ref when the exact key is not published yet. The image keeps the labels of
# the recipe it was built from, so the next run looks for the exact key again
# and replaces it once delivery has published it. Prints what it did.
take_newest_tier() {
  local tier="$1" ref="$2" key="$3" newest="$4" newest_key
  [[ "$FALLBACK_NEWEST" == 1 && "$STRICT_LINEAGE" != 1 && -n "$newest" ]] || return 1
  docker pull "$newest" >/dev/null 2>&1 || return 1
  docker tag "$newest" "$ref" || return 1
  newest_key="$(docker image inspect -f '{{index .Config.Labels "g5.tier.key"}}' "$ref" 2>/dev/null || true)"
  if [[ -z "$newest_key" ]]; then
    echo "WARN: newest mirrored tier $newest carries no lineage key; building locally" >&2
    return 1
  fi
  echo "Base tier not yet published for key $key; using the newest tier on the same parent: $ref <- $newest (lineage $newest_key)"
}

ensure_tier() {
  local tier="$1"
  tier_done "$tier" && return 0
  case "$tier" in
    base|base-rust|base-rust-warm|base-node) ;;
    *)
      echo "ERROR: unknown base tier '$tier' (expected base, base-rust, base-rust-warm, or base-node)" >&2
      exit 2
      ;;
  esac
  compute_tier_key "$tier"
  local ref="g5/${tier}:${TAG}" dfsha key mirror newest parent_tier parent dfpath reason rejected=0
  dfsha="$(get_tier_dfsha "$tier")"
  key="$(get_tier_key "$tier")"
  mirror="$(mirror_ref_of "$tier" "$key")"
  newest="$(newest_ref_of "$tier")"
  parent_tier="$(parent_tier_of "$tier")"
  parent="$(parent_ref_of "$tier")"
  dfpath="${DF_DIR}/$(dockerfile_of "$tier")"

  if docker image inspect "$ref" >/dev/null 2>&1; then
    if reason="$(image_is_current "$ref" "$dfsha" "$key" "$mirror" "$parent")"; then
      echo "Base tier already present and current: $ref"
      publish_tier "$ref" "$mirror" "$newest"
      set_tier_done "$tier"
      return 0
    fi
    echo "Base tier present but $reason — rebuilding: $ref"
  fi

  # Mirror hit, tried BEFORE any parent is ensured: the key already proves
  # the recipe and the whole parent lineage, so a pulled tier needs no parent
  # locally (dev-up pulls base-rust-warm without ever pulling base-rust).
  # The recipe label and a populated lineage label are re-checked so a
  # corrupted or hand-pushed entry degrades to a local build instead of a
  # stale tier; and under strict lineage (callers that run the single-base
  # oracle ensure parents first) a pulled tier whose parent is present must
  # layer on it, or it is rebuilt on it as before. stderr stays visible so an
  # auth or network failure reads differently from a plain miss.
  if [[ -n "$mirror" && "$MIRROR_PULL" == 1 ]] && docker pull "$mirror" >/dev/null; then
    docker tag "$mirror" "$ref"
    local pulled_sha
    pulled_sha="$(docker image inspect -f '{{index .Config.Labels "g5.tier.dockerfile_sha"}}' "$ref" 2>/dev/null || true)"
    if [[ "$pulled_sha" != "$dfsha" ]]; then
      echo "WARN: mirrored base tier $mirror carries freshness label '${pulled_sha:-<none>}', expected '$dfsha'; building locally" >&2
      rejected=1
    elif ! reason="$(image_has_lineage "$ref" "$key" "$mirror")"; then
      echo "WARN: mirrored base tier $mirror $reason; building locally" >&2
      rejected=1
    elif [[ "$STRICT_LINEAGE" == 1 && -n "$parent" ]] && docker image inspect "$parent" >/dev/null 2>&1 \
         && ! tier_built_on_current_parent "$ref" "$parent"; then
      echo "WARN: mirrored base tier $mirror is not layered on the current $parent; building locally" >&2
    else
      echo "Base tier pulled from mirror: $ref <- $mirror"
      restore_newest_name "$ref" "$mirror" "$newest"
      set_tier_done "$tier"
      return 0
    fi
  elif [[ -n "$mirror" && "$MIRROR_PULL" == 1 ]]; then
    # Not published under this key yet: a guest that opted in takes the newest
    # tier on the same parent instead of cooking (g5.runtime.cache.tier-fallback).
    if take_newest_tier "$tier" "$ref" "$key" "$newest"; then
      set_tier_done "$tier"
      return 0
    fi
    echo "Base tier mirror pull failed or image not found (building locally): $mirror"
  elif [[ -n "$mirror" ]]; then
    echo "Base tier mirror is publish-only for this run (building locally): $mirror"
  fi

  # A local build layers on its parent, which must exist and be current.
  local build_args=()
  if [[ -n "$parent_tier" ]]; then
    ensure_tier "$parent_tier"
    # Ensuring the parent may have replaced it (a stale local parent pulled
    # fresh from the mirror). A tier rejected above only for not layering on
    # the OLD parent may layer on the new one: re-check before building, so a
    # valid mirrored tier is never cooked locally because of its parent.
    if docker image inspect "$ref" >/dev/null 2>&1 \
       && image_is_current "$ref" "$dfsha" "$key" "$mirror" "$parent" >/dev/null; then
      echo "Base tier current on the refreshed $parent: $ref"
      publish_tier "$ref" "$mirror" "$newest"
      set_tier_done "$tier"
      return 0
    fi
    # The toolchain tiers are `FROM ${BASE_IMAGE}`; pass the parent explicitly
    # so the tier layers onto the tag this run just ensured.
    build_args=(--build-arg "BASE_IMAGE=g5/${parent_tier}:${TAG}")
  fi

  echo "Building base tier locally: $ref (-f ${dfpath})"
  # `base` takes no --build-arg, so build_args is empty there. Bash 3.2 (macOS
  # system bash) rejects a bare "${build_args[@]}" on an empty array under
  # `set -u`; the `+` form emits nothing when empty and preserves argument
  # boundaries when populated.
  docker build \
    ${build_args[@]+"${build_args[@]}"} \
    --label "g5.tier.dockerfile_sha=${dfsha}" \
    --label "g5.tier.key=${key}" \
    --tag "$ref" \
    --file "${dfpath}" \
    "$CONTEXT"

  publish_tier "$ref" "$mirror" "$newest" "$rejected"
  set_tier_done "$tier"
}

# Validate the requested tiers up front; ensure_tier resolves the dependency
# order itself (a parent is ensured only when a child must be built locally).
for tier in "$@"; do
  case "$tier" in
    base|base-rust|base-rust-warm|base-node) ;;
    *)
      echo "ERROR: unknown base tier '$tier' (expected base, base-rust, base-rust-warm, or base-node)" >&2
      exit 2
      ;;
  esac
done

# Serialize tier builds across concurrent callers.
#
# `g5/base*:<tag>` is a SHARED MUTABLE tag. Several pipeline steps call this
# script for themselves (cloudbuild-ci-e2e's api-runtime-shell, api-image,
# worker-base, edge, and oauth-stub steps, two of which start with no ordering
# between them), so without a lock the freshness check and the `docker build
# --tag` that follows straddle a time-of-check-to-time-of-use window: one caller
# can inspect the tag while a sibling replaces it, and a leaf build can resolve
# a tag mid-rewrite.
#
# The lock MUST live somewhere every caller can see. Cloud Build runs each step
# in its own container that shares only the Docker daemon and `/workspace` —
# `/tmp` is per-container, so a lock under TMPDIR would give each caller its own
# private lock and serialize nothing. Prefer `/workspace` when it exists and is
# writable, and fall back to TMPDIR locally, where every caller is on one host.
#
# The lock is a directory, not `flock`: flock(1) is absent on a stock macOS
# host, which this script also gates. `mkdir` alone does not decide the winner:
# uutils coreutils (Ubuntu 26.04's `mkdir`, as in the CI builder image) reports
# success to every caller racing to create the same directory. The holder is
# whoever creates the `owner` file exclusively (see claim_lock).
if [ -z "${G5_BASE_LOCK_DIR:-}" ]; then
  if [ -d /workspace ] && [ -w /workspace ]; then
    G5_BASE_LOCK_DIR="/workspace/.g5-base-images-${TAG}.lock"
  else
    G5_BASE_LOCK_DIR="${TMPDIR:-/tmp}/g5-base-images-${TAG}.lock"
  fi
fi
G5_BASE_LOCK_HEARTBEAT="${G5_BASE_LOCK_DIR}/heartbeat"
G5_BASE_LOCK_OWNER="${G5_BASE_LOCK_DIR}/owner"
# The holder refreshes the heartbeat every 30s, so 300s of silence means it is
# genuinely gone. Liveness MUST NOT be judged by the lock directory's own age
# alone: a cold `base-rust-warm` cook compiles the whole workspace dependency
# graph and can legitimately hold the lock far longer than any fixed timeout.
G5_BASE_LOCK_HEARTBEAT_INTERVAL="${G5_BASE_LOCK_HEARTBEAT_INTERVAL:-30}"
G5_BASE_LOCK_STALE_AFTER="${G5_BASE_LOCK_STALE_AFTER:-300}"
G5_BASE_LOCK_HELD=0
G5_BASE_HEARTBEAT_PID=""
# Ownership token: release (and the heartbeat loop) only ever touch the lock
# while this token still matches the owner file, so a manually-cleaned or
# replaced lock is never torn down by a stale process.
G5_BASE_LOCK_TOKEN="$(od -An -N12 -tx1 /dev/urandom 2>/dev/null | tr -d ' \n' || echo "")${$}"

file_age_seconds() {
  local now mtime
  now="$(date +%s)"
  # GNU stat (Linux) is `-c %Y`; BSD stat (macOS) is `-f %m`. Probe GNU FIRST:
  # on Linux, GNU's `-f` means --file-system and SUCCEEDS with unrelated output
  # containing "File:", so a BSD-first probe does not fall through — it returns
  # non-numeric text that then expands `File` as a variable inside $(( )) and
  # aborts under `set -u`. Validate numerically regardless, so any unexpected
  # output degrades to "very old" instead of killing the script. Callers must
  # treat that sentinel as "could not read", not as proof of death: the path
  # may simply have been released between their `mkdir` and this read
  # (acquire_lock re-checks the directory before judging).
  mtime="$(stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null || echo "")"
  case "$mtime" in
    ''|*[!0-9]*) echo 999999; return ;;
  esac
  echo $(( now - mtime ))
}

# Age of the CURRENT holder's liveness signal. Falls back to the lock directory
# itself when the heartbeat file is not there yet: between `mkdir` (the atomic
# acquire) and the first heartbeat write there is a window where a missing file
# would otherwise read as infinitely old and let a waiter steal a lock that was
# just legitimately taken. A holder that died inside that window still ages out,
# because the directory's own mtime keeps advancing past the threshold.
holder_age_seconds() {
  if [ -f "$G5_BASE_LOCK_HEARTBEAT" ]; then
    file_age_seconds "$G5_BASE_LOCK_HEARTBEAT"
  else
    file_age_seconds "$G5_BASE_LOCK_DIR"
  fi
}

i_own_the_lock() {
  [ "$(cat "$G5_BASE_LOCK_OWNER" 2>/dev/null || echo "")" = "$G5_BASE_LOCK_TOKEN" ]
}

# One acquisition attempt. Bash's noclobber opens the owner file with O_EXCL,
# so exactly one of the callers whose `mkdir` reported success claims it; the
# others see a held lock (or one released mid-claim) and wait like any caller
# whose `mkdir` failed. Prints the reason on failure.
claim_lock() {
  mkdir "$G5_BASE_LOCK_DIR" 2>&1 || return 1
  if ( set -C; printf '%s' "$G5_BASE_LOCK_TOKEN" > "$G5_BASE_LOCK_OWNER" ) 2>/dev/null; then
    return 0
  fi
  echo "${G5_BASE_LOCK_DIR} was claimed by a concurrent caller"
  return 1
}

release_lock() {
  if [ "$G5_BASE_LOCK_HELD" = "1" ]; then
    [ -n "$G5_BASE_HEARTBEAT_PID" ] && kill "$G5_BASE_HEARTBEAT_PID" 2>/dev/null || true
    # Only tear down a lock we still own. If a waiter reclaimed it after judging
    # us stale, the directory now belongs to someone else and must not be touched.
    if i_own_the_lock; then
      rm -f "$G5_BASE_LOCK_HEARTBEAT" "$G5_BASE_LOCK_OWNER" 2>/dev/null || true
      rmdir "$G5_BASE_LOCK_DIR" 2>/dev/null || true
    fi
    G5_BASE_LOCK_HELD=0
  fi
}

acquire_lock() {
  local announced=0 age silent_polls=0 vanished_polls=0 mkdir_err
  while ! mkdir_err="$(claim_lock)"; do
    age="$(holder_age_seconds)"
    # The age read is NOT atomic with the failed mkdir. A sibling that holds
    # the lock for a sub-second freshness check can release in between —
    # `rm` heartbeat+owner, then `rmdir` — at which point `stat` fails and
    # file_age_seconds degrades to its "unreadable is very old" sentinel. A
    # lock that is GONE was released, not abandoned: retry the acquire at once
    # instead of pronouncing it stale. (Dev deploy builds 2f51f728 and
    # 74214325 died exactly here, a minute into the build, with "no heartbeat
    # for 999999s" while the lock was changing hands between six waiters.)
    # Bounded: a directory that stays absent while mkdir keeps failing is a
    # filesystem error, reported with mkdir's own message.
    if [ ! -d "$G5_BASE_LOCK_DIR" ]; then
      silent_polls=0
      vanished_polls=$((vanished_polls + 1))
      if [ "$vanished_polls" -ge 30 ]; then
        echo "ERROR: cannot create base-image lock ${G5_BASE_LOCK_DIR}: ${mkdir_err}" >&2
        exit 1
      fi
      sleep 1
      continue
    fi
    vanished_polls=0
    if [ "$age" -gt "$G5_BASE_LOCK_STALE_AFTER" ]; then
      # A stale verdict needs the lock PRESENT and silent on two consecutive
      # polls: the heartbeat file is legitimately absent for an instant on a
      # live lock (between mkdir and the first heartbeat write, and during
      # release), so one silent read is evidence of motion, not of death. A
      # release in progress cannot span two polls; a dead holder still fails
      # loud, one poll later than before.
      silent_polls=$((silent_polls + 1))
      if [ "$silent_polls" -ge 2 ]; then
        # DELIBERATELY no automatic stealing. Every reclamation scheme we
        # tried (age-checked rename-aside, post-move liveness restore) still
        # had a split-brain window where two callers could both believe they
        # owned the tag namespace and publish concurrently — worse than
        # failing. Stale locks are also vanishingly rare where this runs:
        # Cloud Build private pools give every build a fresh /workspace, and
        # locally a dead holder means a killed dev command on your own
        # machine. So fail loud with the exact cleanup command and let a
        # human (or the CI retry) decide.
        echo "ERROR: base-image lock at ${G5_BASE_LOCK_DIR} has had no heartbeat for ${age}s" >&2
        echo "       (holder likely died). After confirming no 'docker build' for the" >&2
        echo "       g5/base tiers is still running, remove it and re-run:" >&2
        echo "         rm -rf '${G5_BASE_LOCK_DIR}'" >&2
        exit 1
      fi
    else
      silent_polls=0
    fi
    if [ "$announced" = "0" ]; then
      echo "Waiting for a concurrent base-image build to finish (${G5_BASE_LOCK_DIR})..."
      announced=1
    fi
    # No overall wait deadline while the holder heartbeats: it is making
    # progress, and the surrounding CI step timeout is the correct backstop.
    sleep 5
  done
  G5_BASE_LOCK_HELD=1
  # Heartbeat initialization is PART of acquisition: without a readable owner
  # token, release could not prove ownership and the dir would wedge every
  # later run until the stale-lock failure fires. Fail loud instead.
  if ! : > "$G5_BASE_LOCK_HEARTBEAT" \
     || ! i_own_the_lock; then
    rm -f "$G5_BASE_LOCK_HEARTBEAT" "$G5_BASE_LOCK_OWNER" 2>/dev/null || true
    rmdir "$G5_BASE_LOCK_DIR" 2>/dev/null || true
    G5_BASE_LOCK_HELD=0
    echo "ERROR: could not initialize base-image lock ownership at ${G5_BASE_LOCK_DIR}" >&2
    exit 1
  fi
  # The heartbeat stops the moment we stop owning the lock or the owner
  # process dies, so a killed holder ages out instead of wedging waiters.
  _owner_pid=$$
  ( while kill -0 "$_owner_pid" 2>/dev/null && i_own_the_lock; do
      : > "$G5_BASE_LOCK_HEARTBEAT" 2>/dev/null || exit 0
      sleep "$G5_BASE_LOCK_HEARTBEAT_INTERVAL"
    done ) &
  G5_BASE_HEARTBEAT_PID=$!
  # EXIT releases; INT/TERM must also TERMINATE — a bash trap without an exit
  # resumes execution after the handler, which would keep building (and
  # publishing tags) on a lock that was just released.
  trap release_lock EXIT
  trap 'release_lock; trap - EXIT; exit 130' INT
  trap 'release_lock; trap - EXIT; exit 143' TERM
}

acquire_lock

# Each requested tier, leaf first: a mirrored leaf is pulled by its key and
# needs no parent; a leaf that must be built pulls or builds its parents.
for tier in "$@"; do
  ensure_tier "$tier"
done

# The lock serializes TIER builds only; it is deliberately released before the
# caller's leaf build resolves the mutable g5/base*:<tag> names. Holding a lease
# through every dependent leaf build would serialize all image work on the host
# for minutes at a time. The residual window is accepted: tiers are retagged
# only when their content hash changes, Cloud Build private-pool workers run one
# build per VM (no cross-build daemon sharing), and the local legacy builder
# resolves each FROM when its stage starts, so a mid-build retag yields at worst
# a same-generation tier from a sibling worktree of the same repo.
release_lock

echo "Base image tiers ensured: $*"
