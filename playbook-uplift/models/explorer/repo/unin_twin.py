"""Executable twin of models/verus/uninstall.rs: every spec fn transcribed literally, under the same name.

Values: a resource kind is a tuple ('Code', mapped) | ('Web',) | ('Files', folder, file, keep, protected) |
('Gbl', named, preserve). A world W, parameters P and outcomes Out are dicts; updates copy (upd).
check.py (part A) checks that every `spec fn` in the .rs file has a function of the same name here.
"""
from __future__ import annotations

from collections import namedtuple

Res = namedtuple("Res", "kind owner")
Module = namedtuple("Module", "key ext_key global_scope installer hook_ok deps res needs graph")
HRow = namedtuple("HRow", "m ok")


def upd(base: dict, **kw) -> dict:
    x = dict(base)
    x.update(kw)
    return x


def Out(w, ok, skip):
    return {"w": w, "ok": ok, "skip": skip}


def ipm_id():
    return 0


def k_zpm():
    return 0


def k_ipm():
    return 1


def sd():
    return 0


def Typed(key, exact_zpm):
    return {"key": key, "exact_zpm": exact_zpm}


# ---------------------------------------------------------------- well-formedness
def owns(w, m, r):
    return r in w["res"] and w["res"][r].owner == m


def wf(w):
    mods, res = w["mods"], w["res"]
    for m, mod in mods.items():
        if not all(owns(w, m, r) for r in mod.res):
            return False
        if m in mod.needs:
            return False
        for d in mod.deps:
            if d not in mod.needs:
                return False
            if d in mods and not mods[d].needs <= mod.needs:
                return False
        if m != ipm_id() and (mod.key in (k_zpm(), k_ipm()) or mod.ext_key in (k_zpm(), k_ipm())):
            return False
    ms = list(mods)
    for a in ms:
        for b in ms:
            if a != b and (mods[a].key == mods[b].key or mods[a].ext_key == mods[b].ext_key):
                return False
    if not (sd() in res and res[sd()].owner == ipm_id() and res[sd()].kind[0] == "Code"):
        return False
    if ipm_id() in mods:
        im = mods[ipm_id()]
        if not (im.key == k_zpm() and im.ext_key == k_ipm() and sd() in im.res and res[sd()].kind == ("Code", False)
                and len(im.deps) == 0):   # IPM's manifest declares no <Dependencies> (module.xml)
            return False
    return True


def wf_cmd(w, n):
    return wf(w) and sd() in w["present"] and (not n["exact_zpm"] or n["key"] == k_zpm())


# ---------------------------------------------------------------- L2
def lookup(w, k):
    return lookup_m(w["mods"], k)


def lookup_m(mods, k):
    by_key = [m for m in mods if mods[m].key == k]
    if by_key:
        return min(by_key)          # choose; unique under wf
    by_ext = [m for m in mods if mods[m].ext_key == k]
    if by_ext:
        return min(by_ext)
    return None


def is_web(w, r):
    return r in w["res"] and w["res"][r].kind[0] == "Web"


def before_clean(w, rs):
    if len(rs) == 0:
        return w
    w1 = before_clean(w, rs[:-1])
    if is_web(w1, rs[-1]):
        return upd(w1, present=w1["present"] - {rs[-1]})
    return w1


def code_dependent(w, t, cycle, m):
    mods = w["mods"]
    return m in mods and mods[m].graph is not None and t in mods[m].graph and m not in cycle


def check_skip(w, gs, t, p):
    return (gs and not p["global"]) or (not p["force"] and p["level"] > 0 and any(code_dependent(w, t, p["cycle"], m) for m in w["mods"]))


def in_folder(k, f):
    return k[0] == "Files" and k[1] == f


def folder_set(w, f):
    return frozenset(r for r in w["res"] if in_folder(w["res"][r].kind, f))


def is_mapped(w, r):
    return r in w["res"] and w["res"][r].kind == ("Code", True)


def Acc(w, dl):
    return {"w": w, "del": dl}


def clean_step(a, r):
    w = a["w"]
    if r not in w["res"]:
        return a
    k = w["res"][r].kind
    if k[0] == "Files":
        _, folder, file, keep, protected = k
        if keep:
            return a
        if file:
            return upd(a, w=upd(w, present=w["present"] - {r}))
        if protected:
            return a
        return upd(a, w=upd(w, present=w["present"] - folder_set(w, folder)))
    if k[0] == "Gbl":
        _, named, preserve = k
        if named and not preserve:
            return upd(a, w=upd(w, present=w["present"] - {r}))
        return a
    if k[0] == "Code":
        return upd(a, **{"del": a["del"] | {r}})
    return a


def clean_res(w, rs):
    if len(rs) == 0:
        return Acc(w, frozenset())
    return clean_step(clean_res(w, rs[:-1]), rs[-1])


def code_gone(w, dl, rm_mapped):
    return frozenset(r for r in dl if rm_mapped or not is_mapped(w, r))


def clean_code(w, rs, rm_mapped):
    a = clean_res(w, rs)
    return upd(a["w"], present=a["w"]["present"] - code_gone(a["w"], a["del"], rm_mapped))


def record_delete(o, t):
    w = o["w"]
    mods = dict(w["mods"])
    mods.pop(t, None)
    return upd(o, w=upd(w, mods=mods, streams=w["streams"] - {t}))


def ipm_block(o, t, p):
    if t == ipm_id() and p["full"]:
        return upd(o, w=upd(o["w"], mods={}, streams=frozenset()))
    return o


MUT = set()   # names of twin mutants switched on (check.py, part E)


def exec_clean(w, t, p, fuel):
    if t not in w["mods"]:
        return Out(w, False, False)
    m = w["mods"][t]
    w1 = w if "no-before-delete" in MUT else before_clean(w, m.res)
    if check_skip(w1, m.global_scope, t, p):
        return Out(w1, True, True)
    w2 = upd(w1, hooks=w1["hooks"] + (t,)) if m.installer else w1
    if m.installer and not m.hook_ok:
        return Out(w2, False, False)
    p1 = upd(p, cycle=p["cycle"] | {t})
    if check_skip(w2, m.global_scope, t, p1):
        return Out(w2, True, True)
    w3 = after_code(w2, m, p)
    o = after_deps(w3, m, p1, fuel)
    if not o["ok"]:
        return o
    lvl = 1 if "top-record-kept" in MUT else 0
    o2 = record_delete(o, t) if p["level"] > lvl and not p["nested"] and sd() in o["w"]["present"] else o
    return ipm_block(o2, t, p)


def after_code(w2, m, p):
    return clean_code(w2, m.res, p["global"]) if p["level"] > 0 else w2


def after_deps(w3, m, p1, fuel):
    if not p1["recurse"]:
        return Out(w3, True, False)
    if fuel == 0:
        return Out(w3, False, False)
    return dep_loop(w3, m.deps, p1, fuel - 1)


def dep_loop(w, ds, p, fuel):
    if len(ds) == 0:
        return Out(w, True, False)
    o = dep_loop(w, ds[:-1], p, fuel)
    if not o["ok"]:
        return o
    return dep_step(o["w"], ds[-1], p, fuel)


def dep_step(w, d, p, fuel):
    if d in p["cycle"] or d not in w["mods"]:
        return Out(w, True, False)
    o = exec_clean(w, d, upd(p, level=p["level"] + 1, nested=True), fuel)
    if not o["ok"]:
        return Out(upd(o["w"], hist=o["w"]["hist"] + (HRow(d, False),)), False, False)
    if sd() not in o["w"]["present"] and "no-class-gone-error" not in MUT:
        return Out(o["w"], False, False)
    if not o["skip"]:
        mods = dict(o["w"]["mods"])
        mods.pop(d, None)
        w4 = upd(o["w"], mods=mods)
    else:
        w4 = o["w"]
    return Out(upd(w4, hist=w4["hist"] + (HRow(d, True),)), True, False)


def top_p(force, recurse, glob, full):
    return {"level": 1, "force": force, "recurse": recurse, "global": glob, "nested": False, "full": full, "cycle": frozenset()}


def storage_uninstall(w, k, force, recurse, glob, full, fuel):
    t = lookup(w, k)
    if t is None:
        return Out(upd(w, hist=w["hist"] + (HRow(None, False),)), False, False)
    p = top_p(force, recurse, glob, full)
    if t == ipm_id():
        return exec_clean(upd(w, hist=w["hist"] + (HRow(t, True),)), t, p, fuel)
    o = exec_clean(w, t, p, fuel)
    return upd(o, w=upd(o["w"], hist=o["w"]["hist"] + (HRow(t, o["ok"]),)))


def main_uninstall(w, n, force, recurse, glob, answer, fuel):
    if n["exact_zpm"] and "no-name-guard" not in MUT:
        if w["mapped_from"]:
            return Out(w, False, False)
        w1 = upd(w, prompted=True) if not force else w
        full = (not force) and answer
        return storage_uninstall(upd(w1, exit_flag=True), n["key"], force, recurse, glob, full, fuel)
    return storage_uninstall(w, n["key"], force, recurse, glob, False, fuel)


def clean_cmd(w, k, force, glob, fuel):
    t = lookup(w, k)
    if t is None:
        return Out(w, False, False)
    return exec_clean(w, t, {"level": 0, "force": force, "recurse": True, "global": glob, "nested": False, "full": False, "cycle": frozenset()}, fuel)


# ---------------------------------------------------------------- premises
def has_web(w, t):
    return any(is_web(w, r) for r in w["mods"][t].res)


def intent_dependent(w, t):
    return any(t in w["mods"][m].needs for m in w["mods"])


def intent_blocked(w, t, force, glob):
    return (w["mods"][t].global_scope and not glob) or (not force and intent_dependent(w, t))


def premise_pw(w, t):
    return not has_web(w, t)


def premise_pb(w, t, force, glob):
    return not intent_blocked(w, t, force, glob)


def premise_pg(w):
    return all(w["mods"][m].graph == w["mods"][m].needs for m in w["mods"])


def premise_pga(w, t):
    return not any(r in w["res"] and w["res"][r].kind[0] == "Gbl" and not w["res"][r].kind[1] for r in w["mods"][t].res)


def premise_pf(w):
    res = w["res"]
    for r1 in res:
        k1 = res[r1].kind
        if cuts(res, r1, k1[1] if k1[0] == "Files" else None):
            for r2 in res:
                if in_folder(res[r2].kind, k1[1]) and r1 != r2:
                    return False
    return True


def premise_pn(w, n):
    return lookup(w, n["key"]) != ipm_id() or n["exact_zpm"]


def premise_pi(w, t, recurse):
    return not recurse or ipm_id() not in w["mods"][t].needs


def premise_pcw(w, t):
    return not has_web(w, t) and not w["mods"][t].installer


def premise_pnf(n, force, answer):
    return not (n["exact_zpm"] and not force and answer)


def premise_pr(recurse):
    return not recurse


# ---------------------------------------------------------------- L1
def removed(w, m):
    return m not in w["mods"] or (m == ipm_id() and sd() not in w["present"])


def same_ns(a, b):
    return a["present"] == b["present"] and a["mods"] == b["mods"] and a["streams"] == b["streams"] and a["hooks"] == b["hooks"]


def scope(w, t, recurse):
    return w["mods"][t].needs | {t} if recurse else frozenset({t})


def exempt(w, r, glob):
    if r not in w["res"]:
        return False
    k = w["res"][r].kind
    if k[0] == "Files":
        return k[3] or (k[4] and not k[2])
    if k[0] == "Code":
        return k[1] and not glob
    if k[0] == "Gbl":
        return k[2]
    return False


def l1_blocked_unchanged(w, t, force, glob, o):
    return not intent_blocked(w, t, force, glob) or same_ns(w, o["w"])


def l1_honest(w, t, o):
    return o["ok"] == removed(o["w"], t)   # both directions


def l1_dependents_protect(w, t, force, o):
    return not (not force and intent_dependent(w, t)) or (t in o["w"]["mods"] and o["w"]["mods"][t] == w["mods"][t])


def l1_only_own(w, t, recurse, o):
    sc = scope(w, t, recurse)
    for r in w["present"] - o["w"]["present"]:
        if not (r in w["res"] and w["res"][r].owner in sc):
            return False
    return all(m in sc for m in w["mods"] if m not in o["w"]["mods"])


def l1_kept_files(w, o):
    return all(r in o["w"]["present"] for r in w["present"] if r in w["res"] and w["res"][r].kind[0] == "Files" and w["res"][r].kind[3])


def l1_complete(w, t, glob, o):
    if t in o["w"]["mods"]:
        return True
    return all(exempt(w, r, glob) or r not in o["w"]["present"] for r in w["mods"][t].res)


def l1_self_guard(w, force, o):
    return (o["w"]["exit_flag"] and (o["w"]["prompted"] or force)) or (not o["ok"] and same_ns(w, o["w"]))


def l1_ipm_kept(w, o):
    return sd() in o["w"]["present"] and (ipm_id() not in w["mods"] or ipm_id() in o["w"]["mods"])


def l1_clean_keeps_target(w, t, o):
    ow = o["w"]
    if not (t in ow["mods"] and ow["mods"][t] == w["mods"][t]):
        return False
    if any(owns(w, t, r) and r in w["present"] and r not in ow["present"] for r in w["res"]):
        return False
    return all(h != t for h in ow["hooks"][len(w["hooks"]):])


def l1_rows_honest(w, o):
    return all(h.ok == removed(o["w"], h.m) for h in o["w"]["hist"][len(w["hist"]):] if h.m is not None)   # both directions


def l1_manifest_with_record(w, o):
    return all(m not in o["w"]["streams"] for m in w["mods"] if m not in o["w"]["mods"])


def l1_not_found(w, o):
    return not o["ok"] and same_ns(w, o["w"])


# ---------------------------------------------------------------- frame (proof helpers; transcribed for parity)
def cuts(res, r2, f):
    if r2 not in res:
        return False
    k = res[r2].kind
    return k[0] == "Files" and k[1] == f and not k[2] and not k[3] and not k[4]


def may_delete(res, r, a):
    if r not in res:
        return False
    k = res[r].kind
    if k[0] == "Files":
        return (k[2] and not k[3] and res[r].owner in a) or any(cuts(res, r2, k[1]) and res[r2].owner in a for r2 in res)
    return res[r].owner in a


def frame(w, w2, a, full):
    if w2["res"] != w["res"] or not w2["present"] <= w["present"]:
        return False
    if not all(may_delete(w["res"], r, a) for r in w["present"] - w2["present"]):
        return False
    if not all(m in w["mods"] and w2["mods"][m] == w["mods"][m] for m in w2["mods"]):
        return False
    if not full and not all(m in a for m in w["mods"] if m not in w2["mods"]):
        return False
    if not w2["streams"] <= w["streams"]:
        return False
    n = len(w["hooks"])
    if w2["hooks"][:n] != w["hooks"] or not all(h in a for h in w2["hooks"][n:]):
        return False
    return w2["exit_flag"] == w["exit_flag"] and w2["prompted"] == w["prompted"] and w2["mapped_from"] == w["mapped_from"]


def webs(w, rs):
    return frozenset(r for r in rs if is_web(w, r))


def owned_by(w, rs, a):
    return all(r in w["res"] and w["res"][r].owner in a for r in rs)


def res_loop_post(w, rs, acc, a):
    return frame(w, acc["w"], a, False)


def scope_p(w, t, p):
    return w["mods"][t].needs | {t} if p["recurse"] else frozenset({t})


def deps_within(w, ds, a):
    return all((w["mods"][d].needs | {d}) <= a for d in ds if d in w["mods"])


def top_w(w, n, force, t):
    w0 = upd(upd(w, prompted=True) if not force else w, exit_flag=True) if n["exact_zpm"] else w
    return upd(w0, hist=w0["hist"] + (HRow(t, True),)) if t == ipm_id() else w0


def top_full(n, force, answer):
    return n["exact_zpm"] and not force and answer


# ---------------------------------------------------------------- witness worlds (same as the .rs)
def mk(key, ext, deps, res, needs, graph):
    return Module(key, ext, False, False, True, tuple(deps), tuple(res), frozenset(needs), None if graph is None else frozenset(graph))


def ipm_mod():
    return mk(k_zpm(), k_ipm(), [], [sd()], [], [])


def code(owner):
    return Res(("Code", False), owner)


def base(mods, res, present):
    return {"mods": dict(mods), "streams": frozenset(mods), "res": dict(res), "present": frozenset(present), "hooks": (), "hist": (),
            "exit_flag": False, "prompted": False, "mapped_from": False}


def plain(k):
    return Typed(k, False)


def w_blocked():
    return base({0: ipm_mod(), 2: mk(2, 20, [], [10, 11], [], []), 3: mk(3, 30, [2], [12], [2], [2])},
                {0: code(0), 10: code(2), 11: Res(("Web",), 2), 12: code(3)}, {0, 10, 11, 12})


def w_self():
    return base({0: ipm_mod()}, {0: code(0)}, {0})


def w_ghost():
    return base({0: ipm_mod(), 2: mk(2, 20, [], [10], [], []), 3: mk(3, 30, [2], [12], [2], None)},
                {0: code(0), 10: code(2), 12: code(3)}, {0, 10, 12})


def w_global():
    return base({0: ipm_mod(), 2: mk(2, 20, [], [13], [], [])}, {0: code(0), 13: Res(("Gbl", False, False), 2)}, {0, 13})


def w_shared():
    return base({0: ipm_mod(), 2: mk(2, 20, [], [14], [], []), 3: mk(3, 30, [], [15], [], [])},
                {0: code(0), 14: Res(("Files", 7, False, True, False), 2), 15: Res(("Files", 7, False, False, False), 3)}, {0, 14, 15})


def w_clean():
    x = mk(2, 20, [], [10, 11], [], [])._replace(installer=True)
    return base({0: ipm_mod(), 2: x}, {0: code(0), 10: code(2), 11: Res(("Web",), 2)}, {0, 10, 11})


def w_ipmdep():
    return base({0: ipm_mod(), 2: mk(2, 20, [0], [10], [0], [0])}, {0: code(0), 10: code(2)}, {0, 10})


def w_recskip():
    return base({0: ipm_mod(), 2: mk(2, 20, [], [10, 11], [], []), 3: mk(3, 30, [2], [12], [2], [2]), 4: mk(4, 40, [2], [13], [2], [2])},
                {0: code(0), 10: code(2), 11: Res(("Web",), 2), 12: code(3), 13: code(4)}, {0, 10, 11, 12, 13})


def w_recremove():
    return base({0: ipm_mod(), 2: mk(2, 20, [], [10], [], []), 3: mk(3, 30, [2], [12], [2], [2])},
                {0: code(0), 10: code(2), 12: code(3)}, {0, 10, 12})
