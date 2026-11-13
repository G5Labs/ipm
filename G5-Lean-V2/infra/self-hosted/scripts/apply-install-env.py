#!/usr/bin/env python3
"""Apply customer install-time settings to a self-hosted .env file."""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path


ENV_OVERRIDE_KEYS = [
    "G5_COMPOSE_PROJECT",
    "G5_EXTRA_COMPOSE_FILE",
    "G5_HTTP_PORT",
    "G5_HTTPS_PORT",
    "G5_EDGE_SUBNET",
    "G5_EDGE_IP",
    "G5_API_EDGE_IP",
    "G5_TLS_CERT_FILE",
    "G5_TLS_KEY_FILE",
    "G5_DB_NAME",
    "G5_DB_USER",
    "G5_DB_PASSWORD",
    "G5_JWT_PRIVATE_KEY",
    "G5_JWT_PUBLIC_KEY",
    "G5_SCM_ENCRYPTION_KEY",
    "G5_COOKIE_SECURE",
    "G5_CORS_ALLOWED_ORIGINS",
    "G5_SELF_HOSTED_DEFAULT_ORG_NAME",
    "G5_SELF_HOSTED_DEFAULT_ORG_SLUG",
    "G5_SELF_HOSTED_DEFAULT_ADMIN_EMAIL",
    "G5_SELF_HOSTED_DEFAULT_ADMIN_NAME",
    "G5D_ORG_ID",
    "G5_GITHUB_CLIENT_ID",
    "G5_GITHUB_CLIENT_SECRET",
    "G5_GITHUB_AUTHORIZE_URL",
    "G5_GITHUB_TOKEN_URL",
    "G5_GITHUB_USERINFO_URL",
    "G5_MICROSOFT_CLIENT_ID",
    "G5_MICROSOFT_CLIENT_SECRET",
    "G5_MICROSOFT_TENANT_ID",
    "G5_MICROSOFT_AUTHORIZE_URL",
    "G5_MICROSOFT_TOKEN_URL",
    "G5_MICROSOFT_JWKS_URL",
    "G5_SCM_GITLAB_CLIENT_ID",
    "G5_SCM_GITLAB_CLIENT_SECRET",
    "G5_SCM_GITLAB_AUTHORIZE_URL",
    "G5_SCM_GITLAB_TOKEN_URL",
    "G5_SCM_GITLAB_USERINFO_URL",
    "G5D_ISOLATION_MODE",
    "G5D_LLM_PROVIDER",
    "G5D_LLM_MODEL",
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "OPENROUTER_API_KEY",
    "SELF_HOSTED",
    "ENABLE_CLOUD_FEATURES",
    "RUST_LOG",
]

SAFE_ENV_VALUE = re.compile(r"^[A-Za-z0-9_./:@%+=,-]*$")


def shell_env_value(value: str) -> str:
    if "\0" in value:
        raise ValueError("environment values cannot contain NUL bytes")
    if SAFE_ENV_VALUE.fullmatch(value):
        return value

    escaped: list[str] = []
    for ch in value:
        if ch == "\\":
            escaped.append("\\\\")
        elif ch == "'":
            escaped.append("\\'")
        elif ch == "\n":
            escaped.append("\\n")
        elif ch == "\r":
            escaped.append("\\r")
        elif ch == "\t":
            escaped.append("\\t")
        else:
            escaped.append(ch)
    return "$'" + "".join(escaped) + "'"


def apply_updates(path: Path, updates: dict[str, str]) -> None:
    validate_jwt_key_format(updates)
    lines: list[str] = []
    seen: set[str] = set()
    for line in path.read_text().splitlines():
        if "=" in line and not line.startswith("#"):
            key, _ = line.split("=", 1)
            if key in updates:
                lines.append(f"{key}={shell_env_value(updates[key])}")
                seen.add(key)
                continue
        lines.append(line)

    for key, value in updates.items():
        if key not in seen:
            lines.append(f"{key}={shell_env_value(value)}")

    path.write_text("\n".join(lines) + "\n")


def validate_jwt_key_format(updates: dict[str, str]) -> None:
    private_key = updates.get("G5_JWT_PRIVATE_KEY", "")
    if "-----BEGIN EC PRIVATE KEY-----" in private_key:
        raise ValueError(
            "G5_JWT_PRIVATE_KEY must be a PKCS#8 P-256 PEM beginning with "
            "'-----BEGIN PRIVATE KEY-----'. Generate it with: "
            "openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out jwt-private.pem"
        )


def main() -> int:
    if len(sys.argv) != 7:
        print(
            "usage: apply-install-env.py <env-file> <license-key> <api-url> <install-dir> <data-dir> <external-url>",
            file=sys.stderr,
        )
        return 2

    env_file, license_key, api_url, install_dir, data_dir, external_url = sys.argv[1:7]
    updates = {
        "G5_HOME": install_dir,
        "G5_LICENSE_KEY": license_key,
        "G5_API_URL": api_url,
        "G5_DATA_DIR": data_dir,
        "G5_EXTERNAL_URL": external_url,
    }
    for key in ENV_OVERRIDE_KEYS:
        if key in os.environ:
            updates[key] = os.environ[key]

    try:
        apply_updates(Path(env_file), updates)
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
