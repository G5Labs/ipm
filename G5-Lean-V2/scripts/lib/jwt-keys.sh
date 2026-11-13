#!/usr/bin/env bash

# Generate and validate the ES256 key material consumed by jsonwebtoken.
# jsonwebtoken::EncodingKey::from_ec_pem requires a PKCS#8 private key.

generate_es256_private_key() {
  openssl genpkey \
    -algorithm EC \
    -pkeyopt ec_paramgen_curve:P-256 2>/dev/null
}

derive_es256_public_key() {
  openssl pkey -pubout 2>/dev/null
}

is_compatible_es256_private_key() {
  local private_key="$1"

  [[ "$private_key" == '-----BEGIN PRIVATE KEY-----'* ]] || return 1
  printf '%s\n' "$private_key" | openssl pkey -check -noout >/dev/null 2>&1 || return 1
  printf '%s\n' "$private_key" | openssl pkey -text -noout 2>/dev/null \
    | grep -Eq 'ASN1 OID: (prime256v1|secp256r1)|NIST CURVE: P-256'
}

es256_keypair_matches() {
  local private_key="$1"
  local public_key="$2"
  local derived_public_key

  derived_public_key=$(printf '%s\n' "$private_key" | derive_es256_public_key) || return 1
  [[ "$derived_public_key" == "$public_key" ]]
}
