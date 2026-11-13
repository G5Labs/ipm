#!/usr/bin/env python3
"""Narrow, fail-closed TLC 2.19 adapter for model_evidence.py.

Run --help for the CLI. G5_MODEL_RESULT must name a new result file. Raw Java
stdout/stderr are passed through unchanged. Only single-worker exhaustive BFS,
one selected invariant or safety/action property, and the tested -tool protocol
are supported. A witness is a targeted violation of a *negated reachability*
invariant with a complete finite trace; its meaning still needs model review.

Config subset: one SPECIFICATION name, one INVARIANT/PROPERTY name, and optional
single-line CONSTANT name = literal (integer, Boolean, string or model value).
Blank lines and \\* line comments are allowed; other directives are rejected.
Action-property failures require an exact --action-label because TLC may report
a source location instead of the configured property name. Liveness, simulation,
initial-state violations, multiple properties, constraints and symmetry are not
qualified by this adapter. No result is an unbounded or runtime correctness proof.
"""

from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


VERSION = "TLC2 Version 2.19 of 08 August 2024 (rev: 5a47802)"
NAME = r"[A-Za-z_][A-Za-z_0-9]*"
START = re.compile(r"@!@!@STARTMSG (-?\d+):(\d+) @!@!@")
END = re.compile(r"@!@!@ENDMSG (-?\d+) @!@!@")
INFO_CODES = {2262, 2187, 2220, 2219, 2185, 2189, 2269, 2190, 2191,
              2207, 2208, 2193, 2200, 2199, 2194, 2268, 2186}
MAX_PARSE_BYTES = 16 * 1024 * 1024


def selected_property(path, property_name):
    """Validate the supported cfg subset, without guessing TLC's full grammar."""
    declarations = []
    specs = []
    constants = set()
    literal = rf'(?:-?\d+|TRUE|FALSE|"[^"\\]*"|{NAME})'
    for raw in path.read_text().splitlines():
        line = raw.split("\\*", 1)[0].strip()
        if not line:
            continue
        match = re.fullmatch(rf"(SPECIFICATION|INVARIANT|PROPERTY)\s+({NAME})", line)
        if match:
            (specs if match[1] == "SPECIFICATION" else declarations).append(match.groups())
            continue
        match = re.fullmatch(rf"CONSTANT\s+({NAME})\s*=\s*{literal}", line)
        if not match or match[1] in constants:
            raise ValueError(f"unsupported/duplicate config declaration: {line}")
        constants.add(match[1])
    if len(specs) != 1 or len(declarations) != 1 or declarations[0][1] != property_name:
        raise ValueError("config must select exactly the requested property and one specification")
    return declarations[0][0]


def messages(stdout):
    frames = []
    active = None
    body = []
    for line in stdout.splitlines():
        start = START.fullmatch(line)
        end = END.fullmatch(line)
        if start:
            if active is not None:
                raise ValueError("nested/incomplete TLC message")
            active = (int(start[1]), int(start[2]))
            body = []
        elif end:
            if active is None or int(end[1]) != active[0]:
                raise ValueError("unmatched TLC message terminator")
            frames.append((*active, "\n".join(body).strip()))
            active = None
        elif "@!@!@" in line:
            raise ValueError("unrecognized TLC message marker")
        elif active is not None:
            body.append(line)
        # SANY's ordinary parsing lines are unframed. Its errors must also cause
        # a failed TLC terminal status; no unframed text can grant acceptance.
    if active is not None:
        raise ValueError("truncated TLC message")
    return frames


def parse_result(stdout, stderr, returncode, kind, property_name, action_label=None):
    if stderr.strip():
        raise ValueError("Java/TLC wrote stderr; inspect raw evidence")
    frames = messages(stdout)
    if not frames or frames[0] != (2262, 0, VERSION):
        raise ValueError("unsupported or missing TLC version")
    ids = [code for code, _, _ in frames]
    for required in [2262, 2187, 2219, 2185, 2199, 2186]:
        if ids.count(required) != 1:
            raise ValueError(f"missing/duplicate required TLC message {required}")
    if ids[-1] != 2186:
        raise ValueError("TLC did not reach its terminal message")
    for code, severity, _ in frames:
        if ((severity == 0 and code in INFO_CODES)
                or (severity == 1 and code in {2110, 2112, 2121})
                or (severity == 4 and code == 2217)):
            continue
        raise ValueError(f"unsupported/error TLC message {code}:{severity}")
    stats_text = next(body for code, _, body in frames if code == 2199)
    stats = re.fullmatch(r"([\d,]+) states generated, ([\d,]+) distinct states found, "
                         r"([\d,]+) states left on queue\.", stats_text)
    if not stats:
        raise ValueError("unrecognized final state counts")
    generated, distinct, remaining = [int(item.replace(",", "")) for item in stats.groups()]
    if distinct < 1 or generated < distinct:
        raise ValueError("empty or inconsistent explored state space")
    result = {"complete": True, "properties": [property_name], "tlc_version": VERSION,
              "tlc_returncode": returncode, "generated_states": generated,
              "distinct_states": distinct, "remaining_states": remaining}
    violations = [(code, body) for code, _, body in frames if code in {2110, 2112}]
    if not violations:
        if (returncode != 0 or ids.count(2193) != 1 or remaining != 0
                or 2121 in ids or 2217 in ids):
            raise ValueError("no complete exhaustive success verdict")
        success = next(body for code, _, body in frames if code == 2193)
        if not success.startswith("Model checking completed. No error has been found.\n"):
            raise ValueError("unrecognized success verdict")
        result["outcome"] = "satisfied"
        return result
    if len(violations) != 1 or 2193 in ids or ids.count(2121) != 1:
        raise ValueError("ambiguous or incomplete counterexample")
    code, body = violations[0]
    if kind == "INVARIANT":
        if code != 2110 or body != f"Invariant {property_name} is violated." or returncode != 12:
            raise ValueError("violation is not the exact requested invariant")
    elif (code != 2112 or not action_label or returncode != 13
          or body != f"Action property {action_label} is violated."):
        raise ValueError("violation is not the exact reviewed action-property label")
    trace = [body for code, _, body in frames if code == 2217]
    if len(trace) < 2:
        raise ValueError("missing finite counterexample trace")
    for index, state in enumerate(trace, 1):
        if not re.match(rf"{index}: <[^>]+>\n.+", state, re.DOTALL):
            raise ValueError("incomplete or unordered counterexample states")
    if not trace[0].startswith("1: <Initial predicate>\n"):
        raise ValueError("counterexample does not start at the initial state")
    result.update(outcome="violated", trace=trace, diagnostic=body)
    return result


def execute(args):
    root = args.root.resolve()
    config = (root / args.config).resolve()
    kind = selected_property(config, args.property)
    if args.expect == "witness" and kind != "INVARIANT":
        raise ValueError("witness mode requires a negated-reachability invariant")
    if args.expect == "violated" and kind == "PROPERTY" and not args.action_label:
        raise ValueError("action-property negative control requires an exact action label")
    if not re.fullmatch(NAME, args.module):
        raise ValueError("module must be a TLA module name, not arbitrary TLC options")
    if not math.isfinite(args.timeout) or args.timeout <= 0 or args.heap_mb < 1:
        raise ValueError("timeout and heap must be positive")
    java = args.java.resolve(strict=True)
    jar = args.jar.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="g5-tlc-") as scratch:
        scratch = Path(scratch)
        command = [str(java), f"-Xmx{args.heap_mb}m", "-XX:+UseParallelGC", "-cp", str(jar),
                   "tlc2.TLC", "-tool", "-workers", "1", "-fp", "0", "-seed", "1",
                   "-metadir", str(scratch / "states"), "-config", str(config), args.module]
        timed_out = False
        with (scratch / "stdout").open("wb") as out, (scratch / "stderr").open("wb") as err:
            process = subprocess.Popen(command, cwd=root, stdout=out, stderr=err)
            try:
                returncode = process.wait(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                process.kill()
                returncode = process.wait()
        for name, destination in [("stdout", sys.stdout.buffer), ("stderr", sys.stderr.buffer)]:
            with (scratch / name).open("rb") as stream:
                shutil.copyfileobj(stream, destination)
            destination.flush()
        if timed_out:
            raise ValueError("TLC execution timed out; no completed result")
        if any((scratch / name).stat().st_size > MAX_PARSE_BYTES for name in ["stdout", "stderr"]):
            raise ValueError("TLC output exceeds qualified adapter parsing limit")
        result = parse_result((scratch / "stdout").read_text(), (scratch / "stderr").read_text(),
                              returncode, kind, args.property, args.action_label)
        if args.expect == "witness" and result["outcome"] == "violated":
            result["outcome"] = "witness"
        result.update(module=args.module, config=str(config), java_command=command)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for name in ["java", "jar", "root", "config"]:
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--module", required=True)
    parser.add_argument("--property", required=True)
    parser.add_argument("--expect", choices=["satisfied", "violated", "witness"], required=True)
    parser.add_argument("--action-label")
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--heap-mb", type=int, default=256)
    args = parser.parse_args()
    result_path = os.environ.get("G5_MODEL_RESULT")
    if not result_path or Path(result_path).exists():
        parser.error("G5_MODEL_RESULT must name a new output file")
    try:
        result = execute(args)
        matched = result["outcome"] == args.expect
    except (OSError, ValueError, UnicodeError) as error:
        result = {"complete": False, "error": str(error)}
        matched = False
    with Path(result_path).open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    if not matched:
        print(f"TLC adapter rejected evidence: {result.get('error', 'observed outcome differs from expectation')}",
              file=sys.stderr)
    return 0 if matched else 1


if __name__ == "__main__":
    raise SystemExit(main())
