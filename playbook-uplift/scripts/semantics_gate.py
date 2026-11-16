#!/usr/bin/env python3
"""Playbook 3.3a gate: write-set obligations computed from measured write sets.

Inputs: semantics/semantics.json, semantics/coverage.json, evidence/iris-observations.json (ws-* write sets),
evidence/proofs-*.json, evidence/model-checks*.json, findings/*.md, decisions/*.md, ontology/NN-*.md.
Output: semantics/COVERAGE.md. Exit 1 if:
  - a measured global is not classified;
  - a module file has no semantics declaration, or declares an unknown mechanism/axiom;
  - an (invariant, command) pair whose write set meets the invariant's read set has no coverage entry;
  - a coverage entry cites evidence that does not resolve (P must be verified with controls killed, M must pass);
  - a read-only command writes an IPM fact (INV-FRAME).
  python3 -I scripts/semantics_gate.py [--check]
"""
import fnmatch
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
S = json.loads((ROOT / "semantics/semantics.json").read_text())
COV_PATH = ROOT / "semantics/coverage.json"
COV = json.loads(COV_PATH.read_text()) if COV_PATH.exists() else {}


def evidence_index():
    proofs, checks, obs, tests = {}, {}, {}, {}
    for f in (ROOT / "evidence").glob("proofs-*.json"):
        d = json.loads(f.read_text())
        for r in d["records"]:
            proofs[r["name"]] = r["status"] == "verified" and d.get("controls_all_killed") is True
    for f in (ROOT / "evidence").glob("model-checks*.json"):
        for r in json.loads(f.read_text())["records"]:
            checks[r["name"]] = r["status"] == "pass"
    for r in json.loads((ROOT / "evidence/iris-observations.json").read_text())["records"]:
        obs[r["name"]] = True
    for r in json.loads((ROOT / "evidence/ipm-tests.json").read_text())["records"]:
        tests[r["name"]] = r["status"] == "pass"
    findings = set()
    for f in (ROOT / "findings").glob("*.md"):
        findings |= set(re.findall(r"^#+\s*(F-[A-Z0-9]+-\d+)", f.read_text(), re.M))
    decisions = set()
    for f in (ROOT / "decisions").glob("*.md"):
        decisions |= set(re.findall(r"\|\s*(DEC-[A-Z0-9]+-\d+)\s*\|", f.read_text()))
    return proofs, checks, obs, tests, findings, decisions


def resolve(ref, idx):
    proofs, checks, obs, tests, findings, decisions = idx
    k, _, name = ref.partition(":")
    if k == "P":
        return proofs.get(name) is True
    if k == "M":
        return checks.get(name) is True
    if k == "O":
        return name in obs
    if k == "T":
        return tests.get(name) is True
    if k == "F":
        return name in findings
    if k == "DEC":
        return name in decisions
    if k == "GUARD":
        return bool(name)
    if k == "S":
        return name in STATEMENTS
    return False


STATEMENTS = set()
for _f in (ROOT / "ontology").glob("[0-9][0-9]-*.md"):
    STATEMENTS |= set(re.findall(r"^\| ([A-Z]+-(?:[BDN]\d+|0)) \|", _f.read_text(), re.M))

# What each verdict word requires of its evidence (review REVIEW-2 N3).
VERDICT_NEEDS = {
    "preserved": ("P", "M"),
    "finding": ("F",),
    "named exception": ("S", "DEC"),
    "by design": ("DEC", "S", "T"),
    "observed": ("O",),
    "refusal": ("O",),
    "guarded": ("GUARD",),
}


def verdict_problem(entry):
    v = entry["verdict"].strip().lower()
    word = next((w for w in VERDICT_NEEDS if v.startswith(w)), None)
    if word is None:
        return f"verdict must start with one of {sorted(VERDICT_NEEDS)}"
    kinds = {e.partition(":")[0] for e in entry["evidence"]}
    if not kinds & set(VERDICT_NEEDS[word]):
        return f"verdict '{word}' needs evidence of kind {VERDICT_NEEDS[word]}"
    return None


def main(argv):
    idx = evidence_index()
    errors = []
    obs = {r["name"]: r for r in json.loads((ROOT / "evidence/iris-observations.json").read_text())["records"]}
    ws = {n[3:]: [g for g in r["values"]["written"].split(",") if g] for n, r in obs.items() if n.startswith("ws-") and "written" in r["values"]}
    cls_of = {}
    for c, globs in S["global_classes"].items():
        for g in globs:
            cls_of[g] = c
    for cmd, gl in ws.items():
        for g in gl:
            if g.replace("(killed)", "") not in cls_of:
                errors.append(f"unclassified global {g} written by {cmd}")
    # module declarations
    for f in sorted((ROOT / "ontology").glob("[0-9][0-9]-*.md")):
        decl = S["modules"].get(f.stem)
        if decl is None:
            errors.append(f"{f.stem}: no semantics declaration")
            continue
        for m in decl["mechanisms"]:
            if m not in S["mechanisms"]:
                errors.append(f"{f.stem}: unknown mechanism {m}")
        for a in decl["axioms"]:
            if a not in S["axioms"]:
                errors.append(f"{f.stem}: unknown axiom {a}")
    # pairs
    rows = []
    for inv, spec in S["invariants"].items():
        reads = set(spec["reads"])
        for cmd, gl in sorted(ws.items()):
            touched = sorted({cls_of.get(g.replace("(killed)", ""), "?") for g in gl} & reads)
            if spec.get("must_not_write"):
                if cmd in spec["commands"]:
                    status = "VIOLATED" if touched else "holds (measured)"
                    if touched:
                        errors.append(f"{inv}: read-only command {cmd} writes {touched}")
                    rows.append((inv, cmd, touched, status, ["O:ws-" + cmd]))
                continue
            if not touched:
                continue
            entry = COV.get(inv, {}).get(cmd)
            if obs["ws-" + cmd]["values"].get("status") == "err" and not ((entry or {}).get("refusal") or (entry or {}).get("failure_run")):
                errors.append(f"({inv}, {cmd}): the measured command failed; label the entry as a refusal or remeasure a successful run")
            if entry is None:
                errors.append(f"uncovered pair ({inv}, {cmd}) touching {touched}")
                rows.append((inv, cmd, touched, "UNCOVERED", []))
                continue
            vp = verdict_problem(entry)
            if vp:
                errors.append(f"({inv}, {cmd}): {vp}")
            bad = [r for r in entry["evidence"] if not resolve(r, idx)]
            if bad:
                errors.append(f"({inv}, {cmd}): evidence does not resolve: {bad}")
            rows.append((inv, cmd, touched, entry["verdict"], entry["evidence"]))
    out = ["# Write-set coverage (generated by scripts/semantics_gate.py)", "",
           f"Gate: **{'PASS' if not errors else 'FAIL'}** ({len(errors)} problems). Write sets measured on IRIS 2026.1 "
           f"for {len(ws)} commands (observations `ws-*`, log validation/logs/50-write-sets.log).", "",
           "Verdicts: *preserved* (a proof or passing model check covers it, conditions in the entry), *finding* (the pair "
           "violates the invariant; the finding records it), *guarded* (a code guard prevents the violation), "
           "*by design* (the command's purpose is to change that fact; an open decision governs it), "
           "*observed* (only IRIS observations; no proof).", "",
           "| Invariant | Command | Classes written that it reads | Verdict | Evidence |", "|---|---|---|---|---|"]
    for inv, cmd, touched, status, ev in rows:
        out.append(f"| {inv} | {cmd} | {', '.join(touched) or '-'} | {status} | {'; '.join(ev)} |")
    out += ["", "## Measured write sets", "", "| Command | Globals written (database:global) |", "|---|---|"]
    for cmd, gl in sorted(ws.items()):
        ipm = [g for g in gl if cls_of.get(g.replace("(killed)", "")) != "SystemBookkeeping"]
        out.append(f"| {cmd} | {', '.join(ipm) or '(system bookkeeping only)'} |")
    out += ["", "## Problems", ""] + ([f"- {e}" for e in errors] or ["None."]) + [""]
    text = "\n".join(out) + "\n"
    target = ROOT / "semantics/COVERAGE.md"
    if "--check" in argv:
        if not target.exists() or target.read_text() != text:
            errors.append("COVERAGE.md is stale")
    else:
        target.write_text(text)
    for e in errors:
        print("PROBLEM:", e)
    print(f"{len(rows)} pairs, {len(errors)} problems")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
