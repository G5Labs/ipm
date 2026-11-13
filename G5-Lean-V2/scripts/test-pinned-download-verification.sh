#!/usr/bin/env bash
# Executable contract for operations g5.quality.delivery.pinned-third-party-tools
# (G5Labs/G5-Lean#527): the git-lfs, Rust CLI tool, uv and dl-cache RUN blocks
# of infra/docker/Dockerfile.g5d-base and install_pinned_just of
# scripts/self-hosted-azure/bootstrap-ubuntu-host.sh are executed as shipped
# (install prefixes rewritten to a scratch dir; the Dockerfile text under
# /bin/sh, BuildKit's default RUN shell) against a PATH-shadowed curl double:
#   - bytes matching the repository pin install the expected binaries, for the
#     amd64 and arm64 selections BuildKit passes and the `uname -m` fallback;
#   - substituted bytes (the finding's attacker-controlled upstream response)
#     fail at sha256sum before anything is extracted or executed and stop the
#     block at the first mismatch, leaving no binary behind;
#   - an architecture without a recorded pin fails before any download;
#   - the dl-cache block refuses a tarball that does not match the pin and a
#     cached upstream checksum file that lists another digest or none;
#   - the Azure bootstrap selects the per-arch pin, refuses a binary whose
#     version differs from the pin, and never runs its root main flow when
#     sourced.
# Run by the ci-rust `image-build-script-contracts` step; also runnable
# locally (bash + /bin/sh + coreutils + tar; no Docker, no network).
set -euo pipefail
cd "$(dirname "$0")/.."

DF=infra/docker/Dockerfile.g5d-base
BOOTSTRAP=scripts/self-hosted-azure/bootstrap-ubuntu-host.sh
HOST_ARCH="$(uname -m)"

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin" "$T/tmp" "$T/cargo-bin" "$T/usr-local-bin" "$T/just-bin" "$T/fixtures"

fail() {
  echo "FAIL: $*" >&2
  if [ -n "${OUT:-}" ] && [ -f "$OUT" ]; then sed 's/^/  | /' "$OUT" >&2; fi
  exit 1
}

# Extract the RUN whose text contains $2 from Dockerfile $1: the RUN line plus
# its backslash continuations, with the RUN prefix, the continuation
# backslashes and comment lines (which Docker drops) removed.
extract_run() {
  awk -v needle="$2" '
    /^RUN / { collecting=1; block=""; hit=0 }
    collecting {
      if ($0 ~ /^[[:space:]]*#/) next
      line=$0
      if (index(line, needle)) hit=1
      ends=(line ~ /\\[ \t]*$/)
      sub(/\\[ \t]*$/, "", line)
      if (block == "") { sub(/^RUN /, "", line); block=line } else { block=block " " line }
      if (!ends) { collecting=0; if (hit) { print block; exit } }
    }' "$1"
}

# Every `ARG NAME=value` of the Dockerfile as NAME=value lines (the build's
# default values; the per-arch pins among them are the real repository pins).
dockerfile_args() { grep -E '^ARG [A-Za-z_][A-Za-z0-9_]*=' "$1" | sed -E 's/^ARG //'; }
arg_value() { dockerfile_args "$DF" | sed -n "s/^$1=//p" | head -n 1; }

rewrite_prefixes() {
  # /tmp/ FIRST: the scratch dir itself lives under /tmp/, so a later /tmp/
  # pass would re-hit the substituted prefixes.
  local text="$1"
  text="${text//\/tmp\//$T/tmp/}"
  text="${text//\/tmp /$T/tmp }"
  text="${text//\/usr\/local\/cargo\/bin/$T/cargo-bin}"
  text="${text//\/usr\/local\/bin/$T/usr-local-bin}"
  text="${text//\/opt\/g5\/dl-cache/$T/dl-cache}"
  printf '%s' "$text"
}

UV_VERSION="$(arg_value UV_VERSION)"
SCCACHE_VERSION="$(arg_value SCCACHE_VERSION)"
CARGO_DENY_VERSION="$(arg_value CARGO_DENY_VERSION)"
GIT_LFS_VERSION="$(arg_value GIT_LFS_VERSION)"
JUST_VERSION_PINNED="$(sed -n 's/^JUST_VERSION=//p' "$BOOTSTRAP" | head -n 1)"
GH_CLI_VERSION="$(arg_value GH_CLI_VERSION)"
CARGO_NEXTEST_VERSION="$(arg_value CARGO_NEXTEST_VERSION)"
JUST_VERSION_CACHED="$(arg_value JUST_VERSION)"
[ -n "$GH_CLI_VERSION" ] && [ -n "$CARGO_NEXTEST_VERSION" ] && [ -n "$JUST_VERSION_CACHED" ] \
  || fail "could not read the dl-cache versions from $DF"
[ -n "$UV_VERSION" ] && [ -n "$SCCACHE_VERSION" ] && [ -n "$CARGO_DENY_VERSION" ] && [ -n "$GIT_LFS_VERSION" ] && [ -n "$JUST_VERSION_PINNED" ] \
  || fail "could not read the pinned versions from $DF / $BOOTSTRAP"

# ---------------------------------------------------------------------------
# Fixtures: per-arch release tarballs with the upstream layouts, whose members
# are scripts that print the pinned version, plus one "substituted" tarball
# carrying every member name with a payload that would leave a marker if run.
tool_script() { printf '#!/bin/sh\necho "%s"\n' "$1"; }
evil_script() { printf '#!/bin/sh\n: > "%s/pwned"\necho "evil"\n' "$T"; }
make_tarball() { # $1 out.tar.gz, $2 src dir, rest members
  local out="$1" src="$2"; shift 2
  tar -C "$src" -czf "$out" "$@"
}
sha_of() { sha256sum "$1" | cut -d' ' -f1; }
for arch in x86_64 aarch64; do
  fx="$T/fixtures/$arch"; src="$T/fixtures/src-$arch"
  mkdir -p "$fx" "$src/sccache-${SCCACHE_VERSION}-${arch}-unknown-linux-musl" \
    "$src/cargo-deny-${CARGO_DENY_VERSION}-${arch}-unknown-linux-musl" \
    "$src/uv-${arch}-unknown-linux-gnu" "$src/git-lfs-${GIT_LFS_VERSION}" "$src/just-root"
  tool_script "cargo-nextest fixture $arch" > "$src/cargo-nextest"
  tool_script "sccache fixture $arch" > "$src/sccache-${SCCACHE_VERSION}-${arch}-unknown-linux-musl/sccache"
  printf 'license\n' > "$src/sccache-${SCCACHE_VERSION}-${arch}-unknown-linux-musl/LICENSE"
  tool_script "cargo-deny fixture $arch" > "$src/cargo-deny-${CARGO_DENY_VERSION}-${arch}-unknown-linux-musl/cargo-deny"
  printf 'readme\n' > "$src/cargo-deny-${CARGO_DENY_VERSION}-${arch}-unknown-linux-musl/README.md"
  tool_script "uv ${UV_VERSION} (fixture $arch)" > "$src/uv-${arch}-unknown-linux-gnu/uv"
  tool_script "uvx fixture $arch" > "$src/uv-${arch}-unknown-linux-gnu/uvx"
  tool_script "git-lfs/${GIT_LFS_VERSION} fixture $arch" > "$src/git-lfs-${GIT_LFS_VERSION}/git-lfs"
  printf 'readme\n' > "$src/git-lfs-${GIT_LFS_VERSION}/README.md"
  tool_script "just ${JUST_VERSION_PINNED}" > "$src/just-root/just"
  printf 'readme\n' > "$src/just-root/README"
  chmod +x "$src/cargo-nextest" "$src"/sccache-*/sccache "$src"/cargo-deny-*/cargo-deny "$src"/uv-*/uv "$src"/uv-*/uvx "$src"/git-lfs-*/git-lfs "$src/just-root/just"
  gh_arch=amd64; [ "$arch" = aarch64 ] && gh_arch=arm64
  mkdir -p "$src/gh_${GH_CLI_VERSION}_linux_${gh_arch}/bin"
  tool_script "gh fixture $arch" > "$src/gh_${GH_CLI_VERSION}_linux_${gh_arch}/bin/gh"
  chmod +x "$src/gh_${GH_CLI_VERSION}_linux_${gh_arch}/bin/gh"
  make_tarball "$fx/gh.tar.gz" "$src" "gh_${GH_CLI_VERSION}_linux_${gh_arch}"
  make_tarball "$fx/nextest.tar.gz" "$src" cargo-nextest
  make_tarball "$fx/sccache.tar.gz" "$src" "sccache-${SCCACHE_VERSION}-${arch}-unknown-linux-musl"
  make_tarball "$fx/cargo-deny.tar.gz" "$src" "cargo-deny-${CARGO_DENY_VERSION}-${arch}-unknown-linux-musl"
  make_tarball "$fx/uv.tar.gz" "$src" "uv-${arch}-unknown-linux-gnu"
  make_tarball "$fx/git-lfs.tar.gz" "$src" "git-lfs-${GIT_LFS_VERSION}"
  make_tarball "$fx/just.tar.gz" "$src/just-root" just README
  # A release whose binary reports another version than the pin.
  mkdir -p "$src/just-badversion"; tool_script "just 0.0.1" > "$src/just-badversion/just"; chmod +x "$src/just-badversion/just"
  make_tarball "$fx/just-badversion.tar.gz" "$src/just-badversion" just
done
# Upstream checksum files as the dl-cache block downloads them: good ones list
# the fixture digests for both architectures, bad ones another digest, empty
# ones no entry for the requested file.
zeros="$(printf '0%.0s' $(seq 1 64))"
for kind in good bad empty; do
  sums="$T/fixtures/sums-$kind"; mkdir -p "$sums"
  : > "$sums/gh_checksums.txt"; : > "$sums/just-SHA256SUMS"
  for arch in x86_64 aarch64; do
    fx="$T/fixtures/$arch"; gh_arch=amd64; [ "$arch" = aarch64 ] && gh_arch=arm64
    case "$kind" in
      good)  gh_sum="$(sha_of "$fx/gh.tar.gz")"; just_sum="$(sha_of "$fx/just.tar.gz")"; nextest_sum="$(sha_of "$fx/nextest.tar.gz")"; suffix="" ;;
      bad)   gh_sum="$zeros"; just_sum="$zeros"; nextest_sum="$zeros"; suffix="" ;;
      empty) gh_sum="$(sha_of "$fx/gh.tar.gz")"; just_sum="$(sha_of "$fx/just.tar.gz")"; nextest_sum=""; suffix=".other" ;;
    esac
    printf '%s  %s\n' "$gh_sum" "gh_${GH_CLI_VERSION}_linux_${gh_arch}.tar.gz${suffix}" >> "$sums/gh_checksums.txt"
    printf '%s  %s\n' "$just_sum" "just-${JUST_VERSION_CACHED}-${arch}-unknown-linux-musl.tar.gz${suffix}" >> "$sums/just-SHA256SUMS"
    printf '%s\n' "$nextest_sum" > "$sums/nextest-${arch}.sha256"
  done
done
evil="$T/fixtures/src-evil"
mkdir -p "$evil/sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl" "$evil/cargo-deny-${CARGO_DENY_VERSION}-x86_64-unknown-linux-musl" \
  "$evil/uv-x86_64-unknown-linux-gnu" "$evil/git-lfs-${GIT_LFS_VERSION}"
for member in cargo-nextest "sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl/sccache" \
  "cargo-deny-${CARGO_DENY_VERSION}-x86_64-unknown-linux-musl/cargo-deny" uv-x86_64-unknown-linux-gnu/uv \
  uv-x86_64-unknown-linux-gnu/uvx "git-lfs-${GIT_LFS_VERSION}/git-lfs" just; do
  evil_script > "$evil/$member"; chmod +x "$evil/$member"
done
make_tarball "$T/fixtures/substituted.tar.gz" "$evil" .

# ---------------------------------------------------------------------------
# Doubles. curl: records every URL, serves the fixture the URL names into the
# output file, or the substituted tarball when CURL_MODE=substituted, or the
# wrong-version just when CURL_MODE=badversion. uname -m: FAKE_UNAME.
# git / dpkg-query: what the git-lfs block calls around its download.
# apt-get: proves the bootstrap's root main flow did not run when sourced.
cat >"$T/bin/curl" <<'FAKE'
#!/usr/bin/env bash
n=0; [ -f "$CURL_COUNT" ] && n="$(<"$CURL_COUNT")"; n=$((n+1)); printf '%s' "$n" > "$CURL_COUNT"
out=""; url=""
while [ $# -gt 0 ]; do
  case "$1" in
    --output) out="$2"; shift ;;
    --proto|--retry|--retry-delay|--connect-timeout|--max-time|--speed-limit|--speed-time|--write-out|-w) shift ;;
    --*) ;;
    -*o) out="$2"; shift ;;
    -*) ;;
    *) url="$1" ;;
  esac
  shift
done
printf '%s\n' "$url" >> "$CURL_LOG"
[ -n "$out" ] || { echo "fake curl: no output file for $url (a download must land in a file)" >&2; exit 64; }
case "$url" in *aarch64*|*arm64*) arch=aarch64 ;; *) arch=x86_64 ;; esac
# CURL_MODE: good | substituted | badversion | badchecksumfile[:TOOL] |
# emptychecksumfile[:TOOL]; a TOOL (gh, just, nextest) limits the broken
# checksum file to that tool so each published check is reached on its own.
mode="${CURL_MODE:-good}"; kind="${mode%%:*}"; only="${mode#*:}"; [ "$only" = "$mode" ] && only=all
sums_for() {
  case "$kind" in
    badchecksumfile)   if [ "$only" = all ] || [ "$only" = "$1" ]; then echo "$FIXTURES/sums-bad"; else echo "$FIXTURES/sums-good"; fi ;;
    emptychecksumfile) if [ "$only" = all ] || [ "$only" = "$1" ]; then echo "$FIXTURES/sums-empty"; else echo "$FIXTURES/sums-good"; fi ;;
    *) echo "$FIXTURES/sums-good" ;;
  esac
}
case "$url" in
  *cli/cli/*checksums.txt)        src="$(sums_for gh)/gh_checksums.txt" ;;
  *casey/just/*SHA256SUMS)        src="$(sums_for just)/just-SHA256SUMS" ;;
  *nextest-rs/nextest/*.sha256)   src="$(sums_for nextest)/nextest-${arch}.sha256" ;;
  *cli/cli/*.tar.gz)              src="$FIXTURES/$arch/gh.tar.gz" ;;
  *nextest-rs/nextest/*.tar.gz)   src="$FIXTURES/$arch/nextest.tar.gz" ;;
  *mozilla/sccache/*.tar.gz)      src="$FIXTURES/$arch/sccache.tar.gz" ;;
  *EmbarkStudios/cargo-deny/*.tar.gz) src="$FIXTURES/$arch/cargo-deny.tar.gz" ;;
  *astral-sh/uv/*.tar.gz)         src="$FIXTURES/$arch/uv.tar.gz" ;;
  *git-lfs/git-lfs/*.tar.gz)      src="$FIXTURES/$arch/git-lfs.tar.gz" ;;
  *casey/just/*.tar.gz)           src="$FIXTURES/$arch/just.tar.gz" ;;
  *) echo "fake curl: unexpected URL $url" >&2; exit 65 ;;
esac
case "$kind" in
  substituted) case "$url" in *.tar.gz) src="$FIXTURES/substituted.tar.gz" ;; esac ;;
  badversion) src="$FIXTURES/$arch/just-badversion.tar.gz" ;;
esac
cp "$src" "$out"
FAKE
cat >"$T/bin/uname" <<'FAKE'
#!/usr/bin/env bash
case "$*" in *-m*) echo "${FAKE_UNAME:?}" ;; *) exec /usr/bin/uname "$@" ;; esac
FAKE
cat >"$T/bin/git" <<'FAKE'
#!/usr/bin/env bash
case "$*" in "--version") echo "git version 2.99.0 (fixture)" ;; "lfs version") exec git-lfs version ;; *) echo "fake git: unexpected args: $*" >&2; exit 64 ;; esac
FAKE
cat >"$T/bin/dpkg-query" <<'FAKE'
#!/usr/bin/env bash
exit 1
FAKE
cat >"$T/bin/apt-get" <<'FAKE'
#!/usr/bin/env bash
: > "${APT_MARKER:?}"; echo "fake apt-get must never run from a sourced bootstrap" >&2; exit 1
FAKE
chmod +x "$T/bin/"*

reset_scratch() {
  rm -rf "$T/tmp" "$T/cargo-bin" "$T/usr-local-bin" "$T/just-bin" "$T/dl-cache" "$T/pwned" "$T/curl-count" "$T/curl.log" "$T/apt-marker"
  mkdir -p "$T/tmp" "$T/cargo-bin" "$T/usr-local-bin" "$T/just-bin"
}
calls() { if [ -f "$T/curl-count" ]; then cat "$T/curl-count"; else echo 0; fi; }
urls() { if [ -f "$T/curl.log" ]; then cat "$T/curl.log"; fi; }

# run_block NAME RUN_TEXT MODE TARGETARCH FAKE_UNAME [PIN=value ...]
# Executes the rewritten RUN text under /bin/sh with the Dockerfile's ARG
# defaults in the environment, the given overrides on top.
run_block() {
  local name="$1" text="$2" mode="$3" targetarch="$4" fake_uname="$5"; shift 5
  reset_scratch
  OUT="$T/$name-$mode-${targetarch:-fallback}.out"
  set +e
  env -i PATH="$T/bin:$T/usr-local-bin:$T/cargo-bin:/usr/bin:/bin" HOME="$T" \
      CURL_MODE="$mode" CURL_COUNT="$T/curl-count" CURL_LOG="$T/curl.log" FIXTURES="$T/fixtures" \
      FAKE_UNAME="$fake_uname" TARGETARCH="$targetarch" \
      $(dockerfile_args "$DF" | tr '\n' ' ') "$@" \
      /bin/sh -c "$text" >"$OUT" 2>&1
  STATUS=$?
  set -e
}

# ---------------------------------------------------------------------------
# Dockerfile.g5d-base: Rust CLI tools (nextest, sccache, cargo-deny)
TOOLS_RUN="$(rewrite_prefixes "$(extract_run "$DF" 'cargo-nextest.tar.gz')")"
[ -n "$TOOLS_RUN" ] || fail "could not extract the Rust CLI tools RUN from $DF"
/bin/sh -n <(printf '%s\n' "$TOOLS_RUN") || fail "Rust CLI tools RUN text does not parse under /bin/sh"
UV_RUN="$(rewrite_prefixes "$(extract_run "$DF" 'uv.tar.gz')")"
[ -n "$UV_RUN" ] || fail "could not extract the uv RUN from $DF"
/bin/sh -n <(printf '%s\n' "$UV_RUN") || fail "uv RUN text does not parse under /bin/sh"
LFS_RUN="$(rewrite_prefixes "$(extract_run "$DF" 'git-lfs-linux-')")"
[ -n "$LFS_RUN" ] || fail "could not extract the git-lfs RUN from $DF"
/bin/sh -n <(printf '%s\n' "$LFS_RUN") || fail "git-lfs RUN text does not parse under /bin/sh"

for selection in "amd64 x86_64" "arm64 aarch64" " $HOST_ARCH"; do
  targetarch="${selection% *}"; arch="${selection#* }"
  fx="$T/fixtures/$arch"
  run_block tools "$TOOLS_RUN" good "$targetarch" "$arch" \
    "CARGO_NEXTEST_SHA256_X86_64=$(sha_of "$fx/nextest.tar.gz")" "CARGO_NEXTEST_SHA256_AARCH64=$(sha_of "$fx/nextest.tar.gz")" \
    "SCCACHE_SHA256_X86_64=$(sha_of "$fx/sccache.tar.gz")" "SCCACHE_SHA256_AARCH64=$(sha_of "$fx/sccache.tar.gz")" \
    "CARGO_DENY_SHA256_X86_64=$(sha_of "$fx/cargo-deny.tar.gz")" "CARGO_DENY_SHA256_AARCH64=$(sha_of "$fx/cargo-deny.tar.gz")"
  [ "$STATUS" -eq 0 ] || fail "tools ($selection): matching pins must install (exit $STATUS)"
  for tool in cargo-nextest sccache cargo-deny; do
    [ -x "$T/cargo-bin/$tool" ] || fail "tools ($selection): $tool not installed"
    "$T/cargo-bin/$tool" | grep -q "$tool fixture $arch" || fail "tools ($selection): $tool is not the $arch release"
  done
  [ "$(calls)" -eq 3 ] || fail "tools ($selection): expected 3 downloads, saw $(calls)"
  urls | grep -q -- "-${arch}-unknown-linux-gnu.tar.gz" || fail "tools ($selection): nextest URL does not name the $arch target"
  [ ! -e "$T/tmp/g5-rust-tools" ] || fail "tools ($selection): scratch download directory left behind"
  echo "PASS tools (${targetarch:-uname fallback} -> $arch): pinned tarballs installed"
done

run_block tools "$TOOLS_RUN" substituted amd64 x86_64
[ "$STATUS" -ne 0 ] || fail "tools: substituted bytes must fail the build"
grep -q "FAILED" "$OUT" || fail "tools: substituted bytes must fail at sha256sum"
[ "$(calls)" -eq 1 ] || fail "tools: the block must stop at the first mismatch, saw $(calls) downloads"
[ -z "$(ls -A "$T/cargo-bin")" ] || fail "tools: a binary was installed from substituted bytes"
[ ! -e "$T/tmp/g5-rust-tools/cargo-nextest" ] || fail "tools: substituted tarball was extracted before verification"
[ ! -e "$T/pwned" ] || fail "tools: substituted payload executed"
echo "PASS tools: substituted bytes rejected before extraction, nothing installed"

run_block tools "$TOOLS_RUN" good riscv64 riscv64
[ "$STATUS" -ne 0 ] || fail "tools: an architecture without a pin must fail"
grep -q "FATAL: no pinned" "$OUT" || fail "tools: unpinned architecture must name the missing pin"
[ "$(calls)" -eq 0 ] || fail "tools: unpinned architecture must not download anything"
echo "PASS tools: unpinned architecture fails before any download"

# ---------------------------------------------------------------------------
# Dockerfile.g5d-base: uv
for selection in "amd64 x86_64" "arm64 aarch64" " $HOST_ARCH"; do
  targetarch="${selection% *}"; arch="${selection#* }"
  fx="$T/fixtures/$arch"
  run_block uv "$UV_RUN" good "$targetarch" "$arch" \
    "UV_SHA256_X86_64=$(sha_of "$fx/uv.tar.gz")" "UV_SHA256_AARCH64=$(sha_of "$fx/uv.tar.gz")"
  [ "$STATUS" -eq 0 ] || fail "uv ($selection): matching pin must install (exit $STATUS)"
  [ -x "$T/usr-local-bin/uv" ] && [ -x "$T/usr-local-bin/uvx" ] || fail "uv ($selection): uv/uvx not installed"
  "$T/usr-local-bin/uv" --version | grep -q "fixture $arch" || fail "uv ($selection): uv is not the $arch release"
  urls | grep -q "uv-${arch}-unknown-linux-gnu.tar.gz" || fail "uv ($selection): URL does not name the $arch target"
  [ ! -e "$T/tmp/uv.tar.gz" ] || fail "uv ($selection): downloaded tarball left behind"
  echo "PASS uv (${targetarch:-uname fallback} -> $arch): pinned tarball installed"
done

run_block uv "$UV_RUN" substituted amd64 x86_64
[ "$STATUS" -ne 0 ] || fail "uv: substituted bytes must fail the build"
grep -q "FAILED" "$OUT" || fail "uv: substituted bytes must fail at sha256sum"
[ -z "$(ls -A "$T/usr-local-bin")" ] || fail "uv: a binary was installed from substituted bytes"
[ ! -e "$T/tmp/uv-x86_64-unknown-linux-gnu" ] || fail "uv: substituted tarball was extracted before verification"
[ ! -e "$T/pwned" ] || fail "uv: substituted payload executed"
echo "PASS uv: substituted bytes rejected before extraction, nothing installed"

run_block uv "$UV_RUN" good riscv64 riscv64
[ "$STATUS" -ne 0 ] && [ "$(calls)" -eq 0 ] || fail "uv: unpinned architecture must fail before any download (exit $STATUS, $(calls) downloads)"
echo "PASS uv: unpinned architecture fails before any download"

# ---------------------------------------------------------------------------
# Dockerfile.g5d-base: git-lfs
for selection in "amd64 x86_64" "arm64 aarch64" " $HOST_ARCH"; do
  targetarch="${selection% *}"; arch="${selection#* }"
  fx="$T/fixtures/$arch"
  run_block lfs "$LFS_RUN" good "$targetarch" "$arch" \
    "GIT_LFS_SHA256_AMD64=$(sha_of "$fx/git-lfs.tar.gz")" "GIT_LFS_SHA256_ARM64=$(sha_of "$fx/git-lfs.tar.gz")"
  [ "$STATUS" -eq 0 ] || fail "git-lfs ($selection): matching pin must install (exit $STATUS)"
  [ -x "$T/usr-local-bin/git-lfs" ] || fail "git-lfs ($selection): git-lfs not installed"
  "$T/usr-local-bin/git-lfs" | grep -q "fixture $arch" || fail "git-lfs ($selection): git-lfs is not the $arch release"
  lfs_arch=amd64; [ "$arch" = aarch64 ] && lfs_arch=arm64
  urls | grep -q "git-lfs-linux-${lfs_arch}-v" || fail "git-lfs ($selection): URL does not name the $lfs_arch release"
  [ "$(calls)" -eq 1 ] || fail "git-lfs ($selection): expected exactly one download (no checksum file fetch), saw $(calls)"
  echo "PASS git-lfs (${targetarch:-uname fallback} -> $arch): pinned tarball installed"
done

run_block lfs "$LFS_RUN" substituted amd64 x86_64
[ "$STATUS" -ne 0 ] || fail "git-lfs: substituted bytes must fail the build"
grep -q "FAILED" "$OUT" || fail "git-lfs: substituted bytes must fail at sha256sum"
[ ! -e "$T/usr-local-bin/git-lfs" ] || fail "git-lfs: a binary was installed from substituted bytes"
[ ! -e "$T/tmp/git-lfs-${GIT_LFS_VERSION}" ] || fail "git-lfs: substituted tarball was extracted before verification"
[ ! -e "$T/pwned" ] || fail "git-lfs: substituted payload executed"
echo "PASS git-lfs: substituted bytes rejected before extraction, nothing installed"

run_block lfs "$LFS_RUN" good riscv64 riscv64
[ "$STATUS" -ne 0 ] && [ "$(calls)" -eq 0 ] || fail "git-lfs: unpinned architecture must fail before any download (exit $STATUS, $(calls) downloads)"
grep -q "FATAL: no pinned git-lfs" "$OUT" || fail "git-lfs: unpinned architecture must name the missing pin"
echo "PASS git-lfs: unpinned architecture fails before any download"

# ---------------------------------------------------------------------------
# Dockerfile.g5d-base: dl-cache (gh, just, cargo-nextest tarballs + the
# upstream checksum files guests verify against)
DLCACHE_RUN="$(rewrite_prefixes "$(extract_run "$DF" 'JUST_TARBALL=')")"
[ -n "$DLCACHE_RUN" ] || fail "could not extract the dl-cache RUN from $DF"
/bin/sh -n <(printf '%s\n' "$DLCACHE_RUN") || fail "dl-cache RUN text does not parse under /bin/sh"
dlcache_pins() {
  printf '%s\n' \
    "GH_CLI_SHA256_AMD64=$(sha_of "$T/fixtures/x86_64/gh.tar.gz")" "GH_CLI_SHA256_ARM64=$(sha_of "$T/fixtures/aarch64/gh.tar.gz")" \
    "JUST_SHA256_X86_64=$(sha_of "$T/fixtures/x86_64/just.tar.gz")" "JUST_SHA256_AARCH64=$(sha_of "$T/fixtures/aarch64/just.tar.gz")" \
    "CARGO_NEXTEST_SHA256_X86_64=$(sha_of "$T/fixtures/x86_64/nextest.tar.gz")" "CARGO_NEXTEST_SHA256_AARCH64=$(sha_of "$T/fixtures/aarch64/nextest.tar.gz")"
}
mapfile -t DLCACHE_PINS < <(dlcache_pins)

for selection in "amd64 x86_64" "arm64 aarch64" " $HOST_ARCH"; do
  targetarch="${selection% *}"; arch="${selection#* }"
  gh_arch=amd64; [ "$arch" = aarch64 ] && gh_arch=arm64
  run_block dlcache "$DLCACHE_RUN" good "$targetarch" "$arch" "${DLCACHE_PINS[@]}"
  [ "$STATUS" -eq 0 ] || fail "dl-cache ($selection): matching pins must cache (exit $STATUS)"
  for f in "gh_${GH_CLI_VERSION}_linux_${gh_arch}.tar.gz" "gh_${GH_CLI_VERSION}_checksums.txt" \
           "just-${JUST_VERSION_CACHED}-${arch}-unknown-linux-musl.tar.gz" "just-${JUST_VERSION_CACHED}-SHA256SUMS" \
           "cargo-nextest-${CARGO_NEXTEST_VERSION}-${arch}-unknown-linux-gnu.tar.gz" "cargo-nextest-${CARGO_NEXTEST_VERSION}-${arch}-unknown-linux-gnu.sha256"; do
    [ -r "$T/dl-cache/$f" ] || fail "dl-cache ($selection): $f not cached"
  done
  [ "$(calls)" -eq 6 ] || fail "dl-cache ($selection): expected 6 downloads, saw $(calls)"
  echo "PASS dl-cache (${targetarch:-uname fallback} -> $arch): pinned tarballs and agreeing checksum files cached"
done

run_block dlcache "$DLCACHE_RUN" substituted amd64 x86_64
[ "$STATUS" -ne 0 ] || fail "dl-cache: a tarball that does not match the pin must fail the build"
grep -q "sha256 mismatch" "$OUT" || fail "dl-cache: pin mismatch must fail at ck"
[ "$(calls)" -eq 2 ] || fail "dl-cache: the block must stop at the first mismatch (gh tarball + its checksum file), saw $(calls) downloads"
echo "PASS dl-cache: substituted tarball rejected at the repository pin"

# Each cached checksum file must list the repository pin: break one tool's
# file at a time, so every published check is reached and must name its file.
for tool in gh just nextest; do
  case "$tool" in
    gh)      file="gh_${GH_CLI_VERSION}_checksums.txt"; downloads=2 ;;
    just)    file="just-${JUST_VERSION_CACHED}-SHA256SUMS"; downloads=4 ;;
    nextest) file="cargo-nextest-${CARGO_NEXTEST_VERSION}-x86_64-unknown-linux-gnu.sha256"; downloads=6 ;;
  esac
  run_block dlcache "$DLCACHE_RUN" "badchecksumfile:$tool" amd64 x86_64 "${DLCACHE_PINS[@]}"
  [ "$STATUS" -ne 0 ] || fail "dl-cache ($tool): a checksum file listing another digest must fail the build"
  grep -q "FATAL: ${file} lists sha256 '${zeros}' but the repository pins" "$OUT" || fail "dl-cache ($tool): disagreeing checksum file must fail at published, naming ${file}"
  [ "$(calls)" -eq "$downloads" ] || fail "dl-cache ($tool): the block must stop right after ${file} (download $downloads), saw $(calls)"
  run_block dlcache "$DLCACHE_RUN" "emptychecksumfile:$tool" amd64 x86_64 "${DLCACHE_PINS[@]}"
  [ "$STATUS" -ne 0 ] || fail "dl-cache ($tool): a checksum file without an entry for the tarball must fail the build"
  grep -q "FATAL: ${file} lists sha256 '' but the repository pins" "$OUT" || fail "dl-cache ($tool): missing checksum entry must fail at published, naming ${file}"
  [ "$(calls)" -eq "$downloads" ] || fail "dl-cache ($tool): the block must stop right after ${file} (download $downloads), saw $(calls)"
  echo "PASS dl-cache ($tool): checksum file that disagrees with the pin, or lacks the entry, refused at ${file}"
done

run_block dlcache "$DLCACHE_RUN" good riscv64 riscv64 "${DLCACHE_PINS[@]}"
[ "$STATUS" -ne 0 ] && [ "$(calls)" -eq 0 ] || fail "dl-cache: unpinned architecture must fail before any download (exit $STATUS, $(calls) downloads)"
grep -q "FATAL: no pinned dl-cache" "$OUT" || fail "dl-cache: unpinned architecture must name the missing pin"
echo "PASS dl-cache: unpinned architecture fails before any download"

# ---------------------------------------------------------------------------
# Azure canary bootstrap: install_pinned_just
bash -n "$BOOTSTRAP" || fail "bootstrap does not parse"

# run_just MODE FAKE_UNAME [OVERRIDE_SHA]
run_just() {
  local mode="$1" fake_uname="$2" override="${3:-}"
  reset_scratch
  OUT="$T/just-$mode-$fake_uname.out"
  set +e
  (
    export PATH="$T/bin:/usr/bin:/bin" CURL_MODE="$mode" CURL_COUNT="$T/curl-count" CURL_LOG="$T/curl.log" \
           FIXTURES="$T/fixtures" FAKE_UNAME="$fake_uname" APT_MARKER="$T/apt-marker"
    . "$BOOTSTRAP"
    if [ -n "$override" ]; then JUST_SHA256_X86_64="$override"; JUST_SHA256_AARCH64="$override"; fi
    install_pinned_just "$T/just-bin"
  ) >"$OUT" 2>&1
  STATUS=$?
  set -e
  [ ! -e "$T/apt-marker" ] || fail "bootstrap: sourcing the script ran its root main flow"
}

for arch in x86_64 aarch64; do
  run_just good "$arch" "$(sha_of "$T/fixtures/$arch/just.tar.gz")"
  [ "$STATUS" -eq 0 ] || fail "just ($arch): matching pin must install (exit $STATUS)"
  [ -x "$T/just-bin/just" ] || fail "just ($arch): just not installed"
  urls | grep -q "casey/just/releases/download/${JUST_VERSION_PINNED}/just-${JUST_VERSION_PINNED}-${arch}-unknown-linux-musl.tar.gz" \
    || fail "just ($arch): URL does not name the pinned ${arch} release: $(urls)"
  [ "$(calls)" -eq 1 ] || fail "just ($arch): expected one download, saw $(calls)"
  grep -q "Installed just ${JUST_VERSION_PINNED}" "$OUT" || fail "just ($arch): install was not reported"
  echo "PASS just ($arch): pinned release installed"
done

run_just substituted x86_64
[ "$STATUS" -ne 0 ] || fail "just: substituted bytes must fail the bootstrap"
grep -q "sha256 mismatch" "$OUT" || fail "just: substituted bytes must be reported as a sha256 mismatch"
[ ! -e "$T/just-bin/just" ] || fail "just: a binary was installed from substituted bytes"
[ ! -e "$T/pwned" ] || fail "just: substituted payload executed"
echo "PASS just: substituted bytes rejected, nothing installed"

run_just badversion x86_64 "$(sha_of "$T/fixtures/x86_64/just-badversion.tar.gz")"
[ "$STATUS" -ne 0 ] || fail "just: a binary reporting another version than the pin must fail"
grep -q "version mismatch" "$OUT" || fail "just: version mismatch must be reported"
[ ! -e "$T/just-bin/just" ] || fail "just: the mismatching binary must be removed"
echo "PASS just: wrong-version binary refused and removed"

run_just good riscv64
[ "$STATUS" -ne 0 ] || fail "just: an architecture without a pin must fail"
[ "$(calls)" -eq 0 ] || fail "just: unpinned architecture must not download anything"
grep -q "FATAL: no pinned just" "$OUT" || fail "just: unpinned architecture must name the missing pin"
echo "PASS just: unpinned architecture fails before any download"

echo "pinned download verification tests passed"
