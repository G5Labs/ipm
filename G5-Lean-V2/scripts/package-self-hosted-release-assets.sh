#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_SRC="$ROOT/infra/self-hosted"
OUT_DIR="${1:-$ROOT/.self-hosted-release-assets}"
RELEASE_TAG="${G5_RELEASE_TAG:-${GITHUB_REF_NAME:-latest}}"
PUBLIC_RELEASE_REPO="${G5_GITHUB_RELEASE_REPO:-${G5_PUBLIC_INSTALLER_RELEASE_REPO:-G5Labs/Releases}}"

fail() {
  echo "self-hosted release asset packaging failed: $*" >&2
  exit 1
}

[ -d "$BUNDLE_SRC" ] || fail "missing self-hosted bundle source: $BUNDLE_SRC"
[ -n "$RELEASE_TAG" ] || fail "G5_RELEASE_TAG or GITHUB_REF_NAME is required"
case "$PUBLIC_RELEASE_REPO" in
  */*) ;;
  *) fail "G5_GITHUB_RELEASE_REPO must be OWNER/REPO, got: $PUBLIC_RELEASE_REPO" ;;
esac

case "$RELEASE_TAG" in
  v[0-9]*.[0-9]*.[0-9]*|latest) ;;
  *) fail "release tag must be semver-like (vMAJOR.MINOR.PATCH) or latest: $RELEASE_TAG" ;;
esac

case "$OUT_DIR" in
  /*) ;;
  *) OUT_DIR="$PWD/$OUT_DIR" ;;
esac

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

STAGE="$(mktemp -d)"
cleanup() {
  rm -rf "$STAGE"
}
trap cleanup EXIT

COPYFILE_DISABLE=1 cp -R "$BUNDLE_SRC/." "$STAGE/"

# The public worker-host installer stages the complete guest runtime from the
# immutable release worker image before launching `g5d host`. Ship the
# source-free staging program and its small support scripts in the bundle; the
# gated g5d binary, provider sidecars, and provider profiles remain in the
# licensed image and are extracted only after registry authorization.
install -m 0755 "$ROOT/scripts/stage-local-worker-runtime.sh" \
  "$STAGE/scripts/stage-local-worker-runtime.sh"
install -m 0755 "$ROOT/scripts/build-runtime-disk.sh" "$STAGE/scripts/build-runtime-disk.sh"
install -m 0755 "$ROOT/scripts/stage-guest-bootstrap.py" "$STAGE/scripts/stage-guest-bootstrap.py"
# build-runtime-disk.sh calls this unconditionally; the self-hosted staging
# container mounts the bundle's scripts directory, so the helper must travel.
install -m 0755 "$ROOT/scripts/stage-runtime-platform.sh" "$STAGE/scripts/stage-runtime-platform.sh"
install -m 0755 "$ROOT/scripts/apt-install-with-retry.sh" \
  "$STAGE/scripts/apt-install-with-retry.sh"
# The staging script bind-mounts this checker into its install container to
# prove the staged Claude Code CLI carries the runner flag surface
# (docs/lean-provider-cli-contract.md); the script refuses to run without it.
install -m 0755 "$ROOT/scripts/check-claude-cli-flags.sh" \
  "$STAGE/scripts/check-claude-cli-flags.sh"
install -m 0755 "$ROOT/scripts/check-codex-cli-flags.sh" \
  "$STAGE/scripts/check-codex-cli-flags.sh"
install -m 0644 "$ROOT/scripts/claude-code-cli-version" \
  "$STAGE/scripts/claude-code-cli-version"
install -m 0644 "$ROOT/scripts/codex-cli-version" \
  "$STAGE/scripts/codex-cli-version"
install -d -m 0755 "$STAGE/infra/packer/g5-worker/scripts/lib"
install -m 0755 "$ROOT/infra/packer/g5-worker/scripts/57-install-test-tools.sh" \
  "$STAGE/infra/packer/g5-worker/scripts/57-install-test-tools.sh"
install -m 0644 "$ROOT/infra/packer/g5-worker/scripts/lib/download-with-retry.sh" \
  "$STAGE/infra/packer/g5-worker/scripts/lib/download-with-retry.sh"

python3 - "$STAGE/install.sh" "$RELEASE_TAG" "$PUBLIC_RELEASE_REPO" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
release_tag = sys.argv[2]
release_repo = sys.argv[3]
text = path.read_text()
replacements = {
    'G5_INSTALL_RELEASE="${G5_INSTALL_RELEASE:-latest}"':
        f'G5_INSTALL_RELEASE="${{G5_INSTALL_RELEASE:-{release_tag}}}"',
    'G5_GITHUB_RELEASE_REPO="${G5_GITHUB_RELEASE_REPO:-G5Labs/Releases}"':
        f'G5_GITHUB_RELEASE_REPO="${{G5_GITHUB_RELEASE_REPO:-{release_repo}}}"',
}
for needle, replacement in replacements.items():
    if needle not in text:
        raise SystemExit(f"could not find release default marker in {path}: {needle}")
    text = text.replace(needle, replacement, 1)
path.write_text(text)
PY

if find "$STAGE" -type f \( -name 'g5d' -o -name 'g5d-*' \) | grep -q .; then
  find "$STAGE" -type f \( -name 'g5d' -o -name 'g5d-*' \) >&2
  fail "public installer release assets must not include gated g5d binaries"
fi

install -m 0755 "$STAGE/install.sh" "$OUT_DIR/install.sh"
(
  cd "$STAGE"
  COPYFILE_DISABLE=1 tar \
    --exclude='._*' \
    --exclude='.DS_Store' \
    --exclude='__MACOSX' \
    -czf "$OUT_DIR/g5-self-hosted-bundle.tar.gz" .
)

if tar -tzf "$OUT_DIR/g5-self-hosted-bundle.tar.gz" | grep -E '(^|/)(\._|\.DS_Store$|__MACOSX(/|$))' >/dev/null; then
  tar -tzf "$OUT_DIR/g5-self-hosted-bundle.tar.gz" | grep -E '(^|/)(\._|\.DS_Store$|__MACOSX(/|$))' >&2
  fail "public installer bundle contains host metadata files"
fi
(
  cd "$OUT_DIR"
  python3 - <<'PY' > SHA256SUMS
import hashlib
from pathlib import Path

for name in ["install.sh", "g5-self-hosted-bundle.tar.gz"]:
    digest = hashlib.sha256(Path(name).read_bytes()).hexdigest()
    print(f"{digest}  {name}")
PY
)

echo "self-hosted public release assets written to $OUT_DIR"
