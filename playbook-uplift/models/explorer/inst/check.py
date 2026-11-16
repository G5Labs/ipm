#!/usr/bin/env python3
"""Model checks for modules 03 (update steps) and 04 (dependency resolution).

  python3 -I models/explorer/inst/check.py        (from the experiment root)

Writes evidence/model-checks-inst.json (records: name, status pass/fail, detail, conditions). Parts:
 A. Conformance: every nc-inst-* / nc-dep-* IRIS observation the models can express, replayed on the twin (literal
    transcription of the Verus spec) and on the fine model (written from the ObjectScript); unexpressed ones are listed.
 B. Exhaustive exploration of the twin within stated bounds: every transition is checked against L1 (update steps) or
    every resolution against L1 soundness and completeness (resolver).
 C. Transcription: the twin and the fine model agree on every explored configuration.
"""
from __future__ import annotations

import itertools
import json
import re
import sys
import time
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
IPM = ROOT.parents[2] / "ipm"
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import fine_resolve as FR  # noqa: E402
import fine_update as FU  # noqa: E402
import twin_resolve as TR  # noqa: E402
import twin_update as TU  # noqa: E402
from irisenv import Env  # noqa: E402

OBS = {r["name"]: r["values"] for r in json.loads((ROOT / "evidence" / "iris-observations.json").read_text())["records"]}
DATA = IPM / "tests/integration_tests/Test/PM/Integration/_data"
FIX = ROOT / "validation/iris/inst/fixtures"

# =============================================================================== fixtures

METH_RE = re.compile(r"ClassMethod (GetOrderedMethods|GetOrderedMethodsPrimaryOnly)\(\)[^{]*\{\s*return \[(.*?)\]", re.S)
BODY_RE = re.compile(r"ClassMethod (\w+)\(\)[^{]*\{(.*?)\n\}", re.S)


def parse_package(directory: Path):
    """Classes of an update package directory in query row order (by name): (name, any or None, prim or None), failing methods."""
    rows, failing = [], set()
    for f in sorted(directory.glob("*.cls"), key=lambda p: p.stem):
        text = f.read_text()
        lists = {k: [m.strip().strip('"') for m in v.split(",") if m.strip()] for k, v in METH_RE.findall(text)}
        rows.append((f.stem, lists.get("GetOrderedMethods"), lists.get("GetOrderedMethodsPrimaryOnly")))
        for meth, body in BODY_RE.findall(text):
            if "$$$ThrowOnError($$$ERROR" in body:
                failing.add((f.stem, meth))
    return rows, failing


def vnum(s: str) -> int:
    core = s.split("+")[0].split("-")[0]
    a, b, c = (int(x) for x in core.split("."))
    return a * 1_000_000 + b * 1000 + c


def vstr(n: int) -> str:
    return f"{n // 1_000_000}.{n // 1000 % 1000}.{n % 1000}"


# =============================================================================== A1. update-step conformance

def flat_twin_steps(rows):
    """Twin side of the class list: PkgClass rows -> class_list -> walk order (any then primary-only)."""
    pcs = []
    for i, (name, anyl, pril) in enumerate(rows):
        digits = FU.zstrip_digits(name)
        pcs.append(TU.PkgClass(int(digits) if digits else None, anyl is not None or pril is not None, i))
    m = TU.class_list(pcs)
    if m is None:
        return None
    steps = []
    for num in sorted(m):
        name, anyl, pril = rows[m[num].id]
        steps += [(num, False, x) for x in (anyl or [])]
        steps += [(num, True, x) for x in (pril or [])]
    return tuple(steps)


class Namespace:
    """One module's state in both models across commands."""

    def __init__(self):
        self.tdb = TU.Db(None, False, {}, ())
        self.truns = []
        self.env = Env()
        self.fruns = []

    def cmd(self, kind, ver, rows, failing=(), **kw):
        to = vnum(ver)
        # twin
        steps = flat_twin_steps(rows) if rows is not None else ()
        if steps is None:
            tstat = None  # class list fails: not a step of the Verus L2 (class_list witness)
        else:
            op = TU.Op(kind, kw.get("force", False), kw.get("dev"), kw.get("txn", True), kw.get("top", True),
                       kw.get("source", "Registry"), kw.get("snapshot", False), to, rows is not None, False)
            fail_keys = {(int(FU.zstrip_digits(c)), prim, m) for (c, m) in failing for prim in (False, True)}
            s = drive_twin(TU.start(self.tdb, op, steps), fail_keys, kw.get("lock_free", True))
            self.tdb = TU.abs_(s)["d"]
            self.truns += list(s["log"])
            tstat = "ok" if s["out"] == "Ok" else "err"
        # fine
        ctx = FU.Ctx(kind, to, rows, force=kw.get("force", False), dev=kw.get("dev"), txn=kw.get("txn", True),
                     top=kw.get("top", True), source=kw.get("source", "Registry"), snapshot=kw.get("snapshot", False),
                     lock_free=kw.get("lock_free", True),
                     fail_steps={(prim, c, m) for (c, m) in failing for prim in (False, True)})
        fstat = FU.load_new_module(self.env, ctx, self.fruns)
        return tstat, ("ok" if fstat == "Ok" else "err")

    # observable state
    def twin_view(self, prefix=""):
        out = {}
        d = self.tdb
        if d["ver"] is not None:
            out[prefix + "version"] = vstr(d["ver"]) + ("/dev" if d["dev"] else "")
        n = 0
        for (c, prim, m), r in d["recs"].items():
            out[f"{prefix}{'pri' if prim else 'any'}:V{c}.{m}"] = f"start:{int(r.started)}/end:{int(r.end)}/ok:{int(r.ok)}"
            n += 1
        out[prefix + "stepRows"] = str(n)
        return out

    def fine_view(self, prefix=""):
        out = {}
        if ("mod", "ver") in self.env.db:
            out[prefix + "version"] = vstr(self.env.db[("mod", "ver")]) + ("/dev" if self.env.db.get(("mod", "dev")) else "")
        recs = FU.records(self.env)
        for (c, prim, m), (st, en, ok) in recs.items():
            out[f"{prefix}{'pri' if prim else 'any'}:{c}.{m}"] = f"start:{int(st)}/end:{int(en)}/ok:{int(ok)}"
        out[prefix + "stepRows"] = str(len(recs))
        return out


def drive_twin(s, fail_keys, lock_free=True, phases_ok=True):
    s = TU.step(s, ("Init",))
    while s["pc"] not in ("Closed", "Crashed"):
        pc = s["pc"]
        if pc == "Walk":
            k = s["steps"][s["i"]] if s["i"] < len(s["steps"]) else None
            c = ("Walk", k not in fail_keys)
        else:
            c = {"Guard": ("Guard",), "Manifest": ("Manifest",), "Phases": ("Phases", phases_ok), "Lock": ("Lock", lock_free)}[pc]
        s = TU.step(s, c)
    return s


def simple(major):
    d = DATA / f"update-test/update-modules-{major}.x/update-simple/cls/UpdatePackage"
    return parse_package(d)


def step_errors(major):
    return parse_package(DATA / f"update-test/update-modules-{major}.x/update-step-errors/cls/UpdateStepErrors/UpdatePackage")


def mfix(v):
    return parse_package(FIX / "m" / v / "src/NcInstM/Upd")


def ordfix(v):
    return parse_package(FIX / "order" / v / "src/NcInstOrd/Upd")


def update_scenarios():
    """Each scenario returns (twin prediction, fine prediction) as dicts of observation keys."""
    S = {}

    def both(ns, kv_t, kv_f, label, res):
        kv_t[label], kv_f[label] = res

    def nc1():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "install211", ns.cmd("Install", "2.1.1", *simple(2)))
        both(ns, t, f, "plainInstall350", ns.cmd("Install", "3.5.0", *simple(3)))
        both(ns, t, f, "forceInstall121", ns.cmd("Install", "1.2.1", *simple(1), force=True))
        both(ns, t, f, "forceInstall350", ns.cmd("Install", "3.5.0", *simple(3), force=True))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        return t, f
    S["nc-inst-1-force-install-seeds"] = nc1

    def nc2():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "install211", ns.cmd("Install", "2.1.1", *simple(2)))
        both(ns, t, f, "devInstall350", ns.cmd("Install", "3.5.0", *simple(3), dev=True))
        tv, fv = ns.twin_view("afterDev-"), ns.fine_view("afterDev-")
        tv["versionAfterDev"], fv["versionAfterDev"] = tv.pop("afterDev-version"), fv.pop("afterDev-version")
        t.update(tv), f.update(fv)
        both(ns, t, f, "update350", ns.cmd("Update", "3.5.0", *simple(3)))
        tv, fv = ns.twin_view(), ns.fine_view()
        tv["versionAfterUpdate"], fv["versionAfterUpdate"] = tv.pop("version"), fv.pop("version")
        t.update(tv), f.update(fv)
        return t, f
    S["nc-inst-2-dev-install-then-update"] = nc2

    def nc2b():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "devInstall121", ns.cmd("Install", "1.2.1", *simple(1), dev=True))
        tv, fv = ns.twin_view("afterDev-"), ns.fine_view("afterDev-")
        tv.pop("afterDev-version"), fv.pop("afterDev-version")
        t.update(tv), f.update(fv)
        both(ns, t, f, "update211", ns.cmd("Update", "2.1.1", *simple(2)))
        tv, fv = ns.twin_view(), ns.fine_view()
        tv["versionAfterUpdate"], fv["versionAfterUpdate"] = tv.pop("version"), fv.pop("version")
        t.update(tv), f.update(fv)
        return t, f
    S["nc-inst-2b-dev-fresh-then-update"] = nc2b

    ns3 = {}

    def nc3():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "install121", ns.cmd("Install", "1.2.1", *simple(1)))
        both(ns, t, f, "update211", ns.cmd("Update", "2.1.1", *simple(2), lock_free=False))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        ns3["ns"] = ns
        return t, f
    S["nc-inst-3-update-lock-busy"] = nc3

    def nc3b():
        ns, t, f = ns3["ns"], {}, {}
        both(ns, t, f, "update211again", ns.cmd("Update", "2.1.1", *simple(2)))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        return t, f
    S["nc-inst-3b-update-after-lock-free"] = nc3b

    def mstate(ns, which):
        if which == "twin":
            jl, runs, recs, d = Counter(ns.tdb["jlog"]), Counter(ns.truns), ns.tdb["recs"], ns.tdb
            key = lambda c, m: (c, False, m)  # noqa: E731
            ver = vstr(d["ver"]) + ("/dev" if d["dev"] else "")
            get = lambda c, m: recs.get(key(c, m))  # noqa: E731
            fmt = lambda r: "none" if r is None else f"start:{int(r.started)}/end:{int(r.end)}/ok:{int(r.ok)}"  # noqa: E731
            a_eff, a_runs, s1_eff = jl[key(2, "A")], runs[key(2, "A")], jl[key(1, "S1")]
            v1, va, vb = fmt(get(1, "S1")), fmt(get(2, "A")), fmt(get(2, "B"))
        else:
            env = ns.env
            ver = vstr(env.db[("mod", "ver")]) + ("/dev" if env.db.get(("mod", "dev")) else "")
            recs = FU.records(env)
            fmt = lambda r: "none" if r is None else f"start:{int(r[0])}/end:{int(r[1])}/ok:{int(r[2])}"  # noqa: E731
            a_eff = env.db.get(("eff", "V2", False, "A"), 0)
            a_runs = env.ctr.get(("runs", "V2", False, "A"), 0)
            s1_eff = env.db.get(("eff", "V1", False, "S1"), 0)
            v1, va, vb = fmt(recs.get(("V1", False, "S1"))), fmt(recs.get(("V2", False, "A"))), fmt(recs.get(("V2", False, "B")))
        return {"version": ver, "Mx": str(a_eff) if a_eff else "undef", "My": str(a_runs) if a_runs else "undef",
                "fileLines": str(a_runs), "V1.S1runs": str(s1_eff), "V1.S1": v1, "V2.A": va, "V2.B": vb}

    ns4 = {}

    def nc4():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "load100", ns.cmd("Load", "1.0.0", *mfix("1.0.0"), source="Path"))
        both(ns, t, f, "updateBad", ns.cmd("Update", "2.0.0", *mfix("2.0.0-bad"), source="Path"))
        t.update(mstate(ns, "twin")), f.update(mstate(ns, "fine"))
        ns4["ns"] = ns
        return t, f
    S["nc-inst-4-nondev-failed-update"] = nc4

    def nc4b():
        ns, t, f = ns4["ns"], {}, {}
        both(ns, t, f, "updateGood", ns.cmd("Update", "2.0.0", *mfix("2.0.0-good"), source="Path"))
        t.update(mstate(ns, "twin")), f.update(mstate(ns, "fine"))
        return t, f
    S["nc-inst-4b-nondev-retry"] = nc4b

    def nc4c():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "devLoad100", ns.cmd("Load", "1.0.0", *mfix("1.0.0"), source="Path", dev=True))
        both(ns, t, f, "devUpdateBad", ns.cmd("Update", "2.0.0", *mfix("2.0.0-bad"), source="Path", dev=True))
        t.update(mstate(ns, "twin")), f.update(mstate(ns, "fine"))
        ns4["dev"] = ns
        return t, f
    S["nc-inst-4c-dev-failed-update"] = nc4c

    def nc4d():
        ns, t, f = ns4["dev"], {}, {}
        both(ns, t, f, "devUpdateGood", ns.cmd("Update", "2.0.0", *mfix("2.0.0-good"), source="Path", dev=True))
        t.update(mstate(ns, "twin")), f.update(mstate(ns, "fine"))
        return t, f
    S["nc-inst-4d-dev-retry"] = nc4d

    def nc4e():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "install121", ns.cmd("Install", "1.2.1", *simple(1)))
        both(ns, t, f, "devUpdate211", ns.cmd("Update", "2.1.1", *simple(2), dev=True))
        t["version"], f["version"] = ns.twin_view()["version"], ns.fine_view()["version"]
        return t, f
    S["nc-inst-4e-dev-update-from-registry"] = nc4e

    def nc5():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "install121", ns.cmd("Install", "1.2.1", *step_errors(1)))
        both(ns, t, f, "update211", ns.cmd("Update", "2.1.1", *step_errors(2)))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        return t, f
    S["nc-inst-5-failed-step-update"] = nc5

    def nc6():
        # only the dependency load of update-simple 1.2.1 under `update update-deps-added 2.1.1` is modelled
        ns, t, f = Namespace(), {}, {}
        ns.cmd("Update", "1.2.1", *simple(1), top=False)
        tv, fv = ns.twin_view(), ns.fine_view()
        tv["simpleVersion"], fv["simpleVersion"] = tv.pop("version"), fv.pop("version")
        t.update(tv), f.update(fv)
        return t, f
    S["nc-inst-6-dep-added-by-update"] = nc6

    def nc6b():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "install121", ns.cmd("Install", "1.2.1", *simple(1)))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        t.pop("version"), f.pop("version")
        return t, f
    S["nc-inst-6b-fresh-install-control"] = nc6b

    ord_ns = {}

    def order_view(ns, which):
        if which == "twin":
            runs = [f"V{c}.{m}" for (c, prim, m) in ns.truns]
        else:
            runs = [f"{c}.{m}" for (c, prim, m) in ns.fruns]
        return ",".join(runs) if runs else "<none>"

    def ncm1():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "load100", ns.cmd("Load", "1.0.0", *ordfix("1.0.0"), source="Path"))
        t["orderAfterLoad"], f["orderAfterLoad"] = order_view(ns, "twin"), order_view(ns, "fine")
        both(ns, t, f, "update200", ns.cmd("Update", "2.0.0", *ordfix("2.0.0"), source="Path"))
        t["order"], f["order"] = order_view(ns, "twin"), order_view(ns, "fine")
        t.update(ns.twin_view()), f.update(ns.fine_view())
        ord_ns["ns"] = ns
        return t, f
    S["nc-inst-model-class-order"] = ncm1

    def ncm2():
        ns, t, f = ord_ns["ns"], {}, {}
        ns.truns, ns.fruns = [], []
        both(ns, t, f, "update300", ns.cmd("Update", "3.0.0", *ordfix("3.0.0"), source="Path"))
        t["order"], f["order"] = order_view(ns, "twin"), order_view(ns, "fine")
        t.update(ns.twin_view()), f.update(ns.fine_view())
        return t, f
    S["nc-inst-model-class-number-collision"] = ncm2

    def ncm3():
        ns, t, f = ord_ns["ns"], {}, {}
        ns.truns, ns.fruns = [], []
        tstat, fstat = ns.cmd("Update", "4.0.0", *ordfix("4.0.0"), source="Path")
        # twin: the class list fails (class_list = None, inst_witness_class_without_number_fails): the command fails
        # before any step and is rolled back
        t["update400"], f["update400"] = ("err" if tstat is None else tstat), fstat
        t["order"], f["order"] = order_view(ns, "twin"), order_view(ns, "fine")
        t.update(ns.twin_view()), f.update(ns.fine_view())
        return t, f
    S["nc-inst-model-class-without-number"] = ncm3

    # reviewer finding 7: the stored developer mode (validation/logs/66-inst-fix-stored-dev.log)
    fix_ns = {}

    def ncf1():
        ns, t, f = Namespace(), {}, {}
        both(ns, t, f, "devLoad100", ns.cmd("Load", "1.0.0", *mfix("1.0.0"), source="Path", dev=True))
        both(ns, t, f, "plainLoad100", ns.cmd("Load", "1.0.0", *mfix("1.0.0"), source="Path"))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        fix_ns["ns"] = ns
        return t, f
    S["nc-inst-fix-stored-dev-plain-load"] = ncf1

    def ncf2():
        ns, t, f = fix_ns["ns"], {}, {}
        both(ns, t, f, "plainLoad200", ns.cmd("Load", "2.0.0", *mfix("2.0.0-good"), source="Path"))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        return t, f
    S["nc-inst-fix-stored-dev-load-newer"] = ncf2

    def ncf3():
        ns, t, f = fix_ns["ns"], {}, {}
        both(ns, t, f, "updateBad", ns.cmd("Update", "2.0.0", *mfix("2.0.0-bad"), source="Path"))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        t.update(mstate(ns, "twin")), f.update(mstate(ns, "fine"))
        return t, f
    S["nc-inst-fix-stored-dev-update-path-fails"] = ncf3

    def ncf4():
        ns, t, f = fix_ns["ns"], {}, {}
        both(ns, t, f, "updateGood", ns.cmd("Update", "2.0.0", *mfix("2.0.0-good"), source="Path"))
        t.update(ns.twin_view()), f.update(ns.fine_view())
        t.update(mstate(ns, "twin")), f.update(mstate(ns, "fine"))
        return t, f
    S["nc-inst-fix-stored-dev-update-path-retry"] = ncf4
    return S


NOT_EXPRESSED_INST = {
    "nc-inst-7-dev-mode-guard": "the developer-mode install guard is in LoadModuleReference (Utils/Module.cls#L109), outside this model",
    "nc-inst-8-dev-dependency-replaced": "dependency loads are not modelled (one module per command)",
    "nc-inst-9-reinstall-version": "reinstall's version choice (Main.cls#L2697) is not modelled",
    "nc-inst-10-registry-prefix": "registry selection is not modelled",
    "nc-inst-10b-registry-tie-break": "registry selection is not modelled",
    "nc-inst-10c-registry-tie-break-high": "registry selection is not modelled",
    "nc-inst-20-no-mirror": "no mirror: the observation only records that the check could not run",
}

PARTIAL_INST = {
    "nc-inst-3-update-lock-busy": "lockHeldByJob, seconds, printedLockOrWarning, jobReleased describe the harness, not model state",
    "nc-inst-6-dep-added-by-update": "only the dependency load of update-simple is modelled; the parent's status and version are not",
}


def compare(pred: dict, obs: dict, skip_prefixes=()):
    bad = []
    keys = set(pred)
    # rows observed but not predicted are mismatches too
    for k in obs:
        if re.match(r"^(afterDev-)?(any|pri):", k):
            keys.add(k)
    for k in sorted(keys):
        if any(k.startswith(p) for p in skip_prefixes):
            continue
        if k not in obs:
            bad.append(f"{k}: model {pred.get(k)} not observed")
        elif str(pred.get(k, "<absent>")) != str(obs[k]):
            bad.append(f"{k}: model {pred.get(k, '<absent>')} vs IRIS {obs[k]}")
    return bad


def conformance_update():
    out = {}
    for name, fn in update_scenarios().items():
        t, f = fn()
        obs = OBS[name]
        out[name] = {"twin": compare(t, obs), "fine": compare(f, obs), "compared": sorted(set(t) | set(f))}
    return out


# =============================================================================== A2. resolver conformance

def load_modules(dirs):
    """module.xml files -> {(name, version): [deps]} and registry versions."""
    mans, vers = {}, {}
    for d in dirs:
        root = ET.parse(d / "module.xml").getroot()
        mod = root.find(".//Module")
        name = mod.findtext("Name").strip().lower()
        ver = vnum(mod.findtext("Version").strip())
        deps = []
        for ref in mod.findall("./Dependencies/ModuleReference"):
            deps.append({"name": ref.findtext("Name").strip().lower(), "vs": (ref.findtext("Version") or "").strip(),
                         "scope": ref.get("Scope") or None})
        mans[(name, ver)] = deps
        vers.setdefault(name, []).append(ver)
    for n in vers:
        vers[n] = sorted(set(vers[n]), reverse=True)
    return mans, vers


def expr_set(vs, universe):
    if vs in ("", "*"):
        return frozenset(universe)
    if vs.startswith("^"):
        b = vnum(vs[1:])
        return frozenset(v for v in universe if b <= v < (b // 1_000_000 + 1) * 1_000_000)
    return frozenset(v for v in universe if v == vnum(vs))


def is_exact(vs):
    return re.fullmatch(r"\d+\.\d+\.\d+([-+].*)?", vs) is not None


def module_dirs(base: Path):
    return sorted(p.parent for p in base.rglob("module.xml"))


class Resolver:
    """A namespace with installed modules, resolving on both the twin and the fine model."""

    def __init__(self, mans, vers):
        self.mans, self.vers = mans, vers
        self.universe = sorted({v for (_n, v) in mans} | {1})
        self.names = sorted({n for (n, _v) in mans} | {d["name"] for ds in mans.values() for d in ds})
        self.idx = {n: i for i, n in enumerate(self.names)}
        self.installed = {}  # name -> version
        self.ireqs = []      # (by, on, set)

    def known(self, root):
        out, frontier = [root], [root]
        while frontier:
            nxt = []
            for m in frontier:
                for (by, on, _r) in self.ireqs:
                    if by == m and on not in out:
                        out.append(on)
                        nxt.append(on)
            frontier = nxt
        return out

    def resolve(self, root, root_ver, phases=(), permit=False):
        U = self.universe
        known = self.known(root) if root in self.installed else [root]
        # fine
        reg = FR.Registry(self.vers, {k: [{"name": d["name"], "range": expr_set(d["vs"], U), "exact": is_exact(d["vs"]),
                                           "scope": d["scope"]} for d in v] for k, v in self.mans.items()})
        try:
            graph, sources = FR.build_dependency_graph(root, root_ver, reg, self.installed, self.ireqs, known, phases=phases,
                                                      permit_downgrade=permit, universe=U)
            fine = ("Done", dict(graph), sources)
        except FR.ResolveError as e:
            fine = ("Failed", e.module, e.sources)
        # twin
        ix = self.idx
        man = {(ix[n], v): tuple(TR.Req(ix[d["name"]], expr_set(d["vs"], U), is_exact(d["vs"]), d["scope"]) for d in ds)
               for (n, v), ds in self.mans.items()}
        g = TR.Inp(ix[root], root_ver, {ix[n]: tuple(vs) for n, vs in self.vers.items()}, man,
                   {ix[n]: v for n, v in self.installed.items()}, frozenset(),
                   tuple(TR.IReq(ix[b], ix[o], r) for (b, o, r) in self.ireqs), frozenset(ix[k] for k in known),
                   frozenset(phases), frozenset(), False, permit)
        s, _trace = TR.run(g)
        twin = (s["st"], {self.names[k]: v for k, v in s["sel"].items()}, s.get("fail"))
        return fine, twin, g

    def install(self, root, root_ver, fine_result):
        """LoadDependencies + load: install order; a registry copy older than the installed one is refused."""
        _st, graph, sources = fine_result
        order = FR.flat_install_order(graph, sources)
        for n in order:
            if n in self.installed and self.installed[n] > graph[n]:
                return "err", f"Cannot downgrade {n} from version {vstr(self.installed[n])} to {vstr(graph[n])}", order
        for n in order:
            self.installed[n] = graph[n]
        self.installed[root] = root_ver
        self.ireqs = [x for x in self.ireqs if x[0] not in set(order) | {root}]
        for m in list(order) + [root]:
            for d in self.mans.get((m, self.installed[m]), []):
                self.ireqs.append((m, d["name"], expr_set(d["vs"], self.universe)))
        return "ok", "", order


def err_sources(text):
    return set(re.findall(r"([a-z][\w.-]*) (\d+\.\d+\.\d+):", text or ""))


def conformance_resolve():
    out = {}
    # nc-dep-11 / 11b
    mans, vers = load_modules(module_dirs(FIX / "dep11"))
    r = Resolver(mans, vers)
    fine, twin, _g = r.resolve("nc-dep-root", vnum("1.0.0"))
    obs = OBS["nc-dep-11-greedy-resolution"]
    bad_f, bad_t = [], []
    exp_src = err_sources(obs["installRootErr"])
    if not (obs["installRoot"] == "err" and fine[0] == "Failed" and fine[1] == "nc-dep-c"):
        bad_f.append(f"fine {fine[0]} {fine[1]} vs IRIS {obs['installRoot']}")
    if {(a, vstr(b)) for (a, b) in (s for s, _k in fine[2] if isinstance(s, tuple))} != exp_src:
        bad_f.append(f"fine clash sources {fine[2]} vs IRIS {exp_src}")
    if not (twin[0] == "Failed" and twin[2] and r.names[twin[2][0]] == "nc-dep-c"):
        bad_t.append(f"twin {twin}")
    out["nc-dep-11-greedy-resolution"] = {"twin": bad_t, "fine": bad_f, "compared": ["installRoot", "installRootErr (module, sources)", "module versions"]}
    r = Resolver(mans, vers)
    fa, ta, _g = r.resolve("nc-dep-a", vnum("1.0.0"))
    st, _m, _o = r.install("nc-dep-a", vnum("1.0.0"), fa)
    fine, twin, _g = r.resolve("nc-dep-root", vnum("1.0.0"))
    obs = OBS["nc-dep-11b-solvable-with-pin"]
    bad_f, bad_t = [], []
    want = {n: vnum(obs[n]) for n in ("nc-dep-a", "nc-dep-b", "nc-dep-c")}
    if not (fine[0] == "Done" and fine[1] == want):
        bad_f.append(f"fine {fine[:2]} vs IRIS {want}")
    if not (twin[0] == "Done" and twin[1] == want):
        bad_t.append(f"twin {twin[:2]} vs IRIS {want}")
    out["nc-dep-11b-solvable-with-pin"] = {"twin": bad_t, "fine": bad_f, "compared": ["installRoot", "nc-dep-a", "nc-dep-b", "nc-dep-c"]}

    # nc-dep-12 (build-dependency-graph test data)
    mans, vers = load_modules(module_dirs(DATA / "build-dependency-graph"))
    r = Resolver(mans, vers)
    fc, _t, _g = r.resolve("complex-c", vnum("3.0.0"))
    r.install("complex-c", vnum("3.0.0"), fc)
    obs = OBS["nc-dep-12-permit-downgrade"]
    bad_f, bad_t = [], []
    fine, twin, _g = r.resolve("complex-a", vnum("1.0.0"))
    if not (fine[0] == "Failed" and fine[1] == "complex-c" and any(k == "installed-floor" for _s, k in fine[2])):
        bad_f.append(f"plain load: fine {fine[:2]}")
    if not (twin[0] == "Failed" and r.names[twin[2][0]] == "complex-c"):
        bad_t.append(f"plain load: twin {twin}")
    fine, twin, g = r.resolve("complex-a", vnum("1.0.0"), permit=True)
    if not (fine[0] == "Done" and fine[1].get("complex-c") == vnum("2.0.0")):
        bad_f.append(f"permit: fine {fine[:2]}")
    else:
        st, msg, _o = r.install("complex-a", vnum("1.0.0"), fine)
        if not (st == obs["loadAPermitDowngrade"] and msg.split(" from")[0] in obs["loadAPermitDowngradeErr"]
                and "from version 3.0.0 to 2.0.0" in obs["loadAPermitDowngradeErr"]):
            bad_f.append(f"permit load: {st} {msg}")
        if vstr(r.installed["complex-c"]) != obs["cAfterPermit"] or ("complex-a" in r.installed) != (obs["aAfter"] != "absent"):
            bad_f.append("state after the refused load")
    if not (twin[0] == "Done" and twin[1].get("complex-c") == vnum("2.0.0")
            and not TR.permit_honoured(g, {r.idx[k]: v for k, v in twin[1].items()})):
        bad_t.append(f"permit: twin {twin[:2]}")
    out["nc-dep-12-permit-downgrade"] = {"twin": bad_t, "fine": bad_f,
                                        "compared": ["loadA (module, floor)", "loadAPermitDowngrade", "cAfterPermit", "aAfter"]}

    # nc-dep-13: install order and lock file order
    mans, vers = load_modules(module_dirs(FIX / "dep13"))
    r = Resolver(mans, vers)
    fine, twin, _g = r.resolve("nc-dep-lroot", vnum("1.0.0"))
    obs = OBS["nc-dep-13-lockfile-order"]
    bad_f, bad_t = [], []
    st, _m, order = r.install("nc-dep-lroot", vnum("1.0.0"), fine)
    first = "zz-base first in output" if order.index("nc-dep-zz-base") < order.index("nc-dep-aa-mid") else "aa-mid first in output"
    if first != obs["installOrder"] or st != obs["install"]:
        bad_f.append(f"install order {order}")
    if " ".join(sorted(fine[1])) != obs["dependencyKeys"]:
        bad_f.append(f"lock keys {sorted(fine[1])}")
    if twin[0] != "Done" or " ".join(sorted(twin[1])) != obs["dependencyKeys"]:
        bad_t.append(f"twin {twin[:2]}")
    out["nc-dep-13-lockfile-order"] = {"twin": bad_t, "fine": bad_f, "compared": ["install", "installOrder", "dependencyKeys"]}

    # nc-dep-16: list-dependents lookup
    obs = OBS["nc-dep-16-list-dependents-case"]
    keys = {tuple(ord(ch) for ch in "update-simple")}
    lower = TR.listed(keys, tuple(ord(ch) for ch in "update-simple"))
    mixed = TR.listed(keys, tuple(ord(ch) for ch in "Update-Simple"))
    bad = [] if (str(int(lower)) == obs["lowerListsUpdateDeps"] and str(int(mixed)) == obs["mixedListsUpdateDeps"]) else ["lookup"]
    out["nc-dep-16-list-dependents-case"] = {"twin": bad, "fine": bad, "compared": ["lowerListsUpdateDeps", "mixedListsUpdateDeps"]}

    # nc-dep-17: scoped clash
    mans, vers = load_modules(module_dirs(DATA / "build-dependency-graph"))
    r = Resolver(mans, vers)
    obs = OBS["nc-dep-17-scoped-clash"]
    bad_f, bad_t = [], []
    fine, twin, _g = r.resolve("scoped-root", vnum("2.0.0"))
    want = {n: vnum(obs[n]) for n in ("loop-a", "scoped-b", "loop-c")}
    if fine[0] != "Done" or fine[1] != want:
        bad_f.append(f"load: fine {fine[:2]} vs {want}")
    if twin[0] != "Done" or twin[1] != want:
        bad_t.append(f"load: twin {twin[:2]} vs {want}")
    if fine[0] == "Done":
        r.install("scoped-root", vnum("2.0.0"), fine)
        fine2, twin2, _g = r.resolve("scoped-root", vnum("2.0.0"), phases=("test",))
        if not (fine2[0] == "Failed" and fine2[1] == "loop-c"
                and {(a, vstr(b)) for (a, b) in (s for s, _k in fine2[2] if isinstance(s, tuple))} == err_sources(obs["testOnlyErr"])):
            bad_f.append(f"test phase: fine {fine2}")
        if not (twin2[0] == "Failed" and r.names[twin2[2][0]] == "loop-c"):
            bad_t.append(f"test phase: twin {twin2}")
    out["nc-dep-17-scoped-clash"] = {"twin": bad_t, "fine": bad_f,
                                    "compared": ["loadScopedRoot", "loop-a", "scoped-b", "loop-c", "testOnly (module, sources)"]}
    return out


NOT_EXPRESSED_DEP = {
    "nc-dep-14-lockfile-folder-dependency": "lock-file writing (LockFile.cls) and its error path are file I/O outside the resolver",
    "nc-dep-15-lockfile-failed-install": "the lock file's timing relative to Activate is file I/O outside the resolver",
    "nc-dep-18-installed-requirements": "the requested-version check of the named module is in LoadModuleReference (Utils/Module.cls#L116-L124), not BuildDependencyGraph",
    "nc-dep-19-before": "setup state of the coordinated-update fixtures",
    "nc-dep-19-blocked-update": "coordinated update with installed +snapshot versions, CoUpdatingModules and update -path is not modelled",
    "nc-dep-19b-failed-midway": "resource-ownership error during load, outside the resolver",
    "nc-dep-19c-coordinated-update": "coordinated update (CoUpdatingModules, snapshots) is not modelled",
    "nc-dep-19d-rollback-after-dep-moved": "transactions of dependency loads are not in either model (load_history.rs covers one dependency)",
    "nc-dep-19e-dev-path-update-dep-kept": "transactions of dependency loads are not in either model",
    "nc-dep-19f-dev-registry-update": "transactions of dependency loads are not in either model (the F-INST-5 cause is covered by inst_witness_dev_update_from_registry_rolls_back)",
}

def conformance_mutants():
    """Every code mutant of the fine models must contradict at least one IRIS observation."""
    res = {}
    for mods, fn in ((FU, conformance_update), (FR, conformance_resolve)):
        for m in mods.MUTANTS:
            mods.MUT.clear()
            mods.MUT.add(m)
            try:
                out = fn()
                killers = sorted(n for n, v in out.items() if v["fine"])
            except Exception as e:  # noqa: BLE001
                killers = [f"crash: {e}"]
            finally:
                mods.MUT.clear()
            res[m] = killers
    return res


# =============================================================================== B1/C1. update-step exploration and transcription

PACKAGES = {
    "none": None,
    "empty": [],
    "one": [("V1", ["a"], None)],
    "any+prim": [("V1", ["a"], ["b"])],
    "two-classes": [("V1", ["a"], None), ("V2", ["a"], None)],
    "mixed": [("V1", ["a", "b"], None), ("V2", None, ["a"])],
}
REC_STATES = {"absent": None, "visited": TU.Rec(False, False, True), "done": TU.Rec(False, True, True),
              "failed": TU.Rec(True, True, False)}


def update_configs():
    for kind, force, dev, txn, top, source, snapshot, skip_prim in itertools.product(
            ("Install", "Load", "Update"), (False, True), (None, True, False), (True, False), (True, False), ("Registry", "Path"),
            (False, True), (False, True)):
        if kind == "Load" and (source != "Path" or not top):
            continue  # load takes a directory and names the module itself
        if source == "Path" and snapshot:
            continue  # the snapshot rule only exists for registries
        if dev is False and kind != "Load":
            continue  # only load has -nodev (Main.cls#L307)
        for pre, pre_dev in ((None, False), (1, False), (1, True), (2, False), (2, True), (3, False), (3, True)):
            for pname, rows in PACKAGES.items():
                steps = flat_twin_steps(rows) if rows is not None else ()
                for states in itertools.product(REC_STATES, repeat=len(steps)):
                    recs = {k: REC_STATES[st] for k, st in zip(steps, states) if REC_STATES[st] is not None}
                    recs[(9, False, "stale")] = TU.Rec(False, True, True)
                    op = TU.Op(kind, force, dev, txn, top, source, snapshot, 2, rows is not None, skip_prim)
                    yield op, (pre, pre_dev), pname, rows, steps, recs


def twin_choices(s):
    pc = s["pc"]
    out = []
    if pc == "Init":
        return [("Init",)]
    if pc == "Guard":
        out.append(("Guard",))
    elif pc == "Manifest":
        out.append(("Manifest",))
    elif pc == "Phases":
        out += [("Phases", True), ("Phases", False)]
    elif pc == "Lock":
        out += [("Lock", True), ("Lock", False)]
    elif pc == "Walk":
        out.append(("Walk", True))
        if s["i"] < len(s["steps"]) and not s["seeding"]:
            t = TU.step(s, ("Walk", True))
            if len(t["log"]) > len(s["log"]):
                out.append(("Walk", False))  # the step runs: it may fail
    if TU.running(pc):
        out.append(("Crash",))
    return out


def crash_label(s):
    pc = s["pc"]
    if pc == "Walk":
        return f"step:{s['i']}" if s["i"] < len(s["steps"]) else "walk-end"
    return {"Guard": "guard", "Manifest": "manifest", "Phases": "phases", "Lock": "lock"}[pc]


def fine_run(op, pre_state, rows, recs, path):
    pre, pre_dev = pre_state
    env = Env()
    if pre is not None:
        env.db[("mod", "ver")] = pre
        env.db[("mod", "dev")] = pre_dev
    for (c, prim, m), r in recs.items():
        env.db[FU.row_key(prim, f"V{c}", m)] = {"start": "ts" if r.started else "", "end": "ts" if r.end else "",
                                               "status": "OK" if r.ok else "ERR"}
    lock_free, phases_ok, fail, halt = True, True, set(), None
    for s, c in path:
        if c[0] == "Lock":
            lock_free = c[1]
        elif c[0] == "Phases":
            phases_ok = c[1]
        elif c == ("Walk", False):
            k = s["steps"][s["i"]]
            fail.add((k[1], f"V{k[0]}", k[2]))
        elif c[0] == "Crash":
            halt = crash_label(s)
    ctx = FU.Ctx(op.kind, op.to, rows, force=op.force, dev=op.dev, txn=op.txn, top=op.top, source=op.source,
                 snapshot=op.snapshot, lock_free=lock_free, phases_ok=phases_ok, fail_steps=fail,
                 mirror_member=op.skip_prim, mirror_primary=False)
    env.halt_at = (halt, 1) if halt else None
    runs = []
    st = FU.load_new_module(env, ctx, runs)
    recs_f = {(int(c[1:]), prim, m): TU.Rec(*v) for (c, prim, m), v in FU.records(env).items()}
    eff = Counter({(int(k[1][1:]), k[2], k[3]): v for k, v in env.db.items() if k[0] == "eff" for _ in [0]})
    return {"ver": env.db.get(("mod", "ver")), "dev": bool(env.db.get(("mod", "dev"), False)) if ("mod", "ver") in env.db else False,
            "recs": recs_f, "eff": +eff, "log": tuple((int(c[1:]), prim, m) for (c, prim, m) in runs), "out": st,
            "reached": halt is None or any(p == halt for p in env.points)}


def twin_view_of(t):
    a = TU.abs_(t)
    out = "Crashed" if a["phase"] == "Crashed" else a["out"]
    return {"ver": a["d"]["ver"], "dev": a["d"]["dev"] if a["d"]["ver"] is not None else False, "recs": a["d"]["recs"],
            "eff": +Counter(a["d"]["jlog"]), "log": a["log"], "out": out}


def explore_update(transcribe=True):
    stats = Counter()
    bad_premise, bad_pl, bad_pf5, mism, samples = 0, Counter(), Counter(), [], {}
    t0 = time.time()
    for op, pre_state, pname, rows, steps, recs in update_configs():
        pre = pre_state[0]
        stats["configs"] += 1
        s0 = TU.start(TU.Db(pre, pre_state[1], recs, ()), op, steps)
        stack = [(s0, [])]
        while stack:
            s, path = stack.pop()
            if s["pc"] in ("Closed", "Crashed"):
                stats["runs"] += 1
                if transcribe:
                    if op.kind == "Update" and op.top and pre is None and any(c[0] == "Crash" and st["pc"] == "Guard" for st, c in path):
                        stats["twin-only crash points"] += 1
                        continue
                    f = fine_run(op, pre_state, rows, recs, path)
                    tv = twin_view_of(s)
                    fv = {k: f[k] for k in tv}
                    if tv != fv or not f["reached"]:
                        mism.append({"op": op._asdict(), "pre": pre_state, "pkg": pname, "path": [c for _s, c in path], "twin": str(tv), "fine": str(fv)})
                    stats["transcription-compared"] += 1
                continue
            for c in twin_choices(s):
                t = TU.step(s, c)
                stats["transitions"] += 1
                ok = TU.l1_next(TU.abs_(s), TU.abs_(t))
                pl, pf5 = TU.premise_pl(c), TU.premise_pf5(s)
                if not ok:
                    if pl and pf5:
                        bad_premise += 1
                        samples.setdefault("under-premises", (op._asdict(), pre_state, pname, c))
                    if not pl:
                        bad_pl[pname] += 1
                        samples.setdefault("PL", (op._asdict(), pre_state, pname, c))
                    if not pf5:
                        bad_pf5[op.kind] += 1
                        samples.setdefault("PF5", (op._asdict(), pre_state, pname, c))
                stack.append((t, path + [(s, c)]))
    stats["seconds"] = round(time.time() - t0, 1)
    return stats, bad_premise, bad_pl, bad_pf5, mism, samples


# =============================================================================== B2/C2. resolver exploration and transcription

R6 = [None, ("any", frozenset({1, 2}), False), ("=1", frozenset({1}), True), ("=2", frozenset({2}), True),
      ("<2", frozenset({1}), False), (">=2", frozenset({2}), False)]
R4 = [None, ("any", frozenset({1, 2}), False), ("=1", frozenset({1}), True), (">=2", frozenset({2}), False)]
INSTALLED = [("none", {}, (), False), ("c1", {3: 1}, (), False), ("c1-permit", {3: 1}, (), True), ("c2", {3: 2}, (), False),
             ("c2-permit", {3: 2}, (), True), ("d-needs-c1", {4: 1}, ((4, 3, frozenset({1})),), False)]


def resolver_instances():
    for ra, rb in itertools.product(R6, R6):
        for a1, a2, b1, b2 in itertools.product(R4, repeat=4):
            for inst_name, inst, ireqs, permit in INSTALLED:
                scopes = [(None, ())]
                if inst_name == "none" and rb is not None:
                    scopes = [(None, ()), ("test", ()), ("test", ("test",))]
                for scope, phases in scopes:
                    def req(n, r, sc=None):
                        return TR.Req(n, r[1], r[2], sc)
                    man = {(0, 1): tuple(x for x in (req(1, ra) if ra else None, req(2, rb, scope) if rb else None) if x)}
                    for (n, v), r in (((1, 1), a1), ((1, 2), a2), ((2, 1), b1), ((2, 2), b2)):
                        man[(n, v)] = (req(3, r),) if r else ()
                    man[(3, 1)] = ()
                    man[(3, 2)] = ()
                    yield (ra, rb, a1, a2, b1, b2, inst_name, scope, phases), TR.Inp(
                        0, 1, {1: (2, 1), 2: (2, 1), 3: (2, 1)}, man, dict(inst), frozenset(), tuple(TR.IReq(*x) for x in ireqs),
                        frozenset({0}), frozenset(phases), frozenset(), False, permit)


def all_selections(g):
    for va, vb, vc in itertools.product((None, 1, 2), repeat=3):
        yield {k: v for k, v in ((1, va), (2, vb), (3, vc)) if v is not None}


def explore_resolve(transcribe=True):
    stats = Counter()
    unsound, incomplete, permit_down, unjustified, mism, samples = 0, 0, 0, 0, [], {}
    t0 = time.time()
    for key, g in resolver_instances():
        assert TR.premise_pu(g) and TR.premise_pr(g)
        stats["instances"] += 1
        s, trace = TR.run(g, max_steps=500)
        stats[s["st"]] += 1
        if s["st"] == "Done" and not TR.sound(g, s["sel"]):
            unsound += 1
            samples.setdefault("unsound", str(key))
        if s["st"] == "Failed" and not TR.fail_justified(g, s):
            unjustified += 1
            samples.setdefault("unjustified", str(key))
        solvable = any(TR.sound(g, sel) for sel in all_selections(g))
        if solvable and s["st"] == "Failed":
            incomplete += 1
            samples.setdefault("incomplete", str(key))
        if s["st"] == "Done" and not TR.permit_honoured(g, s["sel"]):
            permit_down += 1
            samples.setdefault("permit-downgrade", str(key))
        if transcribe:
            reg = FR.Registry({n: list(vs) for n, vs in g.avail.items()},
                              {k: [{"name": q.name, "range": q.range, "exact": q.exact, "scope": q.scope} for q in v] for k, v in g.man.items()})
            try:
                graph, _src = FR.build_dependency_graph(0, 1, reg, g.inst, [(i.by, i.on, i.range) for i in g.ireqs], [0],
                                                        phases=tuple(g.phases), permit_downgrade=g.permit, universe=[1, 2])
                fv = ("Done", graph, None)
            except FR.ResolveError as e:
                fv = ("Failed", None, e.module if e.kind in ("clash", "notfound") else e.kind)
            tv = (s["st"], dict(s["sel"]) if s["st"] == "Done" else None, s.get("fail", (None,))[0] if s["st"] == "Failed" else None)
            stats["transcription-compared"] += 1
            if fv != tv:
                mism.append({"instance": str(key), "twin": str(tv), "fine": str(fv)})
    stats["seconds"] = round(time.time() - t0, 1)
    return stats, unsound, incomplete, permit_down, unjustified, mism, samples


# =============================================================================== main

def main(argv):
    records = []

    def rec(name, ok, detail, conditions=None):
        r = {"name": name, "status": "pass" if ok else "fail", "detail": detail}
        if conditions:
            r["conditions"] = conditions
        records.append(r)
        print(f"{r['status']:4} {name}: {json.dumps(detail)[:300]}")

    # A. conformance
    cu = conformance_update()
    for name, v in cu.items():
        rec(f"inst-conf-{name}", not v["twin"] and not v["fine"],
            {"twin": v["twin"] or "match", "fine": v["fine"] or "match", "compared": v["compared"]},
            PARTIAL_INST.get(name))
    cd = conformance_resolve()
    for name, v in cd.items():
        rec(f"dep-conf-{name}", not v["twin"] and not v["fine"],
            {"twin": v["twin"] or "match", "fine": v["fine"] or "match", "compared": v["compared"]})
    inst_obs = sorted(n for n in OBS if n.startswith(("nc-inst-", "nc-dep-")))
    covered = set(cu) | set(cd)
    rest = [n for n in inst_obs if n not in covered]
    why = {**NOT_EXPRESSED_INST, **NOT_EXPRESSED_DEP}
    unexplained = [n for n in rest if n not in why]
    rec("inst-dep-conformance-coverage", not unexplained,
        {"observations": len(inst_obs), "replayed": len(covered), "not_expressed": {n: why.get(n, "?") for n in rest}})
    all_ok = all(r["status"] == "pass" for r in records if r["name"].startswith(("inst-conf-", "dep-conf-")))
    rec("inst-dep-trace-conformance", all_ok, f"{len(covered)} observations replayed on twin and fine model")
    km = conformance_mutants()
    rec("inst-dep-code-mutants-rejected", all(km.values()), {m: v or "SURVIVED" for m, v in km.items()},
        "each mutant of fine_update.py / fine_resolve.py must contradict at least one IRIS observation")

    # B/C. update steps
    stats, bad_premise, bad_pl, bad_pf5, mism, samples = explore_update()
    cond_u = ("twin of update_steps.rs; ops: install/load/update with every flag combination (force, dev typed as "
              "-dev / -nodev (load only) / neither, txn, top, source, snapshot, mirror non-primary); installed version "
              "none/older/same/newer than target, each stored with and without developer mode; packages: "
              + ", ".join(PACKAGES) + " (at most 3 steps, 2 classes); each step record absent/visited/done/failed plus a "
              "stale record; every choice: phases ok/fail, lock granted/timeout, each running step ok/fail, a crash at "
              "every program point")
    rec("inst-explore-refinement-under-premises", bad_premise == 0,
        {"configs": stats["configs"], "transitions": stats["transitions"], "runs": stats["runs"], "violations": bad_premise,
         "seconds": stats["seconds"]}, cond_u + "; premises PL and PF5")
    rec("inst-explore-refinement-without-pl", sum(bad_pl.values()) == 0,
        {"violations": sum(bad_pl.values()), "by_package": dict(bad_pl), "sample": str(samples.get("PL"))},
        cond_u + "; PL dropped (F-INST-1: a fail is expected)")
    rec("inst-explore-refinement-without-pf5", sum(bad_pf5.values()) == 0,
        {"violations": sum(bad_pf5.values()), "by_kind": dict(bad_pf5), "sample": str(samples.get("PF5"))},
        cond_u + "; PF5 dropped (F-INST-5: a fail is expected)")
    rec("inst-transcription-twin-vs-fine", not mism,
        {"compared": stats["transcription-compared"], "mismatches": len(mism), "first": mism[:3],
         "twin_only_crash_points": stats["twin-only crash points"]},
        "twin (Verus spec) vs fine model (ObjectScript structure over irisenv.Env): durable version, developer mode, "
        "step records, journaled effects, executed steps, outcome; same bounds as the exploration")

    # B/C. resolver
    rs, unsound, incomplete, permit_down, unjustified, rmism, rsamples = explore_resolve()
    cond_r = ("twin of dep_resolve.rs; root + 3 modules (a, b needed by the root; c needed by a and b), versions {1, 2} "
              "each, graph depth 2; root requirements from {none, any, =1, =2, fuzzy {1}, fuzzy {2}}, a/b requirements on "
              "c from {none, any, =1, fuzzy {2}}; installed: none, c=1, c=2 (with and without PermitDowngrade), an "
              "outside module needing c=1; the root's requirement on b also test-scoped, with and without the test phase")
    rec("dep-explore-soundness", unsound == 0,
        {"instances": rs["instances"], "done": rs["Done"], "failed": rs["Failed"], "unsound": unsound, "seconds": rs["seconds"]},
        cond_r + "; premises PU and PR hold in every instance")
    rec("dep-explore-failure-justified", unjustified == 0,
        {"failed": rs["Failed"], "unjustified": unjustified, "sample": rsamples.get("unjustified")},
        cond_r + "; L1 fail_justified (a failure names a cycle of recorded requirements or a requirement with no admissible version)")
    rec("dep-explore-termination", rs.get("Bound", 0) == 0, {"runs_hitting_500_steps": rs.get("Bound", 0)}, cond_r)
    rec("dep-explore-completeness", incomplete == 0,
        {"solvable_but_failed": incomplete, "sample": rsamples.get("incomplete")},
        cond_r + "; DEC-DEP-1 option B (a fail confirms incompleteness within the bound)")
    rec("dep-explore-permit-honoured", permit_down == 0,
        {"done_with_refused_downgrade": permit_down, "sample": rsamples.get("permit-downgrade")},
        cond_r + "; L1 permit_honoured (F-DEP-1: a fail is expected)")
    rec("dep-transcription-twin-vs-fine", not rmism,
        {"compared": rs["transcription-compared"], "mismatches": len(rmism), "first": rmism[:3]},
        "twin (Verus spec) vs fine model (BuildDependencyGraph structure): outcome, chosen versions, failing module; same bounds")

    out = {"tool": "models/explorer/inst/check.py", "records": records}
    (ROOT / "evidence" / "model-checks-inst.json").write_text(json.dumps(out, indent=2) + "\n")
    print(f"wrote evidence/model-checks-inst.json: {len(records)} records")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
