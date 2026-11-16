"""Executable twin of models/verus/settings.rs: every spec fn transcribed literally, under the same name.

A config store is {'nodes': {k: Node}, 'conf': frozenset, 'seed': {k: v}}; a registry list is a tuple of Reg in ID
order (index i is ID i+1); default modifiers are {'dbs': {db: {k: v}}}.
"""
from __future__ import annotations

from collections import namedtuple

Node = namedtuple("Node", "val dflt")
Reg = namedtuple("Reg", "name kind enabled deploy token userpw sort_override offer")
MUT = set()


def upd(base: dict, **kw) -> dict:
    x = dict(base)
    x.update(kw)
    return x


# ---------------------------------------------------------------- config
def empty_v():
    return 0


def k_trf():
    return 1


def k_mroot():
    return 2


def mroot_default():
    return 3


def valid_trf(v):
    return v in (empty_v(), 10, 11, 12)


def get_value(c, k):
    n = c["nodes"].get(k)
    return n.val if n is not None and n.val is not None else empty_v()


def default_value(c, k):
    n = c["nodes"].get(k)
    return n.dflt if n is not None and n.dflt is not None else empty_v()


def set_value(c, k, v):
    old = c["nodes"].get(k, Node(None, None))
    d = v if (old.dflt is None or "default-overwritten" in MUT) else old.dflt
    nodes = dict(c["nodes"])
    nodes[k] = Node(v, d)
    return upd(c, nodes=nodes)


def update_one(c, k, v):
    if k not in c["conf"]:
        return (c, "Ok")
    if k == k_trf():
        return (set_value(c, k, v), "Ok") if valid_trf(v) else (c, "Err")
    if k == k_mroot() and v == empty_v():
        return (c, "Err")
    return (set_value(c, k, v), "Ok")


def reset(c, k):
    if k not in c["conf"]:
        return c
    if k == k_trf():
        dv = empty_v()
    elif k == k_mroot():
        dv = mroot_default()
    else:
        dv = get_value(c, k) if "reset-to-value" in MUT else default_value(c, k)
    return set_value(c, k, dv)


def config_set(c, ns, k, v):
    if v == empty_v():
        return (c, "Ok")
    return (update_one(c, k, v)[0], "Ok")


def config_get(c, ns, k):
    return get_value(c, k) if k in c["conf"] else None


def config_delete(c, ns, k):
    return (reset(c, k), "Ok")


def factory(c, k):
    if k == k_trf():
        return empty_v()
    if k == k_mroot():
        return mroot_default()
    return c["seed"].get(k, empty_v())


def valid_set(c, k, v):
    return k in c["conf"] and v != empty_v() and (k != k_trf() or valid_trf(v))


def cfg_abs(c):
    return {k: get_value(c, k) for k in c["conf"]}


def l1_cfg_set(a, conf, k, v):
    return {**a, k: v} if k in conf else a


def l1_config_status(c, k, v, st):
    return (st == "Ok") == valid_set(c, k, v)


def premise_ps(c, k):
    n = c["nodes"].get(k)
    return k == k_trf() or k == k_mroot() or (k in c["seed"] and n is not None and n.dflt == c["seed"][k])


def cfg0():
    return {"nodes": {}, "conf": frozenset({1, 2, 5}), "seed": {}}


# ---------------------------------------------------------------- default modifiers
def pct_db(ns):
    return ns if "percent-per-namespace" in MUT else 0


def dm_store(s, ns):
    return s["dbs"].get(pct_db(ns), {})


def dm_set(s, ns, d):
    dbs = dict(s["dbs"])
    dbs[pct_db(ns)] = {**dm_store(s, ns), **d}
    return {"dbs": dbs}


def dm_get(s, ns):
    return dm_store(s, ns)


def dm_delete(s, ns):
    dbs = dict(s["dbs"])
    dbs[pct_db(ns)] = {}
    return {"dbs": dbs}


def effective(s, ns, cmd):
    return {**dm_get(s, ns), **cmd}


def dm_l1_set(a, ns, d):
    return {**a, ns: {**a.get(ns, {}), **d}}


def dm_abs(s, nss):
    return {ns: dm_store(s, ns) for ns in nss}


def premise_pns(nss, ns):
    return all(x == ns for x in nss)


# ---------------------------------------------------------------- registries
def wf_regs(rs):
    names = [r.name for r in rs]
    return len(names) == len(set(names))


def publish_order(r):
    return 1 if r.deploy else 2 if r.token else 3 if r.userpw else 999


def first_of(rs, k):
    if len(rs) == 0:
        return None
    f = first_of(rs[:-1], k)
    if f is not None and "first-row-best-order" not in MUT:
        return f
    if "first-row-best-order" in MUT:
        cands = [i for i in range(len(rs)) if rs[i].kind == k and rs[i].enabled]
        return min(cands, key=lambda i: (publish_order(rs[i]), i)) if cands else None
    if rs[-1].kind == k and rs[-1].enabled:
        return len(rs) - 1
    return None


def key_lt(a, b):
    return publish_order(a) < publish_order(b) or (publish_order(a) == publish_order(b) and a.name < b.name)


def publishing_server(rs):
    r = first_of(rs, "Remote")
    o = first_of(rs, "Oras")
    if r is None:
        return o
    if o is None:
        return r
    return o if key_lt(rs[o], rs[r]) else r


def find_name(rs, n):
    if len(rs) == 0:
        return None
    if rs[-1].name == n:
        return len(rs) - 1
    return find_name(rs[:-1], n)


def publish_target(rs, named):
    t = find_name(rs, named) if named is not None else publishing_server(rs)
    if t is None:
        return None
    if "publish-checks-enabled" in MUT and not rs[t].enabled:
        return None
    return None if rs[t].kind == "Folder" else t


def search_lists(rs, i):
    if "search-enabled-only" in MUT:
        return 0 <= i < len(rs) and rs[i].enabled
    return 0 <= i < len(rs)


def install_pick(rs):
    if len(rs) == 0:
        return None
    b = install_pick(rs[:-1])
    i = len(rs) - 1
    if not rs[i].enabled or rs[i].offer is None:
        return b
    if b is None or rs[i].offer > rs[b].offer:
        return i
    if "tie-by-sort-order" in MUT and rs[i].offer == rs[b].offer and sort_order(rs, i) < sort_order(rs, b):
        return i
    return b


def holder(rs, i):
    return 0 <= i < len(rs) and rs[i].enabled and rs[i].deploy and rs[i].kind != "Folder"


def l1_publish(rs, named, pick):
    if named is not None:
        if pick is not None:
            return 0 <= pick < len(rs) and rs[pick].name == named and rs[pick].enabled and rs[pick].kind != "Folder"
        return not any(rs[i].name == named and rs[i].enabled and rs[i].kind != "Folder" for i in range(len(rs)))
    if pick is not None:
        return holder(rs, pick)
    return not any(holder(rs, i) for i in range(len(rs)))


def l1_search(rs, i):
    return 0 <= i < len(rs) and rs[i].enabled


def sort_order(rs, i):
    if rs[i].sort_override is not None:
        return rs[i].sort_override
    return 2 if rs[i].kind == "Folder" else 3 + i


def better(rs, i, j):
    return rs[i].offer > rs[j].offer or (rs[i].offer == rs[j].offer and sort_order(rs, i) < sort_order(rs, j))


def l1_install(rs, pick):
    if pick is not None:
        if not (0 <= pick < len(rs) and rs[pick].enabled and rs[pick].offer is not None):
            return False
        return not any(better(rs, j, pick) for j in range(len(rs)) if rs[j].enabled and rs[j].offer is not None and j != pick)
    return not any(rs[j].enabled and rs[j].offer is not None for j in range(len(rs)))


def premise_ph(rs):
    return any(holder(rs, i) for i in range(len(rs)))


def premise_pu(rs):
    return sum(1 for i in range(len(rs)) if holder(rs, i)) <= 1


def premise_pk(rs):
    return all(first_of(rs, rs[i].kind) == i for i in range(len(rs)) if holder(rs, i))


def premise_pe_named(rs, n):
    return all(rs[i].enabled for i in range(len(rs)) if rs[i].name == n)


def premise_pe_all(rs):
    return all(r.enabled for r in rs)


def premise_pt(rs):
    for i in range(len(rs)):
        for j in range(i + 1, len(rs)):
            a, b = rs[i], rs[j]
            if a.enabled and b.enabled and a.offer is not None and b.offer is not None and a.offer == b.offer and sort_order(rs, i) > sort_order(rs, j):
                return False
    return True


def reg(name, kind, enabled, deploy, token):
    return Reg(name, kind, enabled, deploy, token, False, None, None)
