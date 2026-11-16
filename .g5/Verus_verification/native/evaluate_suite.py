#!/usr/bin/env python3
"""Verdict for one product %UnitTest suite run. Usage: python3 -I evaluate_suite.py RUN_DIR TESTCASE [--index CONCERN]
PASS needs process COMPLETED, no environment flag, at least one method of TESTCASE in the extractor output,
no failed method, and IPM's own summary line present and agreeing. A zero-test result is INCOMPLETE; all methods skipped is NO-EFFECTIVE-TESTS.
Without IPM's "Methods:" line, or when its total differs from the extracted methods (a run covers only TESTCASE;
IPM counts skipped methods as passed and the extractor lists them with status 2), the extractor may have read
another TestInstance: INCOMPLETE."""
import json, os, re, sys
from datetime import datetime, timezone

def main(a):
    run_dir, tc = a[0], a[1]
    concern = a[a.index("--index") + 1] if "--index" in a else None
    rec = json.load(open(os.path.join(run_dir, "receipt.json")))
    log = open(os.path.join(run_dir, "raw.log"), encoding="utf-8", errors="replace").read()
    env = [n for n, p in [("license", r"LICENSE LIMIT|License limit exceeded"), ("disk", r"<DISKHARD>|<FILEFULL>|No space left")] if re.search(p, log, re.I)]
    methods = {}
    for m in re.finditer(r"^UTMETHOD (\S+) (\S+) status=(\S*)", log, re.M):
        if m.group(1) == tc:
            methods[m.group(2)] = m.group(3)
    failed_asserts = [l for l in re.findall(r"^UTFAILED (.*)$", log, re.M) if l.startswith(tc + " ")]
    ipm = re.findall(r"Methods: (\d+) total, (\d+) passed, (\d+) failed", log)
    asserts = re.findall(r"Assertions: (\d+) total, (\d+) passed, (\d+) failed", log)
    skipped = re.findall(r"(\d+) skipped", log)
    n_fail = sum(1 for s in methods.values() if s == "0")
    n_pass = sum(1 for s in methods.values() if s == "1")
    other = {k: v for k, v in methods.items() if v not in ("0", "1")}
    if rec["process_outcome"] != "COMPLETED" or env or not methods:
        verdict = "INCOMPLETE"
    elif n_fail or failed_asserts:
        verdict = "FAIL"
    elif ipm and int(ipm[-1][2]) != 0:
        verdict = "FAIL"
    elif not ipm or int(ipm[-1][0]) != len(methods):
        verdict = "INCOMPLETE"
    elif n_pass == 0:
        verdict = "NO-EFFECTIVE-TESTS"  # every method skipped: no evidence either way
    else:
        verdict = "PASS"
    out = {"run_id": rec["run_id"], "suite": tc, "process_outcome": rec["process_outcome"], "environment_flags": env,
           "verdict": verdict, "methods": methods, "methods_passed": n_pass, "methods_failed": n_fail,
           "methods_other_status": other, "failed_assertions": failed_asserts,
           "ipm_summary_methods": ipm[-1] if ipm else None, "ipm_summary_assertions": asserts[-1] if asserts else None,
           "evaluated_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
           "note": "status=1 passed, 0 failed, other values (e.g. 2 skipped) listed separately"}
    json.dump(out, open(os.path.join(run_dir, "verdict.json"), "w"), indent=2)
    if concern:
        with open(os.path.normpath(os.path.join(run_dir, "..", "..", "index.jsonl")), "a") as fh:
            fh.write(json.dumps({"concern": concern, "label": rec["label"],
                                 "outcome": f"{rec['process_outcome']}; {verdict} {n_pass}/{len(methods)} methods",
                                 "run_id": rec["run_id"], "kind": "native", "variant": "as-built",
                                 "expect": f"all methods of {tc} pass"}) + "\n")
    print(json.dumps({k: out[k] for k in ("run_id", "suite", "verdict", "methods_passed", "methods_failed", "methods_other_status", "ipm_summary_methods", "ipm_summary_assertions", "environment_flags")}))

if __name__ == "__main__":
    main(sys.argv[1:])
