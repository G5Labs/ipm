"""Literal transcription of the spec functions in models/verus/ns_mapping.rs (same names, same branches).

State: {ns_id: Ns}; Ns = dict(cfg=Maps, act=Maps, code=bool, rec=int|None, rows=bool); Maps = (pkg, rtn, repo).
Namespace ids are ints >= 0 whose order is the order of their names; LIB = -1 (IRISLIB).
"""
from __future__ import annotations

LIB = -1
NO_MAPS = (None, None, None)


def lib_db():
    return LIB


def ns(cfg, act, code, rec, rows):
    return dict(cfg=cfg, act=act, code=code, rec=rec, rows=rows)


def upd(n, **kw):
    x = dict(n)
    x.update(kw)
    return x


# ---------------------------------------------------------------- environment contract
def pkg_dest(m):
    pkg, rtn, _ = m
    return rtn if rtn is not None else (pkg if pkg is not None else LIB)


def rtn_dest(m):
    return pkg_dest(m)


def glo_dest(m, n):
    return m[2] if m[2] is not None else n


def has_code(s, d):
    return d != LIB and d in s and s[d]["code"]


def rows_at(s, d):
    return d != LIB and d in s and s[d]["rows"]


def has_ipm(s, n):
    return has_code(s, pkg_dest(s[n]["act"]))


def installed(s, n):
    return has_ipm(s, n) and s[n]["rec"] is not None


def activate(s, to):
    return {n: (upd(s[n], act=s[n]["cfg"]) if n in to else s[n]) for n in s}


# ---------------------------------------------------------------- GetMapInfo
def is_source(s, x):
    return x in s and installed(s, x)


def mapped_from(s, t, x):
    if not (t in s and not is_source(s, t) and is_source(s, x)):
        return False
    if pkg_dest(s[x]["act"]) != pkg_dest(s[t]["act"]):
        return False
    return all(y <= x for y in s if is_source(s, y) and pkg_dest(s[y]["act"]) == pkg_dest(s[t]["act"]))


def listed(s, t):
    return any(mapped_from(s, t, x) for x in s)


# ---------------------------------------------------------------- enable -map [-repos]
def map_code_one(s, src, t):
    c = s[t]["cfg"]
    if installed(s, t):
        return s[t]
    pkg = pkg_dest(s[src]["act"]) if c[0] is None else c[0]
    rtn = rtn_dest(s[src]["act"]) if c[1] is None else c[1]
    return upd(s[t], cfg=(pkg, rtn, c[2]))


def repo_mapped(s1, src, t):
    return mapped_from(s1, t, src) and not rows_at(s1, glo_dest(s1[t]["act"], t))


def map_repo_one(s1, src, t):
    c = s1[t]["cfg"]
    if repo_mapped(s1, src, t):
        m = (c[0], c[1], glo_dest(s1[src]["act"], src) if c[2] is None else c[2])
        return upd(s1[t], cfg=m, act=m)
    return s1[t]


def enable_map(s, src, ts, repos):
    s1 = {n: (map_code_one(s, src, n) if n in ts else s[n]) for n in s}
    s2 = {n: (map_repo_one(s1, src, n) if n in ts else s1[n]) for n in s1} if repos else s1
    to = {n for n in s if n in ts and (not installed(s, n) or (repos and repo_mapped(s1, src, n)))}
    return activate(s2, to)


def reset_repos(s, src):
    d = glo_dest(s[src]["act"], src)
    if d in s:
        r = dict(s)
        r[d] = upd(s[d], rows=True)
        return r
    return s


def community(s, src):
    r = reset_repos(s, src)
    return enable_map(r, src, set(r), True)


# ---------------------------------------------------------------- unmap
def remove_rtn(m, ty):
    return (m[0], None, m[2]) if ty == "All" else m


def unmap_one(s, t, repos_only):
    c = s[t]["cfg"]
    if not listed(s, t):
        return s[t]
    c1 = (c[0], c[1], None)
    c2 = c1 if repos_only else remove_rtn((None, c1[1], c1[2]), "Other")
    return upd(s[t], cfg=c2)


def unmap(s, ts, repos_only):
    s1 = {n: (unmap_one(s, n, repos_only) if n in ts else s[n]) for n in s}
    return activate(s1, {n for n in s if n in ts and s1[n]["cfg"] != s[n]["cfg"]})


# ---------------------------------------------------------------- installer, install zpm, version read
def installer(s, n, v):
    c = s[n]["cfg"]
    m = (n if c[0] is None else c[0], n if c[1] is None else c[1], c[2])
    d = pkg_dest(m)
    ns1 = dict(s)
    ns1[n] = upd(s[n], cfg=m, act=m, rec=v)
    if d in ns1:
        ns1[d] = upd(ns1[d], code=True)
    return ns1


def install_zpm(s, n, v):
    if listed(s, n):
        return s
    d = pkg_dest(s[n]["act"])
    ns1 = dict(s)
    ns1[n] = upd(s[n], rec=v)
    if d in ns1:
        ns1[d] = upd(ns1[d], code=True)
    return ns1


def check_ipm_version(s, n):
    return s[n]["rec"]


# ---------------------------------------------------------------- wf
def wf_ns(s, n):
    a = s[n]["act"]
    return (n >= 0 and s[n]["cfg"] == a
            and (a[1] is None or has_code(s, a[1]))
            and not (a[1] is None and a[0] is not None and not has_code(s, a[0]))
            and (a[2] is None or (a[2] != n and a[2] != LIB and a[2] in s)))


def wf(s):
    return all(wf_ns(s, n) for n in s)


# ---------------------------------------------------------------- L1
def view(s, n):
    return dict(code=pkg_dest(s[n]["act"]) if has_ipm(s, n) else None, repos=glo_dest(s[n]["act"], n),
                own_rows=s[n]["rows"], ver=s[n]["rec"])


def abs_(s):
    return {n: view(s, n) for n in s}


def local(v):
    return v["code"] is not None and v["ver"] is not None


def mapped(v):
    return v["code"] is not None and v["ver"] is None


def rows_of(a, n):
    return a[n]["repos"] in a and a[a[n]["repos"]]["own_rows"]


def enable_view(a, src, ts, repos, n):
    v = a[n]
    if n not in ts or local(v):
        return v
    code = v["code"] if v["code"] is not None else a[src]["code"]
    share = repos and code == a[src]["code"] and v["repos"] == n and not rows_of(a, n)
    return dict(v, code=code, repos=a[src]["repos"] if share else v["repos"])


def l1_enable(a, src, ts, repos):
    return {n: enable_view(a, src, ts, repos, n) for n in a}


def l1_reset(a, src):
    return {n: (dict(a[n], own_rows=True) if n == a[src]["repos"] else a[n]) for n in a}


def l1_community(a, src):
    r = l1_reset(a, src)
    return l1_enable(r, src, set(r), True)


def unmap_view(a, ts, repos_only, n):
    v = a[n]
    if n not in ts or not mapped(v):
        return v
    return dict(v, code=v["code"] if repos_only else None, repos=n)


def l1_unmap(a, ts, repos_only):
    return {n: unmap_view(a, ts, repos_only, n) for n in a}


def independent(a, n, v, own_repos):
    b = dict(a)
    b[n] = dict(a[n], code=n, ver=v, repos=n if own_repos else a[n]["repos"])
    return b


def l1_install(a, b, n, v):
    if mapped(a[n]):
        return b == a or b == independent(a, n, v, True) or b == independent(a, n, v, False)
    return b == independent(a, n, v, False)


def version_in_use(a, n):
    d = a[n]["code"]
    return (a[d]["ver"] if d in a else None) if d is not None else None


# ---------------------------------------------------------------- premises
def premise_src(s, src):
    return src in s and installed(s, src) and pkg_dest(s[src]["act"]) == src and s[src]["act"][2] is None


def premise_uniq(s, src):
    return all(y <= src for y in s if is_source(s, y) and pkg_dest(s[y]["act"]) == src)


def premise_tgt(s, src, ts):
    return all(t in s and (s[t]["act"][1] is not None or s[t]["act"][0] is None or s[t]["act"][0] == src) for t in ts)


def premise_repos(s, ts, repos):
    return (not repos) or all(has_ipm(s, t) for t in ts if not installed(s, t))


def premise_info(s, ts):
    return all(listed(s, t) for t in ts if t in s and mapped(view(s, t)))


def premise_rtn(s, ts, repos_only):
    return repos_only or all(s[t]["act"][1] is None for t in ts if listed(s, t))


def premise_inst(s, n):
    a = s[n]["act"]
    return (a[0] is None or a[0] == n) and (a[1] is None or a[1] == n)


def premise_own(s, n):
    return pkg_dest(s[n]["act"]) == n
