#!/usr/bin/env python3
"""Local validation entry point. Receipts are evidence, never a replacement for CI."""

from __future__ import annotations

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import time
import uuid

import validation_cleanup
import workspace_metadata
import stack_secrets

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / ".dev/validation"
COMPOSE = "infra/docker/docker-compose.test.yml"
VERSION = 1


class ValidationBusyError(ValueError):
    """Cache contention is retryable, distinct from a validation failure."""


def capture(*args, root=None):
    root = ROOT if root is None else root
    return subprocess.check_output(args, cwd=root, text=True, stderr=subprocess.PIPE, timeout=60).strip()


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")
        temporary = stream.name
    os.replace(temporary, path)


def file_digest(root, paths):
    result = hashlib.sha256()
    for name in sorted(set(paths)):
        path = root / name
        result.update(os.fsencode(name) + b"\0")
        if path.is_symlink():
            result.update(b"symlink\0" + os.fsencode(os.readlink(path)))
        elif path.is_file():
            result.update(str(path.stat().st_mode & 0o777).encode() + b"\0")
            with path.open("rb") as stream:
                while block := stream.read(1024 * 1024):
                    result.update(block)
        elif path.is_dir():
            raise ValueError(f"Cannot fingerprint directory/gitlink: {name}")
        else:
            result.update(b"missing")
        result.update(b"\0")
    return result.hexdigest()


def candidate(root=None):
    root = ROOT if root is None else root
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root,
    ).decode().strip("\0").split("\0")
    # Local configuration affects the application even though Git ignores it.
    paths += [".env"]
    return {"head": capture("git", "rev-parse", "HEAD", root=root),
            "source": file_digest(root, paths), "worktree": str(root.resolve())}


def builder_inputs(root=None):
    """Read the local runner's actual COPY inputs instead of another crate list."""
    root = ROOT if root is None else root
    dockerfile = root / "infra/docker/Dockerfile.ci-builder"
    contents = dockerfile.read_text().replace("\\\n", " ")
    parent = re.search(r"^ARG BASE_IMAGE=(\S+)", contents, re.MULTILINE)
    if not parent or "$" in parent[1]:
        raise ValueError("CI builder must declare a concrete BASE_IMAGE default")
    inputs = [str(dockerfile.relative_to(root)), ".dockerignore",
              "infra/docker/Dockerfile.ci-builder.dockerignore"]
    for line in contents.splitlines():
        if not line.startswith(("COPY ", "ADD ")):
            continue
        body = line.split(None, 1)[1]
        if body.startswith("["):
            tokens = json.loads(body)
        else:
            tokens = shlex.split(body)
        if any(token.startswith("--") or "$" in token for token in tokens):
            raise ValueError(f"Unsupported CI builder input instruction: {line}")
        for source in tokens[:-1]:
            paths = list(root.glob(source))
            if not paths:
                raise ValueError(f"Missing CI builder input: {source}")
            for path in paths:
                if path.is_dir():
                    inputs.extend(str(child.relative_to(root)) for child in path.rglob("*")
                                  if child.is_file() or child.is_symlink())
                else:
                    inputs.append(str(path.relative_to(root)))
    return parent[1], inputs


def builder_key(coverage, release):
    parent, inputs = builder_inputs()
    return digest({"files": file_digest(ROOT, inputs), "coverage": coverage, "release": release,
                   "parent": capture("docker", "image", "inspect", parent, "--format", "{{.Id}}")})


def project_name(root=None):
    root = ROOT if root is None else root
    name = os.environ.get("G5_CI_COMPOSE_PROJECT", "g5-local-" + digest(str(root.resolve()))[:10])
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,50}", name):
        raise ValueError("G5_CI_COMPOSE_PROJECT must be a lowercase Compose project name (max 51 characters)")
    return name


@contextlib.contextmanager
def validation_lock(project):
    # The harness still shares Cargo/base-image caches. Serialize local runs on
    # this host, while project names isolate their runtime services and volumes.
    path = Path(tempfile.gettempdir()) / f"g5-local-validation-{os.getuid()}.lock"
    with path.open("a+") as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ValidationBusyError("Another local validation run owns the shared build caches; retry when it finishes (exit 75)") from exc
        yield


def workspace_fingerprint(raw):
    if not raw:
        return None
    workspace = json.loads(raw)
    if not workspace:
        return None
    # Multiple repos have a "main" branch. PostgreSQL row order can change on
    # a no-op update; hash semantic identity, not physical/aggregate ordering.
    workspace["branches"] = sorted(workspace.get("branches") or [],
                                   key=lambda branch: (branch["repo_id"], branch["branch_name"]))
    return digest(workspace)


def stack_snapshot(project):
    ids = capture("docker", "ps", "-aq", "--filter", f"label=com.docker.compose.project={project}").split()
    if not ids:
        raise ValueError("no containers belong to this validation project")
    containers = json.loads(capture("docker", "inspect", *ids))
    required = {"postgres", "oauth-stub", "g5-api", "client"}
    seen = set()
    records = []
    for container in containers:
        service = container["Config"]["Labels"].get("com.docker.compose.service")
        state = container["State"]
        if service in required or service == "g5d-host":
            if not state.get("Running") or state.get("Paused") or state.get("Restarting"):
                raise ValueError(f"{service} is not running and unpaused")
            if state.get("Health", {}).get("Status", "healthy") != "healthy":
                raise ValueError(f"{service} is not healthy")
            seen.add(service)
        elif state.get("Status") == "exited" and state.get("ExitCode") != 0:
            raise ValueError(f"setup service {service} failed")
        records.append({"service": service, "id": container["Id"], "image": container["Image"],
                        "started": state.get("StartedAt"), "restarts": container.get("RestartCount", 0)})
    if required - seen:
        raise ValueError(f"missing required services: {', '.join(sorted(required - seen))}")
    images = {}
    image_names = {container["Config"].get("Image") for container in containers}
    image_names.discard(None)
    image_names.update([os.environ.get("G5_CI_IMAGE", "g5-ci-runner"),
                        os.environ.get("G5_CI_PLAYWRIGHT_IMAGE", f"{project}-playwright:ci")])
    for name in sorted(image_names):
        try:
            images[name] = capture("docker", "image", "inspect", name, "--format", "{{.Id}}")
        except subprocess.CalledProcessError:
            images[name] = None
    pg = next(item["id"] for item in records if item["service"] == "postgres")
    schema = capture("docker", "exec", pg, "psql", "-U", "g5", "-d", "g5_test", "-tAc",
                     "SELECT version || ':' || encode(checksum, 'hex') || ':' || success FROM _sqlx_migrations ORDER BY version")
    if not schema or any(line.endswith(":false") for line in schema.splitlines()):
        raise ValueError("migration catalog is empty or contains a failed migration")
    fixtures = [".env.test"]
    workspace = ""
    environment = ROOT / ".env.test"
    if environment.exists():
        match = re.search(r"^G5_E2E_TEST_WORKSPACE_ID=([0-9a-f-]{36})$", environment.read_text(), re.MULTILINE)
        if match:
            workspace = capture("docker", "exec", pg, "psql", "-U", "g5", "-d", "g5_test", "-tAc",
                "SELECT json_build_object('workspace', w.id, 'org', w.org_id, 'branches', "
                "(SELECT json_agg(row_to_json(b) ORDER BY b.repo_id) FROM "
                "(SELECT CASE WHEN w.is_base THEN r.default_branch ELSE w.name END AS branch_name, m.repo_id, m.role, m.root_path FROM workspace_repos m JOIN repos r ON r.id=m.repo_id AND r.org_id=m.org_id "
                "WHERE m.workspace_id = w.id) b)) FROM workspaces w WHERE w.id = '" + match[1] + "'")
    # Store only hashes: environment and fixture files may contain credentials.
    return {"version": VERSION, "candidate": candidate(), "project": project,
            "containers": sorted(records, key=lambda item: item["id"]), "images": images,
            "migrations": digest(schema), "fixtures": file_digest(ROOT, fixtures),
            "workspace": workspace_fingerprint(workspace),
            "configuration": os.environ.get("G5_CI_INPUT_CONFIG_DIGEST", "unmanaged")}


def stack_receipt(project):
    return STATE / f"stack-{project}.json"


def prepared_phases(snapshot, phase, previous=None, prepare_all=False):
    """An image's presence is not evidence it was built for this candidate."""
    ready = {phase}
    if phase == "6":
        ready.add("5")
        if prepare_all:
            ready.add("7")
    if phase == "7" and previous:
        runner = os.environ.get("G5_CI_IMAGE", "g5-ci-runner")
        if (previous.get("candidate") == snapshot["candidate"]
                and previous.get("configuration") == snapshot["configuration"]
                and previous.get("images", {}).get(runner) == snapshot["images"].get(runner)):
            ready.update(set(previous.get("ready_phases", [])) & {"5", "6"})
    return sorted(ready)


def check_stack(project, phase):
    saved = json.loads(stack_receipt(project).read_text())
    current = stack_snapshot(project)
    changed = [key for key in current if saved.get(key) != current[key]]
    if changed:
        raise ValueError("retained stack differs: " + ", ".join(changed))
    if phase not in saved.get("ready_phases", []):
        raise ValueError(f"stack was not prepared for Phase {phase}")
    if phase in {"5", "6"} and not current["images"].get(os.environ.get("G5_CI_IMAGE", "g5-ci-runner")):
        raise ValueError("the Rust runner image is missing")
    if phase == "7" and not current["images"].get(os.environ.get("G5_CI_PLAYWRIGHT_IMAGE", f"{project}-playwright:ci")):
        raise ValueError("the browser runner image is missing")
    if phase in {"6", "7"} and not (ROOT / ".env.test").is_file():
        raise ValueError("seeded workspace environment is missing")
    if phase in {"6", "7"} and not current["workspace"]:
        raise ValueError("seeded workspace is absent from the live database")
    print("Verified retained stack: source, containers, images, schema and fixture files match.", flush=True)


def preflight_commands():
    return [
        ("layout", [sys.executable, "scripts/check-layout.py"]),
        ("commands", [sys.executable, "scripts/check-just-commands.py"]),
        ("migrations", [sys.executable, "scripts/check-migration-checksums.py"]),
        ("migration-versions", [sys.executable, "scripts/check_local_validation_contracts.py"]),
        ("fixture-contracts", [sys.executable, "scripts/test_ci_docker_contract.py"]),
        ("cloud-seed-targets", [sys.executable, "scripts/test_seed_prod_org.py"]),
        ("codex-seed-example", [sys.executable, "scripts/test_seed_codex.py"]),
        ("stack-secrets", [sys.executable, "scripts/test_stack_secrets.py"]),
        ("qa-reset", [sys.executable, "scripts/test_reset_qa.py"]),
        ("build-profile-contract", ["bash", "infra/docker/test-dev-build-profile.sh"]),
        ("validation-harness", [sys.executable, "scripts/test_local_validation.py"]),
        ("review-regressions", [sys.executable, "scripts/test_validation_review.py"]),
    ]


def run_preflight(directory):
    records = []
    initial = candidate()
    for name, command in preflight_commands():
        print(f"=== Preflight: {name} ===", flush=True)
        started = time.monotonic()
        log = directory / f"preflight-{name}.log"
        with log.open("w") as stream:
            try:
                status = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT).returncode
            except OSError as error:
                stream.write(str(error) + "\n")
                status = 127
        record = {"phase": name, "status": status, "candidate": initial,
                  "duration_seconds": round(time.monotonic() - started, 3),
                  "log": str(log), "rerun": shlex.join(command)}
        records.append(record)
        print(f"  {'PASS' if status == 0 else 'FAIL'} in {record['duration_seconds']:.2f}s; {log}", flush=True)
        atomic_json(directory / "preflight.json", records)
        if status:
            print(f"  Rerun: {record['rerun']}\n{log.read_text()[-8000:]}", flush=True)
            return status
    if candidate() != initial:
        records.append({"phase": "source-stability", "status": 1, "candidate": initial,
                        "rerun": "just validation-preflight"})
        atomic_json(directory / "preflight.json", records)
        print("Source changed during preflight; rerun just validation-preflight.", flush=True)
        return 1
    return 0


def normalize(arguments):
    phase, live, rust, browser, reuse = (arguments + ["0", "false", "", "", "auto"][len(arguments):])
    known = {"0", "0b", "1", "1b", "2", "3", "4", "4a", "4b", "4c", "5", "5b", "5c", "6", "6b", "7"}
    if phase not in known:
        if live != "false" or rust or browser:
            raise ValueError("selector shorthand cannot be combined with other selector arguments")
        selector = phase
        phase = "7" if re.search(r"\.spec\.ts|e2e/|--grep|playwright", selector) else (
            "5" if re.search(r"binary\(|package\(|kind\(|platform\(|test\(|::", selector) else "6")
        rust, browser = ("", selector) if phase == "7" else (selector, "")
    elif live not in {"true", "false"} and not rust and not browser and phase in {"5", "6", "7"}:
        rust, browser = ("", live) if phase == "7" else (live, "")
        live = "false"
    if live != "false":
        raise ValueError("local validation is deterministic; use just test-live-e2e for live providers")
    if reuse not in {"auto", "true", "false", "1", "0", "yes", "no", "always", "never"}:
        raise ValueError(f"invalid reuse mode: {reuse}")
    return [phase, live, rust, browser, reuse]


def rerun_command(args, project, phase=None, prepare=False):
    if prepare:
        return shlex.join(["env", f"G5_CI_COMPOSE_PROJECT={project}", "just", "ci-docker-prepare"])
    rerun = list(args)
    if phase:
        rerun[0] = phase
    # auto fails closed to fresh setup if retained state cannot be verified.
    rerun[4] = "auto"
    return shlex.join(["env", f"G5_CI_COMPOSE_PROJECT={project}", "G5_CI_KEEP_STACK=on-failure",
                       "just", "ci-docker", *rerun])


def cargo_metadata():
    """Use Cargo's target model, including explicitly located integration tests.

    Routed through the gateway so selector narrowing and `recommend` run without
    host Rust (G5-Lean#355): host cargo when present, else a pinned offline
    container.
    """
    return workspace_metadata.cargo_metadata()


def integration_inventory(source=None):
    """Keep the recipe's package inventory authoritative for both paths."""
    source = (ROOT / "Justfile").read_text() if source is None else source
    section = source.split("    # Phase 5: Integration", 1)[1].split("    # Phase 5b:", 1)[0]
    return set(re.findall(r"-p ([A-Za-z0-9_-]+)", section))


def integration_targets(selector, source=None):
    """Narrow Cargo only for a provably conjunctive exact package/binary slice."""
    clauses = selector.split(" and ")
    package = re.fullmatch(r"package\(=([A-Za-z0-9_-]+)\)", clauses[0])
    if not package:
        return []
    binary = "*"
    for clause in clauses[1:]:
        match = re.fullmatch(r"(binary|test)\(=([A-Za-z0-9_:.-]+)\)", clause)
        if not match:
            return []
        if match[1] == "binary":
            if binary != "*":
                return []
            binary = match[2]
    if package[1] not in integration_inventory(source):
        return []
    targets = {target["name"] for item in cargo_metadata()["packages"] if item["name"] == package[1]
               for target in item["targets"] if "test" in target["kind"]}
    if not targets or (binary != "*" and binary not in targets):
        return []
    return ["-p", package[1], "--test", binary]


def failed_selector(line, phase):
    if phase == "5":
        match = re.search(r"\bFAIL\s+\[[^]]+\]\s+(?:\(\s*\d+/\d+\)\s+)?\S+\s+(\S+)", line)
        return f"test(={match[1]})" if match else None
    if phase == "6":
        match = re.match(r"test (\S+) \.\.\. FAILED", line)
        return match[1] if match else None
    if phase == "7":
        match = re.search(r"\b(e2e/[^\s]+\.spec\.ts:\d+)(?::\d+)?", line)
        return match[1] if match else None
    return None


def observed_counts(line, phase):
    # Nextest's indented failed-test stdout includes a libtest summary. Only
    # the nextest summary owns Phase 5 totals; counting both doubles failures.
    if phase in {"1", "5"} and "Summary" in line and "test" in line and "run:" in line:
        counts = dict((kind, int(number)) for number, kind in re.findall(r"(\d+) (passed|failed|skipped)", line))
        return {"passed": counts.get("passed", 0), "failed": counts.get("failed", 0), "ignored": counts.get("skipped", 0)}
    if phase == "5":
        return None
    if phase != "7":
        match = re.match(r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored", line)
        if match:
            return dict(zip(("passed", "failed", "ignored"), map(int, match.groups())))
    if phase == "7":
        match = re.match(r"\s*(\d+) (passed|failed|skipped|flaky)\b", line)
        if match:
            return {"passed": 0, "failed": 0, "ignored": 0} | {"ignored" if match[2] == "skipped" else match[2]: int(match[1])}
    return None


def execute(args, directory, project, prepare=False):
    env = dict(os.environ, G5_CI_COMPOSE_PROJECT=project, COMPOSE_PROJECT_NAME=project,
               G5_TEST_API_IMAGE=f"{project}/api:test", G5_CI_VALIDATION_MANAGED="1",
               G5_LOCAL_VALIDATION_WORKTREE=str(ROOT.resolve()),
               G5_CI_DOCKER_LOG_DIR=str(directory), G5_CI_PREPARE_ONLY="1" if prepare else "0")
    env.setdefault("G5_CI_PLAYWRIGHT_IMAGE", f"{project}-playwright:ci")
    env.setdefault("G5_CI_KEEP_STACK", "always" if prepare else "on-failure")
    config = {key: value for key, value in os.environ.items()
              if key.startswith(("G5_", "E2E_", "PLAYWRIGHT_", "DOCKER_", "CARGO_"))
              and key not in {"G5_CI_KEEP_STACK", "G5_CI_DOCKER_LOG_DIR", "G5_CI_COMPOSE_PROJECT"}}
    env["G5_CI_INPUT_CONFIG_DIGEST"] = digest(config)
    env["G5_CI_START_CANDIDATE"] = json.dumps(candidate(), sort_keys=True)
    env["G5_CI_PLAYWRIGHT_ARGS"] = json.dumps(shlex.split(args[3]))
    env["G5_CI_INTEGRATION_ARGS"] = json.dumps(integration_targets(args[2])) if args[0] == "5" else "[]"
    env["G5_CI_RUST_ARGS"] = json.dumps(shlex.split(args[2])) if args[0] == "6" else "[]"
    if env["G5_CI_KEEP_STACK"] not in {"always", "on-failure", "never"}:
        raise ValueError("G5_CI_KEEP_STACK must be always, on-failure, or never")
    if prepare:
        env["G5_CI_KEEP_STACK"] = "always"
    receipt = {"version": VERSION, "candidate": candidate(), "project": project, "arguments": args,
               "prepare_only": prepare, "phases": [], "failures": [], "failed_selectors": [], "status": "running",
               "test_counts": {}, "execution_ready_seconds": {},
               "artifacts": {"playwright": str(directory / "playwright")},
               "log": str(directory / "run.log"), "rerun": rerun_command(args, project, prepare=prepare)}
    atomic_json(directory / "receipt.json", receipt)
    start = time.monotonic()
    phase_start = start
    current_phase = "setup"
    announced = False
    process = subprocess.Popen(["just", "--quiet", "_ci-docker", *args], cwd=ROOT, env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)

    def interrupted(signum, frame):
        os.killpg(process.pid, signal.SIGTERM)

    handlers = {sig: signal.signal(sig, interrupted) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        with (directory / "run.log").open("w") as stream:
            for line in process.stdout:
                stream.write(line)
                stream.flush()
                line = re.sub(r"\x1b\[[0-9;]*[mK]", "", line)
                match = re.match(r"=== Phase ([0-7][abc]?):", line)
                if match:
                    receipt["phases"].append({"phase": current_phase, "status": "passed",
                                              "duration_seconds": round(time.monotonic() - phase_start, 3)})
                    current_phase, phase_start = match[1], time.monotonic()
                    atomic_json(directory / "receipt.json", receipt)
                if re.match(r"=== Phase [567] execution ready ===", line):
                    receipt["execution_ready_seconds"].setdefault(current_phase, round(time.monotonic() - start, 3))
                if line.strip() == "=== Chromium tests ===":
                    receipt["test_counts"]["7"] = {"passed": 0, "failed": 0, "ignored": 0}
                counts = observed_counts(line, current_phase)
                if counts:
                    totals = receipt["test_counts"].setdefault(current_phase, {"passed": 0, "failed": 0, "ignored": 0})
                    for key, value in counts.items():
                        totals[key] = totals.get(key, 0) + value
                selector = failed_selector(line, current_phase)
                if selector and selector not in receipt["failed_selectors"]:
                    receipt["failed_selectors"].append(selector)
                if re.search(r"\b(FAIL|FAILED|ERROR:)\b", line):
                    receipt["failures"] = (receipt["failures"] + [line.strip()])[-30:]
                if not announced and ("dumping" in line.lower() or "failure diagnostics" in line):
                    print(f"\nValidation failure in Phase {current_phase}.\nRerun: {rerun_command(args, project, current_phase if current_phase != 'setup' else None, prepare)}\nReceipt: {directory / 'receipt.json'}\nLog: {directory / 'run.log'}", flush=True)
                    announced = True
                print(line, end="", flush=True)
        status = process.wait()
    finally:
        process.stdout.close()
        for sig, handler in handlers.items():
            signal.signal(sig, handler)
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            process.wait()
    if candidate() != receipt["candidate"]:
        print("Source changed during validation; results do not validate the current candidate.", flush=True)
        status = status or 1
        stack_receipt(project).unlink(missing_ok=True)
    if not status and not prepare and args[0] in {"5", "6", "7"} and (args[2] or args[3]):
        counts = receipt["test_counts"].get(args[0], {})
        if not (counts.get("passed", 0) + counts.get("flaky", 0)):
            message = "The selected slice reported no passing tests; check the selector and ignore/skip markers."
            print(message, flush=True)
            receipt["failures"].append(message)
            status = 1
    receipt["phases"].append({"phase": current_phase, "status": "failed" if status else "passed",
                              "duration_seconds": round(time.monotonic() - phase_start, 3)})
    receipt.update(status=status, duration_seconds=round(time.monotonic() - start, 3))
    receipt["rerun"] = rerun_command(args, project, current_phase if current_phase != "setup" else None, prepare)
    if status and receipt["failed_selectors"] and current_phase in {"5", "6", "7"}:
        focused = list(args)
        focused[2 if current_phase in {"5", "6"} else 3] = receipt["failed_selectors"][0]
        receipt["rerun"] = rerun_command(focused, project, current_phase)
    saved = stack_receipt(project)
    if saved.exists():
        receipt["prepared_stack"] = json.loads(saved.read_text())
    atomic_json(directory / "receipt.json", receipt)
    print(f"\n{'FAIL' if status else 'PASS'}: {receipt['duration_seconds']:.2f}s\nReceipt: {directory / 'receipt.json'}", flush=True)
    if status:
        print(f"Rerun: {receipt['rerun']}", flush=True)
    return status


def recommendations(base):
    base = capture("git", "merge-base", "HEAD", base)
    changes = capture("git", "diff", "--name-only", base).splitlines()
    changes += capture("git", "ls-files", "--others", "--exclude-standard").splitlines()
    commands = {"just validation-preflight"}
    packages = {str(Path(p["manifest_path"]).parent.relative_to(ROOT)): p
                for p in cargo_metadata()["packages"] if Path(p["manifest_path"]).is_relative_to(ROOT)}
    inventory = integration_inventory()
    for path in changes:
        parts = Path(path).parts
        if len(parts) > 2 and parts[0] == "backend":
            item = packages.get(str(Path(*parts[:2])))
            if item:
                package = item["name"]
                kinds = {kind for target in item["targets"] for kind in target["kind"]}
                for kind, flag in [("lib", "--lib"), ("bin", "--bins")]:
                    if kind in kinds:
                        commands.add(shlex.join(["env", "SQLX_OFFLINE=true", "cargo", "test", "--locked", "-p", package, flag]))
                if package in inventory and "test" in kinds:
                    commands.add(shlex.join(["just", "ci-docker-5", f"package(={package})"]))
        if path.startswith("frontend/e2e/") and path.endswith(".spec.ts"):
            commands.add(shlex.join(["just", "ci-docker-7", path.removeprefix("frontend/")]))
        elif path.startswith("frontend/"):
            commands.update({"npm --prefix frontend test -- --run", "npm --prefix frontend run build"})
        if path.startswith("tests/e2e/"):
            commands.add("just ci-docker-6  # supply the affected test selector")
        if path.startswith(("infra/docker/", "scripts/")) or path in {"Justfile", "Cargo.toml", "Cargo.lock"}:
            commands.add("just ci-docker-prepare  # runtime/build changes need a fresh verified stack")
    print("Suggested local checks (selection is advisory; both current-head Cloud checks remain mandatory):")
    for command in sorted(commands):
        print(command)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["preflight", "run", "prepare", "check-stack", "record-stack", "invalidate-stack", "down", "gc", "recommend", "builder-key"])
    parser.add_argument("arguments", nargs="*")
    args = parser.parse_args()
    project = project_name()
    if args.action == "builder-key":
        if len(args.arguments) != 2:
            parser.error("builder-key requires coverage and release-warmup values")
        print(builder_key(*args.arguments))
        return 0
    if args.action == "recommend":
        recommendations(args.arguments[0] if args.arguments else "main")
        return 0
    if args.action == "check-stack":
        check_stack(project, args.arguments[0])
        return 0
    if args.action == "record-stack":
        snapshot = stack_snapshot(project)
        if snapshot["candidate"] != json.loads(os.environ["G5_CI_START_CANDIDATE"]):
            raise ValueError("source changed during preparation; the stack cannot be certified")
        phase = args.arguments[0]
        if phase in {"6", "7"}:
            if not any(item["service"] == "g5d-host" for item in snapshot["containers"]):
                raise ValueError("no workspace host was prepared")
            if not snapshot["workspace"]:
                raise ValueError("seeded workspace is absent from the live database")
        previous = None
        try:
            previous = json.loads(stack_receipt(project).read_text())
        except (OSError, ValueError):
            pass
        snapshot["ready_phases"] = prepared_phases(snapshot, phase, previous,
                                                  os.environ.get("G5_CI_PREPARE_ONLY") == "1")
        atomic_json(stack_receipt(project), snapshot)
        return 0
    if args.action == "invalidate-stack":
        stack_receipt(project).unlink(missing_ok=True)
        return 0
    if args.action in {"preflight", "run", "prepare"}:
        directory = STATE / "runs" / (time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:6])
        directory.mkdir(parents=True)
    # Source-only checks can run while another worktree prepares Docker images.
    if args.action == "preflight":
        return run_preflight(directory)
    with validation_lock(project):
        if args.action == "gc":
            if args.arguments not in ([], ["false"], ["true"]):
                parser.error("gc accepts false (preview) or true (remove)")
            return validation_cleanup.collect_orphan_images(args.arguments == ["true"])
        if args.action == "down":
            status = validation_cleanup.down(ROOT, COMPOSE, project)
            if status == 0:
                stack_receipt(project).unlink(missing_ok=True)
            return status
        command = normalize(args.arguments) if args.action == "run" else ["6", "false", "", "", "false"]
        status = run_preflight(directory)
        if status or args.action == "preflight":
            return status
        # Resolve .env and any generated bundle before execute fingerprints
        # configuration. The child verifies this identity after sourcing .env.
        credentials = stack_secrets.configured_environment(ROOT)
        os.environ.update(credentials)
        os.environ["G5_CI_STACK_SECRETS_DIGEST"] = stack_secrets.bundle_digest(
            {key: credentials[key] for key in stack_secrets.KEYS})
        return execute(command, directory, project, prepare=args.action == "prepare")


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ValidationBusyError as error:
        print(f"Local validation: {error}", file=sys.stderr)
        sys.exit(75)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"Local validation: {error}", file=sys.stderr)
        sys.exit(1)
