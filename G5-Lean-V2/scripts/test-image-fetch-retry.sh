#!/usr/bin/env bash
# Executable contract for the bounded transient-retry around two deploy-lane
# image fetches (software_architecture/infra/docker_images [I32] helm and
# [D10] Azure Functions Core Tools; test_contract/base_image_consolidation
# [D14]): the helm tarball curl in infra/docker/Dockerfile.g5d-g5dev and the
# preview-tool `npm install -g` in infra/docker/Dockerfile.g5d-pairsoft-preview.
#
# It extracts the exact RUN text from each Dockerfile (rewriting only the
# install prefixes to a scratch dir) and executes it under the Dockerfiles'
# `bash -o pipefail` shell with PATH-shadowed `curl` / `npm` doubles that
# count invocations, so the retry budget and the fail-closed exit are proven
# on the shipped command text, not on a copy.
#
# Run by the ci-rust `image-build-script-contracts` step; also runnable
# locally from  (bash + coreutils + tar; no Docker).
set -euo pipefail
cd "$(dirname "$0")/.."

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT
mkdir -p "$TEST_ROOT/bin" "$TEST_ROOT/usr-local-bin" "$TEST_ROOT/usr-bin" "$TEST_ROOT/tmp"

# Extract the RUN instruction whose text contains $2 from Dockerfile $1: the
# `RUN` line plus every backslash-continued line, with the `RUN ` prefix and
# the continuation backslashes removed.
extract_run() {
  local file="$1" needle="$2"
  awk -v needle="$needle" '
    /^RUN / { collecting=1; block=""; hit=0 }
    collecting {
      line=$0
      if (index(line, needle)) hit=1
      sub(/\\[ \t]*$/, "", line)
      if (block == "") { sub(/^RUN /, "", line); block=line } else { block=block " " line }
      if ($0 !~ /\\[ \t]*$/) {
        collecting=0
        if (hit) { print block; exit }
      }
    }' "$file"
}

fail() { echo "FAIL: $*" >&2; [ -n "${OUT:-}" ] && sed 's/^/  | /' "$OUT" >&2; exit 1; }

dockerfile_arg() {
  # ARG NAME=value → value (first definition wins)
  grep -m1 -E "^ARG $2=" "$1" | sed -E "s/^ARG $2=//"
}

# ---------------------------------------------------------------------------
# helm fetch (Dockerfile.g5d-g5dev)
HELM_DF=infra/docker/Dockerfile.g5d-g5dev
HELM_RUN="$(extract_run "$HELM_DF" 'helm.tar.gz')"
[ -n "$HELM_RUN" ] || fail "could not extract the helm RUN from $HELM_DF"
# Rewrite /tmp/ FIRST: on Linux $TEST_ROOT itself lives under /tmp/, so a
# later /tmp/ pass would re-hit the scratch prefixes already substituted in.
HELM_RUN="${HELM_RUN//\/tmp\//$TEST_ROOT/tmp/}"
HELM_RUN="${HELM_RUN//\/usr\/local\/bin/$TEST_ROOT/usr-local-bin}"
bash -n <(printf '%s\n' "$HELM_RUN") || fail "helm RUN text does not parse"

# A real tarball with the exact member the RUN extracts.
mkdir -p "$TEST_ROOT/src/linux-amd64"
printf '#!/bin/sh\necho fake-helm\n' > "$TEST_ROOT/src/linux-amd64/helm"
tar -C "$TEST_ROOT/src" -czf "$TEST_ROOT/helm-good.tar.gz" linux-amd64/helm
GOOD_SHA="$(sha256sum "$TEST_ROOT/helm-good.tar.gz" | cut -d' ' -f1)"

# Fake curl: fail with exit 35 for the first $CURL_FAILURES calls (or always
# when CURL_FAILURES=always), then copy the good tarball to the -o target.
cat >"$TEST_ROOT/bin/curl" <<'FAKE'
#!/usr/bin/env bash
n=0; [ -f "$CURL_COUNT" ] && n="$(<"$CURL_COUNT")"; n=$((n+1)); printf '%s' "$n" > "$CURL_COUNT"
out=""; while [ $# -gt 0 ]; do case "$1" in -o) out="$2"; shift ;; esac; shift; done
if [ "$CURL_FAILURES" = always ] || [ "$n" -le "$CURL_FAILURES" ]; then
  echo "curl: (35) Recv failure: Connection reset by peer (fake attempt $n)" >&2; exit 35
fi
cp "$CURL_GOOD" "$out"
FAKE
chmod +x "$TEST_ROOT/bin/curl"

run_helm() {
  # $1 = CURL_FAILURES
  OUT="$TEST_ROOT/helm-$1.out"; rm -f "$TEST_ROOT/curl-count" "$TEST_ROOT/usr-local-bin/helm"
  set +e
  env PATH="$TEST_ROOT/bin:$PATH" CURL_FAILURES="$1" CURL_COUNT="$TEST_ROOT/curl-count" \
      CURL_GOOD="$TEST_ROOT/helm-good.tar.gz" \
      HELM_VERSION="$(dockerfile_arg "$HELM_DF" HELM_VERSION)" HELM_SHA256="$GOOD_SHA" \
      bash -o pipefail -c "$HELM_RUN" >"$OUT" 2>&1
  STATUS=$?
  set -e
  CALLS="$(<"$TEST_ROOT/curl-count")"
}

run_helm 2
[ "$STATUS" -eq 0 ] || fail "helm: two transient resets then success must install (exit $STATUS)"
[ "$CALLS" -eq 3 ] || fail "helm: expected 3 curl attempts, saw $CALLS"
[ -x "$TEST_ROOT/usr-local-bin/helm" ] || fail "helm: binary not installed after retry"
echo "PASS helm: transient reset retried, installed on attempt 3"

run_helm always
[ "$STATUS" -ne 0 ] || fail "helm: exhausted budget must fail the build"
[ "$CALLS" -eq 5 ] || fail "helm: expected exactly 5 curl attempts on a sustained outage, saw $CALLS"
grep -q "sha256sum" "$OUT" || grep -qi "FAILED\|No such file" "$OUT" || fail "helm: exhausted budget must fail at checksum verification"
[ ! -e "$TEST_ROOT/usr-local-bin/helm" ] || fail "helm: half-install left behind"
echo "PASS helm: sustained outage fails closed after 5 attempts"

# ---------------------------------------------------------------------------
# preview tools npm install (Dockerfile.g5d-pairsoft-preview)
NPM_DF=infra/docker/Dockerfile.g5d-pairsoft-preview
NPM_RUN="$(extract_run "$NPM_DF" 'azure-functions-core-tools@')"
[ -n "$NPM_RUN" ] || fail "could not extract the npm install RUN from $NPM_DF"
NPM_RUN="${NPM_RUN//\/usr\/local\/bin/$TEST_ROOT/usr-local-bin}"
NPM_RUN="${NPM_RUN//\/usr\/bin/$TEST_ROOT/usr-bin}"
bash -n <(printf '%s\n' "$NPM_RUN") || fail "npm RUN text does not parse"

cat >"$TEST_ROOT/bin/npm" <<'FAKE'
#!/usr/bin/env bash
n=0; [ -f "$NPM_COUNT" ] && n="$(<"$NPM_COUNT")"; n=$((n+1)); printf '%s' "$n" > "$NPM_COUNT"
case " $* " in *" install "*" -g "*|*" install -g "*) ;; *) echo "fake npm: unexpected args: $*" >&2; exit 64 ;; esac
if [ "$NPM_FAILURES" = always ] || [ "$n" -le "$NPM_FAILURES" ]; then
  echo "npm error attempting to GET https://cdn.functions.azure.com/... (fake attempt $n)" >&2
  echo "npm error Error: connect ETIMEDOUT 150.171.109.183:443" >&2
  exit 1
fi
: > "$NPM_MARK"
FAKE
chmod +x "$TEST_ROOT/bin/npm"

run_npm() {
  OUT="$TEST_ROOT/npm-$1.out"; rm -f "$TEST_ROOT/npm-count" "$TEST_ROOT/npm-ok" "$TEST_ROOT/usr-bin"/*
  set +e
  env PATH="$TEST_ROOT/bin:$PATH" NPM_FAILURES="$1" NPM_COUNT="$TEST_ROOT/npm-count" NPM_MARK="$TEST_ROOT/npm-ok" \
      AZURE_FUNCTIONS_CORE_TOOLS_VERSION="$(dockerfile_arg "$NPM_DF" AZURE_FUNCTIONS_CORE_TOOLS_VERSION)" \
      AZURITE_VERSION="$(dockerfile_arg "$NPM_DF" AZURITE_VERSION)" \
      SERVE_VERSION="$(dockerfile_arg "$NPM_DF" SERVE_VERSION)" \
      bash -o pipefail -c "$NPM_RUN" >"$OUT" 2>&1
  STATUS=$?
  set -e
  CALLS="$(<"$TEST_ROOT/npm-count")"
}

run_npm 2
[ "$STATUS" -eq 0 ] || fail "npm: two CDN timeouts then success must install (exit $STATUS)"
[ "$CALLS" -eq 3 ] || fail "npm: expected 3 npm attempts, saw $CALLS"
[ -f "$TEST_ROOT/npm-ok" ] || fail "npm: successful attempt did not run"
[ -L "$TEST_ROOT/usr-bin/func" ] || fail "npm: func launcher not linked after retry"
echo "PASS npm: transient CDN timeout retried, installed on attempt 3"

run_npm always
[ "$STATUS" -ne 0 ] || fail "npm: exhausted budget must fail the build"
[ "$CALLS" -eq 5 ] || fail "npm: expected exactly 5 npm attempts on a sustained outage, saw $CALLS"
[ ! -e "$TEST_ROOT/usr-bin/func" ] || fail "npm: launchers linked despite failed install"
echo "PASS npm: sustained outage fails closed after 5 attempts"

echo "image fetch retry tests passed"
