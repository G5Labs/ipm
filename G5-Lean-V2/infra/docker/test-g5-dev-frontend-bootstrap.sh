#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
helper="$script_dir/g5-dev-frontend-bootstrap"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$tmp_dir/bin" "$tmp_dir/client"
cat > "$tmp_dir/client/package.json" <<'JSON'
{"name":"bootstrap-contract","version":"1.0.0","scripts":{"lint":"eslint .","build":"tsc --noEmit","test":"vitest run"}}
JSON
cat > "$tmp_dir/client/package-lock.json" <<'JSON'
{"name":"bootstrap-contract","version":"1.0.0","lockfileVersion":3,"packages":{"":{"name":"bootstrap-contract","version":"1.0.0"}}}
JSON
cat > "$tmp_dir/bin/npm" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == "ci --no-audit --no-fund" ]]
mkdir -p node_modules/.bin
printf '#!/usr/bin/env sh\nexit 0\n' > node_modules/.bin/eslint
cp node_modules/.bin/eslint node_modules/.bin/tsc
cp node_modules/.bin/eslint node_modules/.bin/vitest
chmod +x node_modules/.bin/*
printf 'install\n' >> "${G5_BOOTSTRAP_TEST_LOG:?}"
SH
chmod +x "$tmp_dir/bin/npm"

export PATH="$tmp_dir/bin:$PATH"
export G5_BOOTSTRAP_TEST_LOG="$tmp_dir/npm.log"

"$helper" "$tmp_dir/client"
"$helper" "$tmp_dir/client"
[[ $(wc -l < "$tmp_dir/npm.log") -eq 1 ]]
[[ -x "$tmp_dir/client/node_modules/.bin/eslint" ]]
[[ -x "$tmp_dir/client/node_modules/.bin/tsc" ]]
[[ -x "$tmp_dir/client/node_modules/.bin/vitest" ]]

printf ' ' >> "$tmp_dir/client/package-lock.json"
"$helper" "$tmp_dir/client"
[[ $(wc -l < "$tmp_dir/npm.log") -eq 2 ]]

# Argument-less invocation from inside the client checkout (the `frontend`
# preview profile's working_dir) resolves the cwd instead of failing; the
# unchanged manifests keep it idempotent.
(cd "$tmp_dir/client" && "$helper")
[[ $(wc -l < "$tmp_dir/npm.log") -eq 2 ]]

# Argument-less invocation from a directory that is not a client checkout and
# holds none beneath it still fails loudly instead of guessing.
mkdir -p "$tmp_dir/empty"
if (cd "$tmp_dir/empty" && "$helper"); then
  echo "expected missing client checkout to fail" >&2
  exit 1
fi

rm "$tmp_dir/client/package-lock.json"
if "$helper" "$tmp_dir/client"; then
  echo "expected missing lockfile to fail" >&2
  exit 1
fi

echo "g5 frontend bootstrap contract passed"
