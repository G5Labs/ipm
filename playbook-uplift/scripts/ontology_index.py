#!/usr/bin/env python3
"""Build ontology/INDEX.md from ontology/NN-*.md and enforce the playbook 3.1 gate.

Usage (from the experiment root):
  python3 -I scripts/ontology_index.py            # write INDEX.md, exit 1 on violations
  python3 -I scripts/ontology_index.py --check    # do not write; exit 1 if INDEX.md is stale or on violations
  python3 -I scripts/ontology_index.py --self-test

The format is defined in ontology/FORMAT.md.
"""
from __future__ import annotations

import json
import re
import sys
from collections import Counter, OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ONT = ROOT / "ontology"
EVID = ROOT / "evidence"
DECISIONS = ROOT / "decisions"
FINDINGS = ROOT / "findings"
MAX_GRADE = 8.0
KINDS = {"R", "D", "B", "N"}
EVIDENCE_KINDS = ["P", "M", "O", "O!", "T", "X", "I", "N", "F", "OPEN"]
VERUS_DIR_NAME = "models/verus"


def lemma_premises(proofs_files):
    """{lemma: (labels required, file)} from `// PREMISE <spec fn> = <LABEL>` declarations and each lemma's requires.
    Premise predicates reached through wrapper spec functions count too (transitive closure over spec fn bodies)."""
    out, declared, label_files = {}, {}, {}
    for f in proofs_files:
        text = f.read_text()
        decl = dict(re.findall(r"^// PREMISE (\w+) = ([A-Za-z0-9'\-]+)", text, re.M))
        for lab in decl.values():
            label_files.setdefault(lab, set()).add(f.name)
        declared[f.name] = bool(decl) or bool(re.search(r"^// PREMISES: none", text, re.M))
        bodies = {}
        for m in re.finditer(r"spec fn (\w+)\s*\(.*?\)\s*->\s*\w+\s*(\{.*?\n\})", text, re.S):
            bodies[m.group(1)] = m.group(2)

        def reach(txt, seen):
            found = set()
            for fn in re.findall(r"\b(\w+)\s*\(", txt):
                if fn in decl:
                    found.add(fn)
                elif fn in bodies and fn not in seen:
                    seen.add(fn)
                    found |= reach(bodies[fn], seen)
            return found
        for m in re.finditer(r"proof fn (\w+)\s*\((.*?)\)\s*(requires(.*?))?(ensures|\{)", text, re.S):
            req = m.group(4) or ""
            labels = sorted({decl[fn] for fn in reach(req, set())})
            out[m.group(1)] = (labels, f.name)
    return out, declared, label_files


# ---------------------------------------------------------------- readability
def syllables(word: str) -> int:
    w = re.sub(r"[^a-z]", "", word.lower())
    if not w:
        return 0
    if len(w) <= 3:
        return 1
    w = re.sub(r"([aeiouy])les?$", r"\1l", w)  # module(s), file(s): silent e after vowel+l
    w = re.sub(r"(?:[^laeiouy]es|ed|[^laeiouy]e)$", "", w)
    w = re.sub(r"^y", "", w)
    groups = re.findall(r"[aeiouy]{1,2}", w)
    return max(1, len(groups))


def clean_statement(text: str) -> str:
    text = text.replace("**", "").replace("`", "")
    return text


def fk_grade(text: str) -> float:
    text = clean_statement(text)
    sentences = [s for s in re.split(r"[.!?]+(?:\s|$)", text) if re.search(r"[A-Za-z0-9]", s)]
    words = re.findall(r"[A-Za-z0-9][A-Za-z0-9'\-]*", text)
    if not words:
        return 0.0
    n_sent = max(1, len(sentences))
    syl = 0
    for w in words:
        parts = [p for p in w.split("-") if p]
        syl += sum(syllables(p) if not p.isdigit() else 1 for p in parts) or 1
    return 0.39 * (len(words) / n_sent) + 11.8 * (syl / len(words)) - 15.59


# ---------------------------------------------------------------- parsing
def split_row(line: str) -> list[str]:
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|"):
        line = line[:-1]
    return [c.strip() for c in line.split("|")]


def parse_tables(text: str) -> "OrderedDict[str, list[dict]]":
    """Return {section heading: [row dicts]} for the first table under each '## ' heading."""
    out: "OrderedDict[str, list[dict]]" = OrderedDict()
    section = None
    header = None
    for raw in text.splitlines():
        if raw.startswith("## "):
            section = raw[3:].strip()
            header = None
            continue
        if section is None or not raw.strip().startswith("|"):
            if raw.strip() == "":
                pass
            continue
        cells = split_row(raw)
        if header is None:
            header = cells
            out.setdefault(section, [])
            continue
        if all(re.fullmatch(r":?-{3,}:?", c) for c in cells if c):
            continue
        if len(cells) != len(header):
            out.setdefault("__errors__", []).append({"line": raw, "section": section})
            continue
        out[section].append(dict(zip(header, cells)))
    return out


class Module:
    def __init__(self, path: Path):
        self.path = path
        self.name = path.stem
        text = path.read_text()
        m = re.search(r"^# (.+)$", text, re.M)
        self.title = m.group(1).strip() if m else self.name
        m = re.search(r"^Module root:\s*(\S+)", text, re.M)
        self.root_id = m.group(1) if m else None
        self.tables = parse_tables(text)
        self.glossary = {r["Term"].strip().lower(): r["Defined by"].strip() for r in self.tables.get("Glossary", [])}
        self.statements = self.tables.get("Statements", [])
        self.states = self.tables.get("States", [])
        self.commands = self.tables.get("Commands", [])
        self.cells = self.tables.get("Cells", [])
        self.bad_rows = self.tables.get("__errors__", [])


def load_json_names(path: Path, ok_field: str | None = None) -> dict:
    if not path.exists():
        return {}
    data = json.loads(path.read_text())
    out = {}
    for rec in data.get("records", []):
        out[rec["name"]] = rec
    return out


def load_decisions() -> dict:
    out = {}
    for f in sorted(DECISIONS.glob("*.md")) if DECISIONS.exists() else []:
        for sec, recs in parse_tables(f.read_text()).items():
            for r in recs:
                if "ID" in r and r["ID"].startswith("DEC-"):
                    out[r["ID"]] = r
    return out


def load_findings() -> set:
    out = set()
    for f in sorted(FINDINGS.glob("*.md")) if FINDINGS.exists() else []:
        out |= set(re.findall(r"^#+\s*(F-[A-Z0-9]+-\d+)", f.read_text(), re.M))
    return out


def term_regex(term: str) -> re.Pattern:
    words = [re.escape(w) for w in term.split()]
    body = r"\s+".join(words)
    return re.compile(r"(?<![A-Za-z0-9-])" + body + r"(?:s|es)?(?![A-Za-z0-9])", re.I)


def build(ont_dir: Path = ONT):
    modules = [Module(p) for p in sorted(ont_dir.glob("[0-9][0-9]-*.md"))]
    violations: list[str] = []
    by_id: dict[str, dict] = {}
    order: dict[str, int] = {}
    mod_of: dict[str, Module] = {}
    idx = 0
    for mod in modules:
        for r in mod.bad_rows:
            violations.append(f"{mod.name}: malformed table row in '{r['section']}': {r['line'][:80]}")
        for st in mod.statements:
            sid = st.get("ID", "")
            if not sid:
                violations.append(f"{mod.name}: statement without ID")
                continue
            if sid in by_id:
                violations.append(f"{sid}: duplicate ID")
            by_id[sid] = st
            order[sid] = idx
            mod_of[sid] = mod
            idx += 1

    lemmas, declared, label_files = lemma_premises(sorted((ROOT / VERUS_DIR_NAME).glob("*.rs")))
    proofs, proof_files, dup_proofs = {}, {}, []
    for f in sorted(EVID.glob("proofs-*.json")):
        d = json.loads(f.read_text())
        for rec in d.get("records", []):
            if rec["name"] in proofs:
                dup_proofs.append(rec["name"])
            proofs[rec["name"]] = dict(rec, controls_all_killed=d.get("controls_all_killed"))
    checks = {}
    for f in sorted(EVID.glob("model-checks*.json")):
        checks.update(load_json_names(f))
    obs = load_json_names(EVID / "iris-observations.json")
    tests = load_json_names(EVID / "ipm-tests.json")
    decisions = load_decisions()
    findings = load_findings()

    # global glossary from 00-system
    sys_gloss = {}
    for mod in modules:
        if mod.name.startswith("00-"):
            sys_gloss = dict(mod.glossary)

    def ancestors(sid: str) -> list[str]:
        chain, seen = [], set()
        cur = by_id.get(sid, {}).get("Parent")
        while cur and cur in by_id and cur not in seen:
            chain.append(cur)
            seen.add(cur)
            cur = by_id[cur].get("Parent")
        return chain

    children: dict[str, list[str]] = {}
    for sid, st in by_id.items():
        children.setdefault(st.get("Parent", ""), []).append(sid)

    def in_scope(sid: str, definer: str) -> bool:
        if definer == sid:
            return True
        chain = ancestors(sid)
        if definer in chain:
            return True
        if definer not in by_id:
            return False
        dparent = by_id[definer].get("Parent")
        # earlier sibling of the statement or of any ancestor (lexical scope)
        for node in [sid] + chain:
            if by_id.get(node, {}).get("Parent") == dparent and order[definer] < order[node]:
                return True
        return False

    per_stmt = {}
    for sid, st in by_id.items():
        mod = mod_of[sid]
        kind = st.get("Kind", "")
        parent = st.get("Parent", "")
        if kind not in KINDS:
            violations.append(f"{sid}: unknown kind '{kind}'")
        if kind == "R":
            if sid != "SYS-0" and parent != "SYS-0":
                violations.append(f"{sid}: root must have parent SYS-0")
        elif parent not in by_id:
            violations.append(f"{sid}: parent '{parent}' does not resolve")
        elif parent == "SYS-0" and not mod.name.startswith("00-"):
            violations.append(f"{sid}: only module roots may hang from SYS-0")
        text = st.get("Statement", "")
        g = fk_grade(text)
        if g > MAX_GRADE:
            violations.append(f"{sid}: reading grade {g:.2f} > {MAX_GRADE}: {clean_statement(text)[:90]}")
        gloss = dict(sys_gloss)
        gloss.update(mod.glossary)
        for term, definer in gloss.items():
            if definer not in by_id:
                continue
            if term_regex(term).search(clean_statement(text)) and not in_scope(sid, definer):
                violations.append(f"{sid}: uses '{term}' before its definition {definer} is in scope")
        tags = [t.strip() for t in st.get("Evidence", "").split(";") if t.strip() and t.strip() not in {"—", "-"}]
        kinds_here = []
        for t in tags:
            k, _, ref = t.partition(":")
            k = k.strip()
            ref = ref.strip()
            if k not in EVIDENCE_KINDS:
                violations.append(f"{sid}: unknown evidence tag '{t}'")
                continue
            kinds_here.append(k)
            if k == "P" and ref in lemmas:
                labels, fname = lemmas[ref]
                if not declared.get(fname):
                    violations.append(f"{sid}: proof file {fname} declares no premises (// PREMISE fn = LABEL or // PREMISES: none)")
                notes = st.get("Notes", "")
                missing = [lab for lab in labels if not re.search(r"(?<![A-Za-z0-9])" + re.escape(lab) + r"(?![A-Za-z0-9])", notes)]
                if missing:
                    violations.append(f"{sid}: P:{ref} requires premises {missing} not named in Notes")
                ambiguous = [lab for lab in labels if len(label_files.get(lab, ())) > 1 and fname.replace(".rs", "") not in notes]
                if ambiguous:
                    violations.append(f"{sid}: premise labels {ambiguous} are declared in several proof files; name {fname} in Notes")
            if k == "O!" and ref not in obs:
                violations.append(f"{sid}: IRIS observation '{ref}' not recorded")
            if k == "P" and (ref not in proofs or proofs[ref].get("status") != "verified"):
                violations.append(f"{sid}: proof '{ref}' not recorded as verified")
            elif k == "P" and proofs[ref].get("controls_all_killed") is not True:
                violations.append(f"{sid}: proof '{ref}' is in a file whose controls are missing or not all killed")
            if k == "M" and (ref not in checks or checks[ref].get("status") != "pass"):
                violations.append(f"{sid}: model check '{ref}' not recorded as pass")
            if k == "O" and ref not in obs:
                violations.append(f"{sid}: IRIS observation '{ref}' not recorded")
            if k == "T" and (ref not in tests or tests[ref].get("status") != "pass"):
                violations.append(f"{sid}: IPM test '{ref}' not recorded as passing")
            if k == "X" and not ref:
                violations.append(f"{sid}: X tag needs a test anchor")
            if k == "OPEN" and (ref not in decisions or decisions[ref].get("Status", "").lower() != "open"):
                violations.append(f"{sid}: decision '{ref}' not found as open")
            if k == "F" and ref not in findings:
                violations.append(f"{sid}: finding '{ref}' not in findings.md")
        if "P" in kinds_here and "F" in kinds_here:
            violations.append(f"{sid}: P and F on one statement; a proof of the defect belongs in the finding, a proof under a premise that excludes the defect is not proof of the statement")
        if "P" in kinds_here and "OPEN" in kinds_here and "intent open" not in st.get("Notes", "").lower():
            violations.append(f"{sid}: P with OPEN: say 'intent open' in Notes (the proof states the current behaviour, not a decided intent)")
        if "O!" in kinds_here and not ({"F", "OPEN"} & set(kinds_here)):
            violations.append(f"{sid}: refuting observation (O!) without F or OPEN")
        if kind in {"B", "N"} and not kinds_here:
            violations.append(f"{sid}: behaviour has no evidence tag")
        per_stmt[sid] = {"module": mod.name, "kind": kind, "grade": g, "evidence": sorted(set(kinds_here))}

    for name in sorted(set(dup_proofs)):
        violations.append(f"proof function name '{name}' appears in two proof files; P tags would be ambiguous")

    # glossary definers must be D statements of a visible module
    for mod in modules:
        for term, definer in mod.glossary.items():
            if definer not in by_id:
                violations.append(f"{mod.name}: glossary term '{term}' defined by unknown '{definer}'")
            elif by_id[definer].get("Kind") not in {"D", "R"}:
                violations.append(f"{mod.name}: glossary term '{term}' definer {definer} is not a D statement")
            elif term.lower() not in clean_statement(by_id[definer].get("Statement", "")).lower():
                violations.append(f"{mod.name}: definer {definer} does not mention '{term}'")

    # module roots
    for mod in modules:
        if mod.root_id is None or mod.root_id not in by_id:
            violations.append(f"{mod.name}: missing or unknown module root")

    # cells: complete cross product, each defined or decided
    cell_stats = {}
    for mod in modules:
        states = [s["ID"] for s in mod.states]
        cmds = [c["ID"] for c in mod.commands]
        seen = Counter()
        open_cells = 0
        for c in mod.cells:
            cid = c.get("Cell", "")
            pair = (c.get("State", ""), c.get("Command", ""))
            seen[pair] += 1
            if pair[0] not in states or pair[1] not in cmds:
                violations.append(f"{cid}: unknown state/command {pair}")
            outs = [o.strip() for o in c.get("Outcome", "").split(",") if o.strip()]
            dec = c.get("Decision", "").strip()
            for o in outs:
                if o not in by_id:
                    violations.append(f"{cid}: outcome '{o}' does not resolve")
                else:
                    opens = [x.strip()[5:] for x in by_id[o].get("Evidence", "").split(";") if x.strip().startswith("OPEN:")]
                    if opens and not c.get("Decision", "").strip():
                        violations.append(f"{cid}: outcome {o} waits on {opens[0]}; name the decision in the cell")
            if dec:
                if dec not in decisions or decisions[dec].get("Status", "").lower() != "open":
                    violations.append(f"{cid}: decision '{dec}' not found as open")
                open_cells += 1
            if not outs and not dec:
                violations.append(f"{cid}: cell neither defined nor assigned to a decision")
        for s in states:
            for k in cmds:
                if seen[(s, k)] == 0:
                    violations.append(f"{mod.name}: cell ({s}, {k}) missing")
                elif seen[(s, k)] > 1:
                    violations.append(f"{mod.name}: cell ({s}, {k}) listed {seen[(s, k)]} times")
        cell_stats[mod.name] = (len(states), len(cmds), len(mod.cells), open_cells)

    return modules, by_id, per_stmt, cell_stats, decisions, violations


def render(modules, by_id, per_stmt, cell_stats, decisions, violations) -> str:
    L = []
    L.append("# Ontology index (generated)")
    L.append("")
    L.append("Generated by `scripts/ontology_index.py` from `ontology/NN-*.md`. Do not edit by hand.")
    L.append("")
    L.append(f"Gate: **{'PASS' if not violations else 'FAIL'}** ({len(violations)} violations).")
    L.append("")
    L.append("## Counts per module")
    L.append("")
    head = ["Module", "Statements", "D", "B", "N"] + EVIDENCE_KINDS + ["States", "Commands", "Cells", "Cells on decision"]
    L.append("| " + " | ".join(head) + " |")
    L.append("|" + "---|" * len(head))
    tot = Counter()
    for mod in modules:
        ids = [s for s, v in per_stmt.items() if v["module"] == mod.name]
        kinds = Counter(per_stmt[s]["kind"] for s in ids)
        ev = Counter(k for s in ids for k in per_stmt[s]["evidence"])
        ns, nk, nc, no = cell_stats.get(mod.name, (0, 0, 0, 0))
        row = [mod.title, len(ids), kinds["D"], kinds["B"], kinds["N"]] + [ev[k] for k in EVIDENCE_KINDS] + [ns, nk, nc, no]
        for h, v in zip(head[1:], row[1:]):
            tot[h] += v
        L.append("| " + " | ".join(str(x) for x in row) + " |")
    L.append("| **Total** | " + " | ".join(str(tot[h]) for h in head[1:]) + " |")
    L.append("")
    L.append("Evidence key: P proved (Verus), M model-checked (explorer), O observed on IRIS, T IPM test run here, "
             "X IPM test present but not run here, I inspection, N not implemented, F contradicted (finding), OPEN human decision.")
    L.append("")
    L.append("## Evidence combinations (behaviour and exception statements)")
    L.append("")
    combos = Counter("+".join(v["evidence"]) or "(none)" for v in per_stmt.values() if v["kind"] in {"B", "N"})
    L.append("| Combination | Statements |")
    L.append("|---|---|")
    for c, n in sorted(combos.items(), key=lambda x: (-x[1], x[0])):
        L.append(f"| {c} | {n} |")
    L.append("")

    def listing(title, pred):
        L.append(f"## {title}")
        L.append("")
        rows = [s for s, v in per_stmt.items() if pred(v)]
        if not rows:
            L.append("None.")
        for s in rows:
            L.append(f"- `{s}` ({per_stmt[s]['module']}): {clean_statement(by_id[s]['Statement'])}")
        L.append("")

    L.append("## T tags on test runs with skipped assertions")
    L.append("")
    tests_all = json.loads((EVID / "ipm-tests.json").read_text())["records"] if (EVID / "ipm-tests.json").exists() else []
    skipped = {r["name"]: r.get("skipped_assertions") for r in tests_all if r.get("skipped_assertions") not in (0, None)}
    hits = []
    for s2, st2 in by_id.items():
        for tg in [x.strip() for x in st2.get("Evidence", "").split(";")]:
            if tg.startswith("T:") and tg[2:] in skipped:
                hits.append(f"- `{s2}` cites `{tg[2:]}`: skipped assertions = {skipped[tg[2:]]}")
    L += (hits or ["None."]) + [""]
    listing("Proof-only statements", lambda v: v["evidence"] == ["P"])
    listing("Proved statements", lambda v: "P" in v["evidence"])
    listing("Observed on IRIS", lambda v: "O" in v["evidence"] or "T" in v["evidence"])
    listing("Not implemented", lambda v: "N" in v["evidence"])
    listing("Contradicted by the code (findings)", lambda v: "F" in v["evidence"])
    listing("Waiting on a human decision", lambda v: "OPEN" in v["evidence"])
    L.append("## Open decisions")
    L.append("")
    opened = [(k, v) for k, v in decisions.items() if v.get("Status", "").lower() == "open"]
    for k, v in opened:
        L.append(f"- `{k}` ({v.get('Module', '')}): {v.get('Question', '')}")
    if not opened:
        L.append("None.")
    L.append("")
    oos = ONT / "OUT-OF-SCOPE.md"
    if oos.exists():
        L.append("## Named exclusions (out of scope)")
        L.append("")
        L += [l for l in oos.read_text().splitlines()[3:]]
        L.append("")
    L.append("## Gate violations")
    L.append("")
    if violations:
        for v in violations:
            L.append(f"- {v}")
    else:
        L.append("None.")
    L.append("")
    return "\n".join(L)


def self_test() -> int:
    """Inject one violation of each kind into a temp copy and require the gate to catch it."""
    import shutil
    import tempfile
    global ONT, EVID, DECISIONS, FINDINGS
    failures = 0
    cases = {
        "parent": lambda t: t.replace("| SYS-D1 | SYS-0 | D |", "| SYS-D1 | SYS-NOPE | D |", 1),
        "grade": lambda t: t.replace("| SYS-D1 | SYS-0 | D | ", "| SYS-D1 | SYS-0 | D | Notwithstanding extraordinarily sophisticated institutional considerations, interoperability necessitates comprehensive organizational standardization. ", 1),
        "definition-order": None,
        "cell-missing": None,
        "p-with-f": None,
        "premise-not-named": None,
    }
    tmp = Path(tempfile.mkdtemp())
    *_, base_viol = build(ONT)
    base = set(base_viol)
    try:
        for name, mut in cases.items():
            work = tmp / name
            shutil.copytree(ROOT / "ontology", work / "ontology")
            for sub in ["evidence", "decisions", "findings"]:
                if (ROOT / sub).exists():
                    shutil.copytree(ROOT / sub, work / sub)
            sysf = work / "ontology" / "00-system.md"
            t = sysf.read_text()
            if name == "definition-order":
                # use a glossary term in SYS-0 itself before it is defined
                term = next(iter(Module(sysf).glossary))
                t = re.sub(r"(\| SYS-0 \| SYS-0 \| R \| )", r"\1The " + term + " matters. ", t, count=1)
            elif name == "p-with-f":
                f = sorted((work / "ontology").glob("02-*.md"))[0]
                tt = f.read_text()
                tt = re.sub(r"(\| HIST-B19 \| [^|]*\| [^|]*\| [^|]*\| )", r"\1P:g_refines; ", tt, count=1)
                f.write_text(tt)
            elif name == "premise-not-named":
                f = sorted((work / "ontology").glob("02-*.md"))[0]
                tt = f.read_text()
                tt = re.sub(r"(\| HIST-B8 \|.*?)P1, P2, P3, PC, PI", r"\1(premises removed)", tt, count=1)
                f.write_text(tt)
            elif name == "cell-missing":
                f = sorted((work / "ontology").glob("01-*.md"))[0]
                lines = f.read_text().splitlines()
                idx = max(i for i, l in enumerate(lines) if re.match(r"\|\s*[A-Z]+-C\d+", l))
                del lines[idx]
                f.write_text("\n".join(lines) + "\n")
            else:
                t = mut(t)
            sysf.write_text(t)
            saved = (ONT, EVID, DECISIONS, FINDINGS)
            ONT, EVID, DECISIONS, FINDINGS = work / "ontology", work / "evidence", work / "decisions", work / "findings"
            try:
                *_, viol = build(ONT)
            finally:
                ONT, EVID, DECISIONS, FINDINGS = saved
            new = [v for v in viol if v not in base]
            caught = len(new) > 0
            print(f"self-test {name}: {'caught' if caught else 'MISSED'} ({len(new)} new violations beyond the {len(base)} of the unmutated tree)")
            failures += 0 if caught else 1
    finally:
        shutil.rmtree(tmp)
    return failures


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return 1 if self_test() else 0
    result = build()
    text = render(*result)
    violations = result[-1]
    target = ONT / "INDEX.md"
    if "--check" in argv:
        stale = (not target.exists()) or target.read_text() != text
        if stale:
            print("INDEX.md is stale; regenerate it")
        for v in violations:
            print("VIOLATION:", v)
        return 1 if (stale or violations) else 0
    target.write_text(text)
    for v in violations:
        print("VIOLATION:", v)
    print(f"wrote {target.relative_to(ROOT)}: {len(result[2])} statements, {len(violations)} violations")
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
