#!/usr/bin/env python3
"""Provision complete stack credentials; stdout is shell input, never a log."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shlex
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PRIVATE = "G5_JWT_PRIVATE_KEY"
PUBLIC = "G5_JWT_PUBLIC_KEY"
SCM = "G5_SCM_ENCRYPTION_KEY"
USER_SIGNER = "G5_JWT_SECRET"
E2E_SIGNER = "G5_E2E_JWT_PRIVATE_KEY"
KEYS = (PRIVATE, PUBLIC, SCM)
FIXTURE_PUBLIC_SHA256 = "d94d519f7b045527885a75667bb1bc07bcf7b82c1d57c48e49d96fcafaa701d0"
FIXTURE_SCM = bytes(range(32)).hex()


def openssl(*args, data=b""):
    result = subprocess.run(["openssl", *args], input=data, capture_output=True)
    if result.returncode:
        raise ValueError("Invalid ES256 stack key material (values withheld)")
    return result.stdout


def validate(bundle):
    if not isinstance(bundle, dict) or set(bundle) != set(KEYS):
        raise ValueError("Stack credentials must contain a complete JWT pair and SCM key")
    invalid = [key for key in KEYS if not isinstance(bundle[key], str) or not bundle[key]]
    if invalid:
        raise ValueError("Empty or invalid stack credential fields: " + ", ".join(invalid)
                         + ". Supply complete credentials; see docs/local-stack-secrets.md for recovery")
    # Reuse the same PKCS#8/P-256 policy as self-hosted provisioning.
    result = subprocess.run(
        ["bash", "-euo", "pipefail", "-c",
         'source "$1"; is_compatible_es256_private_key "$G5_BOOTSTRAP_PRIVATE"',
         "bash", str(ROOT / "scripts/lib/jwt-keys.sh")],
        env={**os.environ, "G5_BOOTSTRAP_PRIVATE": bundle[PRIVATE]},
        capture_output=True,
    )
    if result.returncode:
        raise ValueError(f"{PRIVATE} must be a PKCS#8 P-256 private key")
    derived = openssl("pkey", "-pubout", "-outform", "DER", data=bundle[PRIVATE].encode())
    supplied = openssl("pkey", "-pubin", "-outform", "DER", data=bundle[PUBLIC].encode())
    if derived != supplied:
        raise ValueError("The stack JWT private/public keys do not match")
    if hashlib.sha256(derived).hexdigest() == FIXTURE_PUBLIC_SHA256:
        raise ValueError("The public JWT fixture key cannot be used by a runnable stack")
    try:
        scm = bytes.fromhex(bundle[SCM])
    except ValueError:
        raise ValueError(f"{SCM} must contain 64 hex characters") from None
    if len(bundle[SCM]) != 64 or len(scm) != 32:
        raise ValueError(f"{SCM} must contain 64 hex characters")
    if scm == bytes(range(32)):
        raise ValueError("The public SCM fixture key requires deliberate data migration or reprovisioning")
    return bundle


def generate():
    result = subprocess.run(
        ["bash", "-euo", "pipefail", "-c",
         'source "$1"; private=$(generate_es256_private_key); '
         'public=$(printf "%s\\n" "$private" | derive_es256_public_key); '
         'printf "%s\\0%s" "$private" "$public"',
         "bash", str(ROOT / "scripts/lib/jwt-keys.sh")],
        capture_output=True,
    )
    if result.returncode:
        raise ValueError("Could not generate stack JWT keys (values withheld)")
    private, public = result.stdout.decode().split("\0", 1)
    return validate({PRIVATE: private, PUBLIC: public, SCM: secrets.token_hex(32)})


def require_empty_local_data(env=None):
    """Do not introduce a new encryption key over unbound retained data."""
    def docker(*args):
        result = subprocess.run(["docker", *args], env=env, text=True, capture_output=True)
        if result.returncode:
            raise ValueError("Cannot check local Docker data; refusing to generate replacement credentials")
        return result.stdout.strip()

    volumes = set(docker("volume", "ls", "--format", "{{.Name}}").splitlines())
    if volumes.intersection({"g5-lean_pgdata_dev", "g5-lean_api_repos"}):
        raise ValueError("Existing G5 dev data has no retained secret bundle; restore its complete credentials or deliberately migrate/reprovision it")
    ids = docker("ps", "-aq", "--filter", "label=com.docker.compose.project").splitlines()
    if ids:
        for container in json.loads(docker("inspect", *ids)):
            labels = container.get("Config", {}).get("Labels", {}) or {}
            files = labels.get("com.docker.compose.project.config_files", "")
            project = labels.get("com.docker.compose.project", "")
            networks = container.get("NetworkSettings", {}).get("Networks", {}) or {}
            g5_identity = project and (
                project in {"g5-dev", "g5-test"}
                or re.fullmatch(r"g5-local-[0-9a-f]{10}", project)
                # Include the former daemon name when detecting retained data.
                or labels.get("com.docker.compose.service") in {"g5-api", "g5-daemon", "g5-api-indexer", "g5d-host"}
                or labels.get("io.g5.validation.project") == project
                or any(f"{project}_{network}" in networks for network in ("g5-dev", "g5-test"))
            )
            if g5_identity and any(
                Path(name).name in {"docker-compose.dev.yml", "docker-compose.test.yml"}
                for name in files.split(",")
            ):
                raise ValueError("A retained G5 local stack has no secret bundle; restore its credentials or deliberately remove the disposable test stack with just ci-docker-down; see docs/local-stack-secrets.md")


def sync_directory_chain(path):
    # mkdir may have created any ancestor, including in another initializer or
    # a prior failed attempt. Flush child-to-parent without guessing which
    # pre-existing directory entries were already durable.
    for parent in (path, *path.parents):
        directory = os.open(parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)


def retained_bundle(path, *, local, probe_env=None):
    path = Path(path).expanduser().absolute()
    resolved = path.resolve()
    if resolved.is_relative_to(ROOT) or any((parent / ".git").exists() for parent in resolved.parents):
        raise ValueError("Store the secret bundle outside the source checkout/build context")
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    # One shared path serializes initializers from different worktrees.
    lock_fd = os.open(str(path) + ".lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(lock_fd, "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if path.is_symlink():
            raise ValueError("The retained stack credential file must not be a symlink")
        if path.exists():
            if path.stat().st_mode & 0o077:
                raise ValueError("Restrict the retained stack credential file to mode 0600")
            try:
                bundle = json.loads(path.read_text())
            except (ValueError, UnicodeError):
                raise ValueError("Retained stack credentials are corrupt; restore them instead of regenerating") from None
            validate(bundle)
            # A prior rename may have succeeded before its directory flush failed.
            # Retrying must make that same bundle durable before it can be used.
            sync_directory_chain(path.parent)
            return bundle
        if local:
            require_empty_local_data(probe_env)
        bundle = generate()
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, prefix=".g5-secrets-", delete=False) as stream:
                temporary = Path(stream.name)
                json.dump(bundle, stream)
                stream.write("\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, path)
            sync_directory_chain(path.parent)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
        return bundle


def local_bundle(env=None):
    env = os.environ if env is None else env
    if any(key in env for key in (*KEYS, USER_SIGNER)):
        supplied = {key: env.get(key, "") for key in KEYS}
        supplied[PRIVATE] = supplied[PRIVATE] or env.get(USER_SIGNER, "")
        if USER_SIGNER in env and env[USER_SIGNER] != supplied[PRIVATE]:
            raise ValueError("The local user and worker JWT signers must match")
        return validate(supplied)
    config_root = Path(env.get("XDG_CONFIG_HOME", Path(env.get("HOME", Path.home())) / ".config"))
    path = env.get("G5_LOCAL_STACK_SECRETS_FILE", config_root / "g5/local-stack-secrets.json")
    return retained_bundle(path, local=True, probe_env=env)


def shell_exports(bundle):
    return "\n".join(f"export {key}={shlex.quote(value)}"
                     for key, value in {**bundle, USER_SIGNER: bundle[PRIVATE], E2E_SIGNER: bundle[PRIVATE]}.items())


def configured_environment(root, env=None):
    """Resolve the recipe's .env inputs before validation records its identity."""
    env = dict(os.environ if env is None else env)
    result = subprocess.run(
        ["bash", "-euo", "pipefail", "-c",
         'set -a; if [[ -f .env ]]; then source .env; fi; '
         'exec "$1" -c "import json, os; print(json.dumps(dict(os.environ)))"',
         "bash", sys.executable],
        cwd=root, env=env, text=True, capture_output=True,
    )
    if result.returncode:
        raise ValueError("Cannot load local .env configuration (values withheld)")
    try:
        configured = json.loads(result.stdout)
    except ValueError:
        raise ValueError("Local .env must not write to stdout") from None
    bundle = local_bundle(configured)
    return {**bundle, USER_SIGNER: bundle[PRIVATE], E2E_SIGNER: bundle[PRIVATE]}


def bundle_digest(bundle):
    return hashlib.sha256(json.dumps(bundle, sort_keys=True).encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["local", "job"])
    parser.add_argument("path", nargs="?")
    args = parser.parse_args()
    if args.mode == "job" and not args.path:
        parser.error("job requires a private path outside the checkout")
    if args.mode == "local" and args.path:
        parser.error("use G5_LOCAL_STACK_SECRETS_FILE for local configuration")
    bundle = local_bundle() if args.mode == "local" else retained_bundle(args.path, local=False)
    expected = os.environ.get("G5_CI_STACK_SECRETS_DIGEST")
    if expected and bundle_digest(bundle) != expected:
        raise ValueError("Stack credentials changed after validation captured its configuration")
    print(shell_exports(bundle))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError) as error:
        print(f"Stack secret setup failed: {error}", file=sys.stderr)
        sys.exit(1)
