"""Executable twin of models/verus/dep_resolve.rs: a literal transcription of its spec functions.

Version expressions are Python frozensets of allowed versions (Verus ISet<nat>); maps are dicts; Seq is tuple.
Each function carries the name and body of the Verus spec function it transcribes.
"""
from __future__ import annotations

from collections import namedtuple

Req = namedtuple("Req", "name range exact scope")
IReq = namedtuple("IReq", "by on range")
Entry = namedtuple("Entry", "path scope")
Item = namedtuple("Item", "idx dpath")
# Inp fields: root root_ver avail man inst refresh ireqs known phases qphases ignore permit
Inp = namedtuple("Inp", "root root_ver avail man inst refresh ireqs known phases qphases ignore permit")


def upd(x, **kw):
    y = dict(x)
    y.update(kw)
    return y


def manifest(g, n, v):
    return g.man.get((n, v), ())


def versions(g, n):
    return g.avail.get(n, ())


def active(ph, q):
    return q.scope is None or q.scope in ph


# ---------------------------------------------------------------- L1
def has(g, sel, n):
    return n == g.root or n in sel


def vsel(g, sel, n):
    return g.root_ver if n == g.root else sel[n]


def reqs_met(g, sel):
    for m in [g.root] + list(sel):
        for q in manifest(g, m, vsel(g, sel, m)):
            if active(g.phases, q) and not (has(g, sel, q.name) and vsel(g, sel, q.name) in q.range):
                return False
    return True


def installed_met(g, sel):
    if g.ignore:
        return True
    return all(ir.range.__contains__(sel[ir.on]) for ir in g.ireqs
               if ir.on in sel and ir.by not in g.known and ir.by not in sel)


def no_downgrade(g, sel):
    if g.ignore or g.permit:
        return True
    return all(sel[d] >= g.inst[d] for d in sel if d in g.inst)


def exists_ok(g, sel):
    return all(sel[d] in versions(g, d) or (d in g.inst and sel[d] == g.inst[d]) for d in sel)


def sound(g, sel):
    return reqs_met(g, sel) and installed_met(g, sel) and no_downgrade(g, sel) and exists_ok(g, sel)


# ---------------------------------------------------------------- L2
def init(g):
    return {"sel": {}, "recs": {}, "cur": {g.root: Entry((g.root,), None)}, "next": {}, "par": g.root,
            "par_ver": g.root_ver, "pend": (), "st": "Running", "why": ("Running",)}


def cache_ver(g, s, n):
    return s["sel"][n] if n in s["sel"] else g.root_ver


def dpath(q, path):
    return () if (q.scope is not None and len(path) <= 1) else path


def has_cycle(ph, reqs, path):
    return any(active(ph, q) and q.name in dpath(q, path) for q in reqs)


def items_from(ph, reqs, path, exact, i):
    out = ()
    for j in range(i, len(reqs)):
        if active(ph, reqs[j]) and reqs[j].exact == exact:
            out = out + (Item(j, dpath(reqs[j], path)),)
    return out


def classify(ph, reqs, path):
    return items_from(ph, reqs, path, True, 0) + items_from(ph, reqs, path, False, 0)


def is_min(m, n):
    return n in m and all(n <= x for x in m)


def step_dequeue(g, s, n):
    e = s["cur"][n]
    path = () if (e.scope is not None and len(e.path) == 1) else e.path
    v = cache_ver(g, s, n)
    reqs = manifest(g, n, v)
    if has_cycle(g.phases, reqs, path):
        return upd(s, st="Failed", why=("Cycle", n, path))
    cur = dict(s["cur"])
    del cur[n]
    return upd(s, cur=cur, par=n, par_ver=v, pend=classify(g.phases, reqs, path))


def rec_ok(s, d, v):
    return all(rng.__contains__(v) for key, rng in s["recs"].items() if key[0] == d)


def excluded(g, s, b):
    return b in g.known or b in s["sel"]


def inst_ok(g, s, d, v):
    return g.ignore or all(ir.range.__contains__(v) for ir in g.ireqs if ir.on == d and not excluded(g, s, ir.by))


def base(g, s, q, v):
    return v in q.range and rec_ok(s, q.name, v) and inst_ok(g, s, q.name, v)


def keep_installed(g, s, q):
    return (not g.ignore and q.name in g.inst and base(g, s, q, g.inst[q.name])
            and (q.name not in s["sel"] or s["sel"][q.name] == g.inst[q.name]) and q.name not in g.refresh)


def floor_ok(g, d, v):
    return not (not g.ignore and d in g.inst and not g.permit) or v >= g.inst[d]


def cand(g, s, q, v):
    return base(g, s, q, v) and floor_ok(g, q.name, v)


def quick_ok(g, s, d, c):
    gr = {**s["sel"], d: c}
    for sub in manifest(g, d, c):
        if active(g.qphases, sub) and sub.name in gr and gr[sub.name] not in sub.range:
            return False
    return True


def first_cand(g, s, q, vs, i):
    for j in range(i, len(vs)):
        if cand(g, s, q, vs[j]):
            return vs[j]
    return None


def fuzzy_pick(g, s, q, vs, i, last):
    for j in range(i, len(vs)):
        if cand(g, s, q, vs[j]):
            if quick_ok(g, s, q.name, vs[j]):
                return vs[j]
            last = vs[j]
    return last


def pick(g, s, q):
    if keep_installed(g, s, q):
        return g.inst[q.name]
    if q.exact:
        return first_cand(g, s, q, versions(g, q.name), 0)
    return fuzzy_pick(g, s, q, versions(g, q.name), 0, None)


def step_process(g, s):
    it = s["pend"][0]
    q = manifest(g, s["par"], s["par_ver"])[it.idx]
    v = pick(g, s, q)
    if v is None:
        return upd(s, st="Failed", fail=(q.name, s["par"], s["par_ver"]), why=("NoVersion", s["par"], s["par_ver"], it.idx))
    return upd(s, sel={**s["sel"], q.name: v}, recs={**s["recs"], (q.name, s["par"], s["par_ver"]): q.range},
               next={**s["next"], q.name: Entry(it.dpath + (q.name,), q.scope)}, pend=s["pend"][1:])


def step_level(g, s):
    if not s["next"]:
        return upd(s, st="Done")
    return upd(s, cur=s["next"], next={})


def enabled(s, c):
    if s["st"] != "Running":
        return False
    if c[0] == "Dequeue":
        return len(s["pend"]) == 0 and is_min(s["cur"], c[1])
    if c[0] == "Process":
        return len(s["pend"]) > 0
    return len(s["pend"]) == 0 and not s["cur"]


def step(g, s, c):
    if c[0] == "Dequeue":
        return step_dequeue(g, s, c[1])
    if c[0] == "Process":
        return step_process(g, s)
    return step_level(g, s)


def the_choice(s):
    """The unique enabled choice (dep_deterministic)."""
    if s["st"] != "Running":
        return None
    if s["pend"]:
        return ("Process",)
    if s["cur"]:
        return ("Dequeue", min(s["cur"]))
    return ("Level",)


def run(g, max_steps=10000):
    s = init(g)
    trace = []
    for _ in range(max_steps):
        c = the_choice(s)
        if c is None:
            return s, trace
        assert enabled(s, c)
        s = step(g, s, c)
        trace.append((c, s))
    return upd(s, st="Bound"), trace


# ---------------------------------------------------------------- L1: a failure has a stated reason
def edge(recs, a, b):
    return any(k[0] == b and k[1] == a for k in recs)


def chain(recs, p):
    return all(edge(recs, p[k], p[k + 1]) for k in range(len(p) - 1))


def cycle_reason(g, b, n, p):
    if not (has(g, b["sel"], n) and len(p) > 0 and p[-1] == n and chain(b["recs"], p)):
        return False
    return any(active(g.phases, q) and q.name in p for q in manifest(g, n, vsel(g, b["sel"], n)))


def admissible(g, b, q, v):
    return (v in q.range
            and all(r.__contains__(v) for key, r in b["recs"].items() if key[0] == q.name)
            and (g.ignore or all(ir.range.__contains__(v) for ir in g.ireqs
                                 if ir.on == q.name and ir.by not in g.known and ir.by not in b["sel"]))
            and (not (not g.ignore and q.name in g.inst and not g.permit) or v >= g.inst[q.name]))


def no_version(g, b, q):
    if any(admissible(g, b, q, v) for v in versions(g, q.name)):
        return False
    return not (not g.ignore and q.name in g.inst and q.name not in g.refresh
                and (q.name not in b["sel"] or b["sel"][q.name] == g.inst[q.name]) and admissible(g, b, q, g.inst[q.name]))


def version_reason(g, b, m, mv, i):
    reqs = manifest(g, m, mv)
    return has(g, b["sel"], m) and i < len(reqs) and active(g.phases, reqs[i]) and no_version(g, b, reqs[i])


def fail_justified(g, b):
    w = b["why"]
    if w[0] == "Cycle":
        return cycle_reason(g, b, w[1], w[2])
    if w[0] == "NoVersion":
        return version_reason(g, b, w[1], w[2], w[3])
    return False


def premise_pu(g):
    for (n, v), reqs in g.man.items():
        names = [q.name for q in reqs if active(g.phases, q)]
        if len(names) != len(set(names)):
            return False
    return True


def premise_pr(g):
    return all(q.name != g.root for reqs in g.man.values() for q in reqs if active(g.phases, q))


# Utils/Module.cls#L1213-L1216
def load_refuses(installed, target):
    return installed > target


def permit_honoured(g, sel):
    return (not g.permit) or all(not load_refuses(g.inst[d], sel[d]) for d in sel if d in g.inst)


# F-DEP-3
def lower(c):
    return c + 32 if 65 <= c <= 90 else c


def lower_all(s):
    return tuple(lower(c) for c in s)


def listed(keys, typed):
    return typed in keys


def case_blind(keys, typed):
    return (lower_all(typed) not in keys) or listed(keys, typed)
