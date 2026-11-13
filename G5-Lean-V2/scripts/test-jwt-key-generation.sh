#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/jwt-keys.sh
source "$SCRIPT_DIR/lib/jwt-keys.sh"

private_key=$(generate_es256_private_key)
public_key=$(printf '%s\n' "$private_key" | derive_es256_public_key)

[[ "$private_key" == '-----BEGIN PRIVATE KEY-----'* ]]
[[ "$public_key" == '-----BEGIN PUBLIC KEY-----'* ]]
is_compatible_es256_private_key "$private_key"
es256_keypair_matches "$private_key" "$public_key"

legacy_private_key=$(
  openssl ecparam -genkey -name prime256v1 -noout 2>/dev/null \
    | openssl ec 2>/dev/null
)
if is_compatible_es256_private_key "$legacy_private_key"; then
  echo "legacy SEC1 EC private key was accepted" >&2
  exit 1
fi

mismatched_private_key=$(generate_es256_private_key)
if es256_keypair_matches "$mismatched_private_key" "$public_key"; then
  echo "mismatched ES256 key pair was accepted" >&2
  exit 1
fi

echo "JWT key generation tests passed"
