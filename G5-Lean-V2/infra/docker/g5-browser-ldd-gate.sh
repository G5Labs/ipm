#!/usr/bin/env bash
# Build-time gate over baked Playwright Chromium (docker_images [I32]): every
# chrome and headless binary under the browsers directory must resolve all of
# its shared libraries against the image's apt set, and both browser types
# must be present. Shared by Dockerfile.g5d-g5dev and
# Dockerfile.g5d-pairsoft-preview; scripts/test_g5dev_browser_gate.py runs
# this file against ELF fixtures.
#
# The headless binary's name depends on the download source: the
# Playwright-built zips ship chrome-linux/headless_shell, while the
# Chrome-for-Testing fallback ships chrome-headless-shell-linux64/
# chrome-headless-shell (deploy build 117b2513 failed on this skew). Both
# browser types are required, rather than a binary count tied to how many
# revisions are installed: one pinned revision supplies one chrome and one
# headless binary.
#
# Usage: g5-browser-ldd-gate.sh [browsers-dir]   (default /opt/ms-playwright)
set -euo pipefail
browsers="${1:-/opt/ms-playwright}"
bins="$(mktemp)"
ldd_out="$(mktemp)"
trap 'rm -f "$bins" "$ldd_out"' EXIT
find "$browsers" -type f \( \( -name chrome -path '*chrome-linux*' \) -o -name headless_shell -o -name chrome-headless-shell \) > "$bins"
grep -Eq '/chrome$' "$bins" || { echo "FATAL: baked Playwright Chromium browser is missing under ${browsers}" >&2; exit 1; }
grep -Eq '/(headless_shell|chrome-headless-shell)$' "$bins" || { echo "FATAL: baked Playwright Chromium headless shell is missing under ${browsers}" >&2; exit 1; }
while read -r bin; do
  if ! ldd "${bin}" > "$ldd_out"; then
    cat "$ldd_out" >&2
    echo "FATAL: ldd failed on ${bin} — not a dynamic executable, corrupted, or unreadable; the gate cannot verify it (I32)" >&2
    exit 1
  fi
  if grep 'not found' "$ldd_out"; then
    echo "FATAL: ${bin} has unresolved libraries; extend the Chromium apt set (I32)" >&2
    exit 1
  fi
done < "$bins"
