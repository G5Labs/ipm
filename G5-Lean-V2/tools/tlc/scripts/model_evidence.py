#!/usr/bin/env python3
"""Record and check model-run evidence; the project owns TLC result parsing.

Usage: model_evidence.py run MANIFEST RUN_DIRECTORY [--root CHECKOUT]
       model_evidence.py check MANIFEST RUN_DIRECTORY [--root CHECKOUT]

Manifest: {"command": ["python3", "tlc_adapter.py"], "timeout_seconds": 300,
 "inputs": {"formal": ["Model.tla", "Model.cfg"], "tool": ["tlc_adapter.py",
 "tla2tools.jar"], "ontology": ["contract.md"], "binding": ["binding.md"],
 "implementation": ["src/handler.cs"]},
 "expected": {"outcome": "satisfied", "properties": ["Invariant"]}}

Paths and command cwd are relative to --root (default: manifest directory).
Relative and empty PATH entries also resolve there; the recorded absolute executable
is the executable invoked for the run.
The adapter writes JSON to the absolute G5_MODEL_RESULT path supplied by this
wrapper: {"complete": true, "outcome": "satisfied", "properties": ["Invariant"]}.
Other supported outcomes are "violated" and "witness". A counterexample/control
matching its expectation is not a safety pass. The adapter must inspect TLC's
actual terminal verdict, property and trace, not just its exit code. Adapter and
checker sources, tools, configs and transitive formal dependencies must be listed.

Each new run directory retains stdout, stderr, parsed result and a receipt.
Exit 0 means completed evidence matches the declared expectation and fingerprints;
exit 1 means failed, incomplete, changed or corrupt evidence; exit 2 is usage/setup.
Check never reruns a command. It flags changed implementation/ontology/bindings for
applicability review even when formal inputs are unchanged. A reviewer decides
whether a rerun is necessary; this helper does not certify that decision.

This is integrity/accounting support, not a semantic validator, trusted attestation,
dependency discoverer, universal TLC parser or production-conformance proof. It
cannot detect omitted inputs, dishonest adapters, edits restored between snapshots,
environment-dependent behavior, or coordinated rewriting of evidence and its receipt.
Declare environment assumptions as inputs where relevant. Isolate run inputs; keep the
receipt in versioned evidence and inspect model, adapter and binding semantics.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time


OUTCOMES = {"satisfied", "violated", "witness"}


def digest(path):
    with Path(path).open("rb") as stream:
        h = hashlib.sha256()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def load_json(path):
    return json.loads(Path(path).read_text())


def expectation(value):
    if not isinstance(value, dict) or value.get("outcome") not in OUTCOMES:
        raise ValueError("expected/result outcome must be satisfied, violated or witness")
    props = value.get("properties")
    if (not isinstance(props, list) or not props
            or any(not isinstance(p, str) or not p.strip() for p in props)
            or len(set(props)) != len(props)):
        raise ValueError("properties must be a nonempty list of distinct names")
    return {"outcome": value["outcome"], "properties": sorted(props)}


def manifest(path):
    data = load_json(path)
    if not isinstance(data, dict):
        raise ValueError("manifest must be a JSON object")
    command = data.get("command")
    if (not isinstance(command, list) or not command
            or any(not isinstance(p, str) or not p for p in command)):
        raise ValueError("command must be a nonempty argv list; no shell string")
    timeout = data.get("timeout_seconds")
    if (not isinstance(timeout, (float, int)) or isinstance(timeout, bool)
            or not math.isfinite(timeout) or timeout <= 0):
        raise ValueError("timeout_seconds must be finite and positive")
    inputs = data.get("inputs")
    if not isinstance(inputs, dict) or not inputs.get("formal") or not inputs.get("tool"):
        raise ValueError("inputs must declare at least formal and tool files")
    for role, paths in inputs.items():
        if (not isinstance(role, str) or not role or not isinstance(paths, list)
                or any(not isinstance(p, str) or not p for p in paths)
                or len(set(paths)) != len(paths)):
            raise ValueError("each input role must contain distinct file paths")
    expectation(data.get("expected"))
    return data


def snapshot(data, root):
    result = {}
    for role, paths in data["inputs"].items():
        result[role] = {p: digest(root / p) for p in paths}
    return result


def observed(path, expected):
    value = load_json(path)
    if not isinstance(value, dict) or value.get("complete") is not True:
        raise ValueError("adapter did not report a complete run")
    actual = expectation(value)
    if actual != expectation(expected):
        raise ValueError("observed outcome/properties differ from current expectation")
    return actual


def now():
    return datetime.now(timezone.utc).isoformat()


def run(manifest_path, output, root):
    data = manifest(manifest_path)
    manifest_hash = digest(manifest_path)
    before = snapshot(data, root)
    env = os.environ.copy()
    search_path = os.pathsep.join(str((root / entry).absolute())
                                 for entry in os.get_exec_path(env))
    executable = shutil.which(data["command"][0], path=search_path)
    if os.sep in data["command"][0]:
        executable = str((root / data["command"][0]).absolute())
    executable_hash = digest(executable) if executable else None
    output.mkdir(parents=True, exist_ok=False)
    receipt = {"version": 1, "started": now(), "command": data["command"],
               "manifest_sha256": manifest_hash, "inputs": before,
               "expected": expectation(data["expected"]), "root": str(root),
               "runner_sha256": digest(__file__), "executable": executable,
               "executable_sha256": executable_hash, "errors": [],
               "returncode": None, "timed_out": False, "matched": False}
    started = time.monotonic()
    env["G5_MODEL_RESULT"] = str(output / "result.json")
    with (output / "stdout.log").open("xb") as stdout, (output / "stderr.log").open("xb") as stderr:
        try:
            if executable is None:
                raise FileNotFoundError(f"executable not found: {data['command'][0]}")
            process = subprocess.Popen(data["command"], executable=executable, cwd=root, env=env,
                                       stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                receipt["returncode"] = process.wait(timeout=data["timeout_seconds"])
            except subprocess.TimeoutExpired:
                receipt["timed_out"] = True
                try:
                    if os.name == "posix":
                        os.killpg(process.pid, signal.SIGKILL)
                    else:
                        process.kill()
                except ProcessLookupError:
                    pass  # The process may have exited while handling the timeout.
                receipt["returncode"] = process.wait()
                receipt["errors"].append("execution timed out")
        except OSError as error:
            receipt["errors"].append(f"could not execute adapter: {error}")
    receipt["finished"] = now()
    receipt["elapsed_seconds"] = time.monotonic() - started
    if receipt["returncode"] != 0:
        receipt["errors"].append(f"adapter exit status is {receipt['returncode']}")
    try:
        if digest(manifest_path) != manifest_hash or snapshot(data, root) != before:
            receipt["errors"].append("declared inputs or manifest changed during execution")
        if executable and digest(executable) != executable_hash:
            receipt["errors"].append("executable changed during execution")
        receipt["observed"] = observed(output / "result.json", data["expected"])
    except (OSError, ValueError, TypeError) as error:
        receipt["errors"].append(str(error))
    receipt["artifacts"] = {name: digest(output / name) for name in
                            ["stdout.log", "stderr.log", "result.json"] if (output / name).is_file()}
    receipt["matched"] = not receipt["errors"]
    # Never overwrite an execution or its receipt, including unsuccessful runs.
    with (output / "receipt.json").open("x") as file:
        json.dump(receipt, file, indent=2)
        file.write("\n")
    return receipt["errors"]


def check(manifest_path, output, root):
    data = manifest(manifest_path)
    receipt = load_json(output / "receipt.json")
    if not isinstance(receipt, dict):
        raise ValueError("receipt must be a JSON object")
    errors = []
    if receipt.get("version") != 1:
        errors.append("unsupported receipt version")
    if (receipt.get("matched") is not True or receipt.get("errors") != []
            or receipt.get("returncode") != 0 or receipt.get("timed_out") is not False):
        errors.append("execution was unsuccessful or incomplete")
    if digest(manifest_path) != receipt.get("manifest_sha256"):
        errors.append("manifest changed: review inputs, command and expectations")
    if data["command"] != receipt.get("command"):
        errors.append("command differs from receipt")
    if expectation(data["expected"]) != receipt.get("expected"):
        errors.append("current expectation differs from receipt")
    current = snapshot(data, root)
    recorded = receipt.get("inputs", {})
    if not isinstance(recorded, dict):
        raise ValueError("receipt inputs must be a JSON object")
    for role in sorted(set(current) | set(recorded)):
        if current.get(role) != recorded.get(role):
            errors.append(f"{role} inputs changed: applicability review required")
    if digest(__file__) != receipt.get("runner_sha256"):
        errors.append("evidence helper changed: applicability review required")
    executable = receipt.get("executable")
    if not executable or digest(executable) != receipt.get("executable_sha256"):
        errors.append("recorded executable is missing or changed")
    artifacts = receipt.get("artifacts", {})
    if not isinstance(artifacts, dict):
        raise ValueError("receipt artifacts must be a JSON object")
    if set(artifacts) != {"stdout.log", "stderr.log", "result.json"}:
        errors.append("missing execution artifacts")
    for name in ["stdout.log", "stderr.log", "result.json"]:
        if digest(output / name) != artifacts.get(name):
            errors.append(f"corrupt or changed artifact: {name}")
    if observed(output / "result.json", data["expected"]) != receipt.get("observed"):
        errors.append("parsed result differs from receipt")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("action", choices=["run", "check"])
    parser.add_argument("manifest", type=Path)
    parser.add_argument("run_directory", type=Path)
    parser.add_argument("--root", type=Path)
    args = parser.parse_args()
    root = (args.root or args.manifest.resolve().parent).resolve()
    try:
        errors = globals()[args.action](args.manifest.resolve(), args.run_directory.resolve(), root)
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f"Evidence {args.action} failed: {error}", file=sys.stderr)
        return 2 if args.action == "run" else 1
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Evidence matches the declared expectation; this is not a whole-system or implementation correctness claim.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
