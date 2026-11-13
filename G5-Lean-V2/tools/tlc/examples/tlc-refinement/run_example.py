#!/usr/bin/env python3
"""Create fresh manifests/receipts for the small refinement qualification suite.

Usage: python3 run_example.py --java /path/to/java --jar /path/to/tla2tools.jar
                             --output /new/evidence/directory

This driver reports whether expected acceptance AND rejection controls behaved
correctly. Its exit 0 is qualification of these examples, not 14 safety proofs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


EXAMPLE = Path(__file__).resolve().parent
SCRIPTS = EXAMPLE.parent.parent / "scripts"
LABEL = "line 22, col 35 to line 22, col 66 of module Implementation"
CASES = [
    ("good", "good.cfg", "RefinesIntent", "satisfied", 0, []),
    ("bad-map", "bad-map.cfg", "RefinesIntent", "violated", 0, []),
    ("bad-write", "bad-write.cfg", "RefinesIntent", "violated", 0, []),
    ("staged-witness", "stage-witness.cfg", "NoStageThenCommit", "witness", 0, []),
    ("stuck", "stuck.cfg", "RefinesIntent", "satisfied", 0, []),
    ("missing-path", "stuck-witness.cfg", "NoStageThenCommit", "witness", 1, []),
    ("weak-statement", "weak.cfg", "WeakTypeOnly", "satisfied", 0, []),
    ("direct", "direct.cfg", "RefinesIntent", "satisfied", 0, []),
    ("direct-witness", "direct-witness.cfg", "NoDirectCommit", "witness", 0, []),
    ("wrong-property-label", "bad-map.cfg", "RefinesIntent", "violated", 1,
     ["--action-label", "UnrelatedProperty"]),
    ("parse-error", "parse-error.cfg", "Safe", "violated", 1, ["--module", "ParseError"]),
    ("config-error", "config-error.cfg", "MissingInvariant", "violated", 1, []),
    ("resource-error", "stage-witness.cfg", "NoStageThenCommit", "witness", 1, ["--heap-mb", "1"]),
    ("timeout", "stage-witness.cfg", "NoStageThenCommit", "witness", 1, ["--timeout", "0.001"]),
]


def write_new(path, data):
    with path.open("x") as output:
        json.dump(data, output, indent=2)
        output.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java", type=Path, required=True)
    parser.add_argument("--jar", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    java, jar, output = args.java.resolve(strict=True), args.jar.resolve(strict=True), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    tools = [SCRIPTS / "tlc_adapter.py", SCRIPTS / "model_evidence.py", Path(__file__), java, jar]
    version = subprocess.run([str(java), "-version"], capture_output=True, text=True, check=True)
    write_new(output / "toolchain.json", {
        "java_version": version.stderr or version.stdout,
        "sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in tools},
        "driver_command": [sys.executable, *sys.argv],
    })
    summary = []
    for name, cfg, prop, outcome, expected_exit, extra in CASES:
        case = output / name
        case.mkdir()
        command = [sys.executable, str(SCRIPTS / "tlc_adapter.py"), "--java", str(java), "--jar", str(jar),
                   "--root", str(EXAMPLE), "--module", "Implementation", "--config", cfg,
                   "--property", prop, "--expect", outcome, "--action-label", LABEL, "--timeout", "10", *extra]
        module = "ParseError.tla" if name == "parse-error" else "Implementation.tla"
        manifest = case / "manifest.json"
        write_new(manifest, {
            "command": command, "timeout_seconds": 20,
            "inputs": {"formal": [str(EXAMPLE / item) for item in [module, "Intent.tla", cfg]],
                       "tool": [str(path) for path in tools],
                       "ontology": [str(EXAMPLE / "intent.md")],
                       "binding": [str(EXAMPLE / "bindings.md")],
                       "implementation": [str(EXAMPLE / "store_fixture.py")]},
            "expected": {"outcome": outcome, "properties": [prop]},
        })
        run_command = [sys.executable, str(SCRIPTS / "model_evidence.py"), "run", str(manifest), str(case / "run")]
        execution = subprocess.run(run_command, capture_output=True, text=True, timeout=30)
        (case / "helper.stdout.log").write_text(execution.stdout)
        (case / "helper.stderr.log").write_text(execution.stderr)
        check_command = [*run_command[:2], "check", *run_command[3:]]
        check = subprocess.run(check_command, capture_output=True, text=True, timeout=30)
        (case / "check.stdout.log").write_text(check.stdout)
        (case / "check.stderr.log").write_text(check.stderr)
        matched = execution.returncode == expected_exit and check.returncode == expected_exit
        receipt = json.loads((case / "run" / "receipt.json").read_text())
        result = json.loads((case / "run" / "result.json").read_text())
        item = {"case": name, "expected_wrapper_exit": expected_exit,
                "wrapper_exit": execution.returncode, "check_exit": check.returncode,
                "qualification_matches": matched, "evidence_matches": receipt["matched"],
                "result": result, "run_command": run_command, "check_command": check_command}
        summary.append(item)
        print(f"{name}: qualification={'matched' if matched else 'FAILED'}; evidence={receipt['matched']}")
    write_new(output / "summary.json", summary)
    return 0 if all(item["qualification_matches"] for item in summary) else 1


if __name__ == "__main__":
    raise SystemExit(main())
