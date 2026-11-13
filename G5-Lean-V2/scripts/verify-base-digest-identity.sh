#!/usr/bin/env bash
# Verify the single-external-base property (test_contract/base_image_consolidation
# [D2], docker_images [D18]): every G5-built image passed as an argument must
# carry the CURRENT local g5/base tier's RootFS layers as an exact prefix of its
# own RootFS — i.e. it transitively derives from the one common base, byte-for-
# byte. This is the automated oracle for scenario D2; it runs after the local
# `just ci-docker` Phase 3 image builds (and can be pointed at any built set).
#
# Usage: verify-base-digest-identity.sh <image> [<image> ...]
#   G5_BASE_IMAGE_REF   Base ref to verify against (default g5/base:26.04).
#
# Exits non-zero naming every image whose ancestry does not match. Images that
# are deliberately outside the property (the postgres service image, the
# public customer default, the Playwright runner, the tungsten maven builder
# stage) must simply not be passed in — the exclusion list lives with [D18].
set -euo pipefail

BASE_REF="${G5_BASE_IMAGE_REF:-g5/base:26.04}"

# --static-scan: daemon-free fleet completeness check. Enumerates EVERY
# Dockerfile under infra/docker (plus oauth-stub) and classifies every
# FROM line: it must resolve to a g5/base* tier, an internal stage/ref, or a
# name on the [D18] exclusion list — an unclassified external base fails, so
# an image the runtime oracle was never pointed at cannot slip a new
# distribution in silently.
if [[ "${1:-}" == "--static-scan" ]]; then
  root="${2:-infra/docker}"
  if [[ ! -d "$root" ]]; then
    echo "FAIL: static fleet scan directory does not exist: $root" >&2
    exit 1
  fi
  dockerfiles="$(find "$root" -name 'Dockerfile*' -type f | sort)" || exit 1
  if [[ -z "$dockerfiles" ]]; then
    echo "FAIL: static fleet scan found no Dockerfiles in $root" >&2
    exit 1
  fi
  fail=0
  # [D18] exclusions and internal tiers, ANCHORED exactly — an unanchored
  # g5/base prefix would admit lookalikes like g5/base-attacker:latest.
  allowed='^(g5/base:26\.04|g5/base-rust:26\.04|g5/base-rust-warm:26\.04|g5/base-node:26\.04|g5/g5d-base:latest|g5/g5d:latest|scratch)$|^ubuntu:26\.04@sha256:[0-9a-f]{64}$|^mcr\.microsoft\.com/playwright:v[0-9.]+-[a-z]+(@sha256:[0-9a-f]{64})?$|^maven:3\.9-eclipse-temurin-17@sha256:[0-9a-f]{64}$|^g5-ci-builder$'
  # g5.scope.deployment.images.caddy: this reviewed multi-platform image is
  # only a binary source; its Alpine layers cannot become runtime ancestry.
  caddy_binary_source='caddy:2.11.4-alpine@sha256:6aeddd44c3078b0f9a35206472a11420648a79c184603ef95957d0a20044cb2b'
  normalized_dockerfile_instructions() {
    awk -v bom=$'\357\273\277' '
      BEGIN { directive_phase=1 }
      NR == 1 { sub("^" bom, "") }
      directive_phase {
        directive=tolower($0)
        if (directive ~ /^[[:space:]]*#[[:space:]]*(syntax|escape|check)[[:space:]]*=[[:space:]]*[^[:space:]]/) {
          if (directive ~ /^[[:space:]]*#[[:space:]]*escape[[:space:]]*=/) {
            sub(/^[[:space:]]*#[[:space:]]*escape[[:space:]]*=[[:space:]]*/, "", directive)
            sub(/[[:space:]]*$/, "", directive)
            if (directive != "\\") {
              print "FAIL: " FILENAME ": custom Dockerfile escape directive is unsupported by static scan" > "/dev/stderr"
              exit 1
            }
          }
        } else {
          # Docker stops reading directives after an instruction, ordinary
          # comment or blank row; later escape-shaped comments are inactive.
          directive_phase=0
        }
      }
      /^[[:space:]]*#/ { next }
      {
        # Match BuildKit: an escape preceded by another escape is literal;
        # continued bytes concatenate, skipping comment and empty rows.
        while ($0 ~ /(^|[^\\])\\[[:space:]]*$/) {
          sub(/\\[[:space:]]*$/, "")
          line=$0
          if (getline <= 0) break
          while ($0 ~ /^[[:space:]]*(#|$)/) { if (getline <= 0) break }
          $0=line $0
        }
        sub(/^[[:space:]]+/, "")
        opcode=toupper($1)
        if ((opcode == "COPY" || opcode == "ADD" || opcode == "RUN" || opcode == "ONBUILD") && index($0, "<<")) {
          print "FAIL: " FILENAME ": Dockerfile heredoc syntax is unsupported by static scan" > "/dev/stderr"
          exit 1
        }
        print
      }
    ' "$1"
  }
  source_providers() {
    awk '
      function trim(value) {
        sub(/^[[:space:]]+/, "", value); sub(/[[:space:]]+$/, "", value)
        return value
      }
      function unquote(value, first) {
        value=trim(value); first=substr(value, 1, 1)
        if ((first == "\042" || first == "\047") && substr(value, length(value), 1) == first) {
          value=substr(value, 2, length(value)-2)
        }
        return value
      }
      function emit_provider(value) {
        if (deferred) {
          print "FAIL: deferred ONBUILD source provider scope is unsupported by static scan" > "/dev/stderr"
          exit 1
        }
        if (value == "") {
          print "FAIL: empty source provider is unsupported by static scan" > "/dev/stderr"
          exit 1
        }
        print value "\t" $0
      }
      {
        opcode=toupper($1)
        rest=$0; sub(/^[^[:space:]]+[[:space:]]+/, "", rest)
        deferred=(opcode == "ONBUILD")
        if (deferred) {
          opcode=toupper(rest); sub(/[[:space:]].*$/, "", opcode)
          sub(/^[^[:space:]]+[[:space:]]+/, "", rest)
        }
        if (opcode != "COPY" && opcode != "RUN") next
        # Only leading instruction flags own providers. Shell prose and COPY
        # paths containing from= do not select an image.
        while (rest ~ /^--/) {
          quote=""
          for (position=1; position<=length(rest); position++) {
            character=substr(rest, position, 1)
            if (quote != "") {
              if (character == quote) quote=""
            } else if (character == "\042" || character == "\047") {
              quote=character
            } else if (character ~ /[[:space:]]/) break
          }
          token=substr(rest, 1, position-1); rest=trim(substr(rest, position))
          if (toupper(token) ~ /^--FROM=/ && opcode == "COPY") {
            if (quote != "" || index(token, "\\")) {
              print "FAIL: COPY source flag syntax is unsupported by static scan" > "/dev/stderr"
              exit 1
            }
            emit_provider(unquote(substr(token, 8)))
          } else if (toupper(token) ~ /^--MOUNT=/ && opcode == "RUN") {
            if (quote != "" || index(token, "\\")) {
              print "FAIL: source mount flag syntax is unsupported by static scan" > "/dev/stderr"
              exit 1
            }
            mount=unquote(substr(token, 9))
            if (index(mount, "\042") || index(mount, "\047") || mount == "") {
              print "FAIL: quoted or empty mount CSV syntax is unsupported by static scan" > "/dev/stderr"
              exit 1
            }
            count=split(mount, fields, ",")
            for (field=1; field<=count; field++) {
              value=trim(fields[field])
              if (value == "") {
                print "FAIL: empty mount CSV field is unsupported by static scan" > "/dev/stderr"
                exit 1
              }
              if (toupper(value) ~ /^FROM[[:space:]]*=/) {
                sub(/^[^=]*=/, "", value); emit_provider(unquote(value))
              }
            }
          }
        }
      }
    '
  }
  while IFS= read -r df; do
    # FROM, ARG and source-access checks consume one instruction stream so
    # leading whitespace and continuations cannot shift numeric stage indexes.
    instructions="$(normalized_dockerfile_instructions "$df")"
    global_args="$(printf '%s\n' "$instructions" | awk -v df="$df" '
      toupper($1) == "FROM" { exit }
      toupper($1) == "ARG" {
        argument=$0
        sub(/^[^[:space:]]+[[:space:]]+/, "", argument)
        name=argument
        sub(/[=[:space:]].*$/, "", name)
        if (name !~ /^[A-Za-z_][A-Za-z0-9_]*$/ || seen[name]++) {
          print "FAIL: " df ": ambiguous or duplicate global ARG declaration is unsupported by static scan" > "/dev/stderr"
          exit 1
        }
        print argument
      }
    ')"
    stages=""
    stage_index=0
    caddy_source_index=""
    while IFS= read -r from; do
      ref="$(printf '%s' "$from" | awk '{print $2}')"
      alias="$(printf '%s' "$from" | awk 'toupper($3)=="AS"{print $4}')"
      # Resolve ${ARG} only from unique literal defaults before the first FROM.
      # Expressions/quotes/escapes are refused instead of guessing Docker's
      # evaluation order or recursively chasing defaults (which can cycle).
      # An ARG with no default — resolvable only by a caller override — is
      # REJECTED: the scan must classify what a bare `docker build` would do.
      while [[ "$ref" == *'${'* ]]; do
        var="$(printf '%s' "$ref" | sed -n 's/.*\${\([A-Za-z_][A-Za-z0-9_]*\)}.*/\1/p')"
        [ -n "$var" ] || break
        def="$(printf '%s\n' "$global_args" | sed -n "s/^${var}=//p")"
        if ! [[ "$def" =~ ^[A-Za-z0-9./_:@-]+$ ]]; then
          echo "FAIL: ${df}: FROM uses ARG '${var}' without a supported global literal default — unclassifiable ([D18]/[D2])" >&2
          fail=1; ref=""; break
        fi
        ref="${ref//\$\{${var}\}/${def}}"
      done
      [ -n "$ref" ] || continue
      if printf '%s\n' "$alias" | grep -qiFx "caddy-source" && printf '%s\n' "$stages" | grep -qiFx "caddy-source"; then
        echo "FAIL: ${df}: duplicate reserved caddy-source stage alias is unsupported by static scan" >&2
        fail=1
      elif [[ "$ref" == "$caddy_binary_source" ]]; then
        if [[ "$alias" != "caddy-source" ]]; then
          echo "FAIL: ${df}: pinned Caddy image is only allowed AS caddy-source" >&2
          fail=1
        else
          caddy_source_index="$stage_index"
        fi
      elif [[ -n "$caddy_source_index" && "$ref" == "caddy-source" ]]; then
        echo "FAIL: ${df}: caddy-source is a binary source, not a runtime ancestor" >&2
        fail=1
      elif printf '%s' "$stages" | grep -qFx "$ref" 2>/dev/null; then
        : # internal stage reference
      elif printf '%s' "$ref" | grep -qE "$allowed"; then
        : # tier or named, anchored exclusion
      else
        echo "FAIL: ${df}: unclassified external base '${ref}' ([D18]/[D2])" >&2
        fail=1
      fi
      [ -n "$alias" ] && stages="${stages}
${alias}"
      stage_index=$((stage_index + 1))
    done < <(printf '%s\n' "$instructions" | grep -iE '^from[[:space:]]')
    # Check every use of this source, including RUN mounts: any other access
    # could copy its Alpine userland while preserving the base-layer prefix.
    # Direct Caddy refs (tagless, tagged or digest-only) are checked even in
    # files without a pinned source. Numeric access needs that actual stage.
    stage_refs='caddy-source'
    source_stage_refs="$stage_refs"
    if [[ -n "$caddy_source_index" ]]; then
      stage_refs="${stage_refs}|${caddy_source_index}"
      # COPY resolves indexes with decimal Atoi: padded/positive forms and
      # signed zero access the same stage. Only canonical binary COPY is allowed.
      numeric_source_ref="[+]?0*${caddy_source_index}"
      [[ "$caddy_source_index" == 0 ]] && numeric_source_ref='[+-]?0+'
      source_stage_refs="${source_stage_refs}|${numeric_source_ref}"
    fi
    source_use="^(${source_stage_refs}|([^[:space:],'\"]*/)?caddy([:@][^[:space:],'\"]+)?|[$][^[:space:],'\"]+)$"
    binary_copy="^[Cc][Oo][Pp][Yy][[:space:]]+--from=(${stage_refs})[[:space:]]+/usr/bin/caddy[[:space:]]+/usr/bin/caddy[[:space:]]*$"
    providers="$(printf '%s\n' "$instructions" | source_providers)"
    while IFS=$'\t' read -r provider instruction; do
      [ -n "$provider" ] || continue
      if printf '%s\n' "$stages" | grep -qiFx -- "$provider"; then
        : # named stage; Caddy-specific access is restricted below
      elif awk -v reference="$provider" -v stages="$stage_index" 'BEGIN {
        exit !(reference ~ /^[+-]?[0-9]+$/ && reference+0 >= 0 && reference+0 < stages)
      }'; then
        : # actual decimal stage index, including padded and signed zero
      elif printf '%s\n' "$provider" | grep -qE "$allowed"; then
        : # the same explicit external tiers/exclusions as FROM
      else
        echo "FAIL: ${df}: unclassified COPY or RUN mount source provider '${provider}' ([D18]/[D2])" >&2
        fail=1
      fi
      if printf '%s\n' "$provider" | grep -qiE "$source_use" && { [[ -z "$caddy_source_index" ]] || ! printf '%s\n' "$instruction" | grep -qE "$binary_copy"; }; then
        echo "FAIL: ${df}: Caddy source may only be used by COPY of /usr/bin/caddy" >&2
        fail=1
      fi
    done <<< "$providers"
  done <<< "$dockerfiles"
  [ "$fail" -eq 0 ] && echo "OK: static fleet scan — every FROM classifies"
  exit "$fail"
fi

if [[ $# -eq 0 ]]; then
  echo "Usage: verify-base-digest-identity.sh [--static-scan [dir]] | <image> [<image> ...]" >&2
  exit 2
fi

layers_of() {
  docker image inspect -f '{{range .RootFS.Layers}}{{println .}}{{end}}' "$1"
}

base_layers="$(layers_of "$BASE_REF")" || {
  echo "ERROR: base image ${BASE_REF} is not present locally" >&2
  exit 2
}
n_base="$(printf '%s' "$base_layers" | grep -c .)"
[[ "$n_base" -gt 0 ]] || { echo "ERROR: ${BASE_REF} reports zero layers" >&2; exit 2; }

fail=0
for image in "$@"; do
  if ! img_layers="$(layers_of "$image" 2>/dev/null)"; then
    echo "FAIL: ${image} — not present locally, cannot verify ancestry" >&2
    fail=1
    continue
  fi
  if [[ "$(printf '%s' "$img_layers" | sed -n "1,${n_base}p")" == "$(printf '%s' "$base_layers")" ]]; then
    echo "OK:   ${image} shares ${BASE_REF}'s ${n_base} base layer(s)"
  else
    echo "FAIL: ${image} does NOT derive from the current ${BASE_REF} ([D18]/[D2])" >&2
    fail=1
  fi
done
exit "$fail"
