#!/usr/bin/env python3
"""Collect IRIS observations and IPM test results from validation/logs into evidence/*.json.

Observation lines look like  OBS|<name>|k=v;k=v;...  (they may follow IPM output on the same line).
Usage: python3 -I scripts/collect_evidence.py
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOGS = ROOT / "validation" / "logs"
EVID = ROOT / "evidence"
OBS_RE = re.compile(r"OBS\|([^|]+)\|(.*)$")


def parse_kv(s: str) -> dict:
    out = {}
    for part in s.split(";"):
        if "=" in part:
            k, _, v = part.partition("=")
            out[k.strip()] = v.strip()
    return out


def header(text: str) -> dict:
    m = re.search(r"^# container: (\S+)\s+image: (\S+)\s+at: (\S+)", text, re.M)
    build = re.search(r"IRIS for UNIX .*?\(Build ([^)]+)\)", text)
    return {
        "container": m.group(1) if m else None,
        "image": m.group(2) if m else None,
        "at": m.group(3) if m else None,
    }


def superseded() -> dict:
    """{log file name: set of OBS names, or {"*"}} from validation/logs/SUPERSEDED.tsv (log, obs or *, reason)."""
    out = {}
    path = LOGS / "SUPERSEDED.tsv"
    if not path.exists():
        return out
    for i, line in enumerate(path.read_text().splitlines()):
        if i == 0 or not line.strip():
            continue
        parts = line.split("\t")
        out.setdefault(parts[0].strip(), set()).add(parts[1].strip())
    return out


def main() -> int:
    EVID.mkdir(exist_ok=True)
    records = {}
    dups = []
    skip = superseded()
    skipped = []
    for log in sorted(LOGS.glob("*.log")):
        text = log.read_text(errors="replace")
        meta = header(text)
        for line in text.splitlines():
            m = OBS_RE.search(line)
            if not m:
                continue
            name, kv = m.group(1), parse_kv(m.group(2))
            if log.name in skip and ("*" in skip[log.name] or name in skip[log.name]):
                skipped.append((log.name, name))
                continue
            if name.startswith("error-"):
                print(f"probe error in {log.name}: {line[:200]}")
                continue
            if name in records:
                dups.append((name, records[name]["log"], log.name))
            records[name] = {"name": name, "log": f"validation/logs/{log.name}", "values": kv, **meta}
    install = (LOGS / "00-install-ipm.log").read_text(errors="replace") if (LOGS / "00-install-ipm.log").exists() else ""
    b = re.search(r"(IRIS for UNIX .*?\(Build [^)]+\)[^\n]*)", install)
    env = {
        "iris": b.group(1).strip() if b else None,
        "ipm_commit": "571e04a19aba9f9b4bc0ea23fbe09182038f4e55",
        "ipm_manifest_version": "0.10.11-SNAPSHOT",
    }
    (EVID / "iris-observations.json").write_text(json.dumps({"environment": env, "records": sorted(records.values(), key=lambda r: r["name"])}, indent=2) + "\n")

    tests = []
    for log in sorted(LOGS.glob("1*-ipm-*.log")):
        text = log.read_text(errors="replace")
        for m in re.finditer(r"RESULT (\S+) (.*)", text):
            pass
        # whole-class runs
        for m in re.finditer(r"Methods: (\d+) total, (\d+) passed, (\d+) failed\s+Assertions: (\d+) total, (\d+) passed, (\d+) failed", text):
            pass
    hist = LOGS / "11-ipm-history-class.log"
    if hist.exists():
        t = hist.read_text(errors="replace")
        m = re.search(r"Methods: (\d+) total, (\d+) passed, (\d+) failed\s+Assertions: (\d+) total, (\d+) passed, (\d+) failed", t)
        ok = "RESULT History OK" in t
        tests.append({"name": "Test.PM.Integration.History", "status": "pass" if ok and m and m.group(3) == "0" else "fail",
                      "methods": m.group(1) if m else None, "assertions": m.group(4) if m else None,
                      "log": "validation/logs/11-ipm-history-class.log", "note": "whole class, with the zot ORAS registry from tests/oras on a shared network"})
    # Any other log may record IPM test runs as lines "RESULT <Test.Class[#Method]> OK|<error>"
    for extra in sorted(LOGS.glob("*.log")):
        if extra.name.startswith(("10-", "11-")):
            continue
        t = extra.read_text(errors="replace")
        for m in re.finditer(r"^RESULT (Test\.\S+) (.*)$", t, re.M):
            rest = m.group(2).strip()   # may carry a label such as "(verbose,no-quiet)" before the status
            ok = (rest == "OK" or rest.endswith(" OK")) and "ERROR" not in rest
            tests.append({"name": m.group(1), "status": "pass" if ok else "fail",
                          "detail": m.group(2).strip()[:300], "log": f"validation/logs/{extra.name}"})
    # Skipped assertions per test class, read from ^UnitTest.Result in every container that still exists
    # (validation/logs/54-unittest-skips-*.log, lines UTRES|run|class|pass=|fail=|skip=).
    skips, seen = {}, set()
    for log in sorted(LOGS.glob("54-unittest-skips-*.log")):
        for line in log.read_text(errors="replace").splitlines():
            if line.startswith("UTRES|"):
                parts = line.split("|")
                cls, sk = parts[2], parts[5].split("=")[1] or "0"
                seen.add(cls)
                skips[cls] = max(skips.get(cls, 0), int(sk))
    merged = {}
    for r in tests:
        cls = r["name"].split("#")[0].split("(")[0]
        r["skipped_assertions"] = skips.get(cls, 0) if cls in seen else "unknown (run's container removed)"
        m = merged.setdefault(r["name"], {"name": r["name"], "runs": [], "status": "pass", "skipped_assertions": r["skipped_assertions"]})
        m["runs"].append({k: v for k, v in r.items() if k != "name"})
        if r["status"] != "pass":
            m["status"] = "fail"    # conservative: a class passes only if every recorded run passed
    tests = list(merged.values())
    (EVID / "ipm-tests.json").write_text(json.dumps({"environment": env, "records": tests}, indent=2) + "\n")
    print(f"{len(records)} observations, {len(tests)} test records")
    print(f"skipped {len(skipped)} superseded observation lines (validation/logs/SUPERSEDED.tsv)")
    for s in skipped:
        print("  skipped:", s)
    for d in dups:
        print("note: observation name repeated (latest kept):", d)
    return 0


if __name__ == "__main__":
    sys.exit(main())
