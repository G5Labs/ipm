#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ASSERT_SCRIPT="${ROOT_DIR}/scripts/assert-g5d-self-contained.sh"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

fake_bin="${tmp_dir}/g5d"
printf '#!/usr/bin/env bash\nexit 0\n' > "${fake_bin}"
chmod +x "${fake_bin}"

fake_ldd_dir="${tmp_dir}/bin"
mkdir -p "${fake_ldd_dir}"

run_with_fake_ldd() {
  local ldd_output="$1"
  cat > "${fake_ldd_dir}/ldd" <<EOF
#!/usr/bin/env bash
cat <<'LDD'
${ldd_output}
LDD
EOF
  chmod +x "${fake_ldd_dir}/ldd"
  PATH="${fake_ldd_dir}:${PATH}" "${ASSERT_SCRIPT}" "${fake_bin}" "${tmp_dir}/g5d.ldd" >/dev/null
}

expect_pass() {
  local name="$1"
  local ldd_output="$2"

  if ! run_with_fake_ldd "${ldd_output}"; then
    echo "FAIL: expected self-contained check to pass for ${name}" >&2
    exit 1
  fi
}

expect_fail() {
  local name="$1"
  local ldd_output="$2"

  if run_with_fake_ldd "${ldd_output}" 2>/dev/null; then
    echo "FAIL: expected self-contained check to fail for ${name}" >&2
    exit 1
  fi
}

expect_pass "ordinary system libraries" $'linux-vdso.so.1 (0x00007ffc)\nlibc.so.6 => /lib/x86_64-linux-gnu/libc.so.6'
expect_fail "dynamic libkrunfw" $'libkrunfw.so.5 => /usr/local/lib/libkrunfw.so.5'
expect_fail "dynamic libkrun" $'libkrun.so.1 => /usr/local/lib/libkrun.so.1'
expect_fail "missing dependency" $'libkrunfw.so.5 => not found'

if [[ $# -gt 0 ]]; then
  for binary in "$@"; do
    "${ASSERT_SCRIPT}" "${binary}"
  done
fi

echo "g5d self-contained dependency checks passed"
