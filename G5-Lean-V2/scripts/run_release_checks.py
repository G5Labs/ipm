#!/usr/bin/env python3
"""Required external workspace checks for release candidates.

Uses explicitly provisioned synthetic fixtures, never customer workspaces. The
existing supported standalone helper owns disposable VM/clone cleanup. This gate
checks real agent execution and preview; browser login, edit/save/restart, tenant
isolation and backup restoration remain required manual preactivation acceptance.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlparse

from app_release import read_env, require


def validate_config(config, environment, base_url):
    require(isinstance(config, dict), "Release check secret must contain a JSON object")
    parsed = urlparse(base_url)
    require(parsed.scheme == "https" and parsed.hostname and not parsed.username
            and not parsed.password and parsed.path in {"", "/"}
            and not parsed.query and not parsed.fragment,
            "Release check URL must be an explicit HTTPS origin")
    require(config.get("environment") == environment and config.get("base_url", "").rstrip("/") == base_url.rstrip("/"),
            "Release fixture environment/URL does not match deployment")
    for stage in ("chat", "preview"):
        fixture = config.get(stage) or {}
        require(isinstance(fixture, dict), f"{stage} fixture must be a JSON object")
        for field in ("org_id", "fixture_workspace_id"):
            require(re.fullmatch(r"[0-9a-fA-F-]{36}", fixture.get(field, "")),
                    f"{stage}.{field} must identify a provisioned synthetic fixture")
        require(fixture.get("api_key", "").startswith("g5k_"), f"{stage} requires an org-scoped API key")
    require(config["preview"].get("profile"), "Preview fixture requires an explicit committed preview profile")


def main():
    env = os.environ
    workspace = Path(env.get("WORKSPACE_DIR", "/workspace"))
    if read_env(workspace / "deploy-control.env").get("SKIP_DEPLOY") == "true":
        print("Skipped release checks for a skipped deployment")
        return 0
    required = env.get("_RELEASE_CHECKS_REQUIRED") == "true"
    if not required:
        require(not env.get("_NEXT_TRIGGER") or env["_ENVIRONMENT"] == "qa",
                "Staging cannot promote while external checks are disabled")
        print("External release acceptance DISABLED: internal bootstrap only; customer readiness is unverified")
        return 0
    require(env.get("_RELEASE_CHECK_BASE_URL") and env.get("_RELEASE_CHECK_SECRET"),
            "Required release checks need an explicit URL and secret; missing configuration is a failure")
    preview_hostname = env.get("_PREVIEW_HOSTNAME", "")
    # Match the standalone canary's bare DNS hostname check before any stage runs.
    label = r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?"
    require(len(preview_hostname) <= 253 and re.fullmatch(rf"{label}(?:\.{label})*", preview_hostname),
            "Required release checks need _PREVIEW_HOSTNAME as a bare hostname (no scheme, path, port or userinfo)")
    payload = subprocess.check_output([
        "gcloud", "secrets", "versions", "access", "latest",
        f"--project={env['_TARGET_PROJECT']}", f"--secret={env['_RELEASE_CHECK_SECRET']}",
    ], text=True)
    try:
        config = json.loads(payload)
    except ValueError:
        raise ValueError("Release check secret must contain valid JSON") from None
    validate_config(config, env["_ENVIRONMENT"], env["_RELEASE_CHECK_BASE_URL"])
    refs = read_env(workspace / "image-refs.env")
    require(refs.get("G5D_TUNGSTEN_IMAGE_REF"), "Agent check requires the promoted Tungsten runtime image")
    script = Path(__file__).with_name("canary-tungsten-microvm.sh")
    for stage in ("chat", "preview"):
        fixture = config[stage]
        # Explicitly clear personal/legacy defaults and out-of-band notification
        # hooks. Preprovision the org's LLM config; never import a developer key.
        child = {key: value for key, value in env.items() if not key.startswith("G5_CANARY_")
                 and key not in {"ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL", "ANTHROPIC_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL"}}
        child.update({
            "G5_CANARY_API_URL": env["_RELEASE_CHECK_BASE_URL"],
            "G5_CANARY_API_KEY": fixture["api_key"],
            "G5_CANARY_ORG_ID": fixture["org_id"],
            "G5_CANARY_ORG_SLUG": "release-check-fixture",
            "G5_CANARY_FIXTURE_WS_ID": fixture["fixture_workspace_id"],
            "G5_CANARY_STAGES": stage,
            "G5_CANARY_BASE_IMAGE_REF": refs["G5D_TUNGSTEN_IMAGE_REF"] if stage == "chat" else "org-default",
            "G5_CANARY_PREVIEW_PROFILE": config["preview"]["profile"],
            "G5_CANARY_PREVIEW_HOST_SUFFIX": preview_hostname,
            "G5_CANARY_REGION": env["_AR_REGION"],
            "G5_CANARY_LLM_SECRET_FILE": "/dev/null",
            "G5_CANARY_REFRESH_TOKEN_FILE": "/dev/null",
        })
        subprocess.run(["bash", str(script)], env=child, check=True)
    print("Required agent/preview checks passed. Separate manual preactivation acceptance remains required for login, save/restart, isolation and restore.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as exc:
        # Do not print secret JSON, command environment or response bodies.
        message = str(exc) if isinstance(exc, ValueError) else "Release check failed; inspect the preceding stage result"
        print(f"Release checks refused: {message}", file=sys.stderr)
        sys.exit(1)
