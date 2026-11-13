#!/usr/bin/env python3
"""Evaluate a follow-up native run against the expectations snapshotted in the run's inputs/.

  python3 -I -B evaluate_native.py RUN_DIR NAME     (NAME: r05, p21 or r04)

Writes RUN_DIR/verdict.json. A case CONFORMS when every expected field equals its observation,
DEVIATES when one differs, INCOMPLETE when a field is missing, measured twice with different values,
or the case markers or run outcome are wrong. `@ALL` means "equals the case's `total`", `@FIRST2`
means "the IDs listed in the case's `first2`".
"""
import json, os, re, sys
from datetime import datetime, timezone

OBS_RE = re.compile(r"(?:^|>)\s*OBS (\S+) ([^=\s]+)=(.*)$")
MARK_RE = re.compile(r"(?:^|>)\s*CASE-(BEGIN|END) (\S+)")


def main(argv):
    run, name = argv[0], argv[1]
    rec = json.load(open(os.path.join(run, "receipt.json")))
    exp = json.load(open(os.path.join(run, "inputs", "cases", f"expected-{name}.json")))["cases"]
    obs, marks, native_end = {}, {}, False
    for line in open(os.path.join(run, "raw.log"), encoding="utf-8", errors="replace"):
        line = line.rstrip("\r\n")
        native_end |= line.startswith("NATIVE-END")
        m = MARK_RE.search(line)
        if m:
            marks.setdefault(m.group(2), []).append(m.group(1))
            continue
        m = OBS_RE.search(line)
        if m:
            obs.setdefault(m.group(1), {}).setdefault(m.group(2), []).append(m.group(3).rstrip())
    out = {}
    for c in exp:
        cid = c["id"]
        o = obs.get(cid, {})
        problems = []
        if rec["process_outcome"] != "COMPLETED" or not native_end:
            problems.append("run not completed")
        if marks.get(cid) != ["BEGIN", "END"]:
            problems.append(f"markers {marks.get(cid)}")
        conflicts = sorted(k for k, v in o.items() if len(set(v)) > 1)
        if conflicts:
            problems.append("conflicting measurements " + ",".join(conflicts))
        val = {k: v[0] for k, v in o.items()}
        fields = {}
        for k, want in c["expect"].items():
            got = val.get(k)
            if want == "@ALL":
                want = val.get("total")
            elif want == "@FIRST2":
                ids = val.get("first2", "").split(",")
                got = ",".join(x.split("#")[-1] for x in (got or "").split(",")) if got is not None else None
                want = ",".join(ids)
            fields[k] = {"expected": want, "observed": got, "result": "MISSING" if got is None else ("AGREE" if got == want else "DIFFER")}
        missing = [k for k, f in fields.items() if f["result"] == "MISSING"]
        differ = [k for k, f in fields.items() if f["result"] == "DIFFER"]
        verdict = "INCOMPLETE" if (problems or missing) else ("DEVIATES" if differ else "CONFORMS")
        out[cid] = {"verdict": verdict, "differ": differ, "missing": missing, "problems": problems, "source": c["source"],
                    "fields": fields, "observations": val}
        print(cid, verdict, ",".join(differ) or "-", ";".join(problems + [f"missing {m}" for m in missing]))
    json.dump({"run_id": rec["run_id"], "process_outcome": rec["process_outcome"], "native_end": native_end, "cases": out,
               "evaluator_sha256": __import__("hashlib").sha256(open(os.path.abspath(__file__), "rb").read()).hexdigest(),
               "evaluated_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")},
              open(os.path.join(run, "verdict.json"), "w"), indent=1, sort_keys=True)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
