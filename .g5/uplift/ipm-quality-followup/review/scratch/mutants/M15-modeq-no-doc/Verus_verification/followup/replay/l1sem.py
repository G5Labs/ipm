"""L1 matching clauses of crates/ipm-load-history/src/contract.rs, applied to native observations.

Every predicate is three-valued: True, False, or None when an observation the clause needs is
missing. A missing observation never counts as agreement.

The projection from `OBS` lines to the model's `Mod` and `Twin` records is in `project_mod` and
`project_twin`; PROJECTION.md lists each field, its producer and its observability.
"""

# ---------------------------------------------------------------- three-valued logic


def isbad(x):
    return isinstance(x, tuple) and len(x) == 2 and x[0] == "bad"


def tv(x):
    """A malformed observation is unknown: it can neither pass nor fail a clause. The evaluator
    reports it as an evidence defect (`malformed_fields`)."""
    return x if (x is True or x is False or x is None) else None


def AND(*xs):
    xs = [tv(x) for x in xs]
    if any(x is False for x in xs):
        return False
    if all(x is True for x in xs):
        return True
    return None


def OR(*xs):
    xs = [tv(x) for x in xs]
    if any(x is True for x in xs):
        return True
    if all(x is False for x in xs):
        return False
    return None


def NOT(x):
    x = tv(x)
    return None if x is None else (not x)


def EQ(a, b):
    if a is None or b is None or isbad(a) or isbad(b):
        return None
    return a == b


# ---------------------------------------------------------------- decoded expectation codes

VERDICTS = ["success", "refused: path", "refused: manifest", "refused: downgrade", "refused: update required",
            "refused: system requirements", "failed: dependency", "failed: Activate phase",
            "failed after the load completed (CleanUp)"]
SLOTS = ["top", "dep1", "dep2"]


def ver_ord(s):
    """explore.rs ver_str inverse: '2.0.0' -> 2, '0.0.0-0 (placeholder)' -> 0."""
    if s == "0.0.0-0 (placeholder)":
        return 0
    major, minor, patch = s.split(".")
    if minor != "0" or patch != "0" or not major.isdigit():
        raise ValueError(f"not a model version: {s!r}")
    return int(major)


def ver_str(v):
    return "0.0.0-0 (placeholder)" if v == 0 else f"{v}.0.0"


def tri_dec(x):
    return {False: 0, True: 1, "any": 2}[x]


def tri_enc(x):
    return {0: False, 1: True, 2: "any"}[x]


def decode_mexp(spec):
    """Inverse of explore.rs mexp_json. Returns (kind, ver, dev)."""
    if spec == "unchanged":
        return (0, 0, 2)
    if spec == "any (partial changes allowed)":
        return (3, 0, 2)
    keys = set(spec)
    dev = tri_dec(spec["developer_mode"])
    if keys == {"installed", "developer_mode"}:
        return (1, ver_ord(spec["installed"]), dev)
    if keys == {"manifest_only", "developer_mode", "resources"} and spec["resources"] == "as before":
        return (2, ver_ord(spec["manifest_only"]), dev)
    if keys == {"manifest_present", "developer_mode", "resources"} and spec["resources"] == "any":
        return (4, ver_ord(spec["manifest_present"]), dev)
    if keys == {"unchanged_or_present", "developer_mode"}:
        return (5, ver_ord(spec["unchanged_or_present"]), dev)
    if keys == {"unchanged_or_installed", "developer_mode"}:
        return (6, ver_ord(spec["unchanged_or_installed"]), dev)
    raise ValueError(f"unknown module expectation {spec!r}")


def encode_mexp(e):
    kind, ver, dev = e
    d = tri_enc(dev)
    if kind == 0:
        return "unchanged"
    if kind == 1:
        return {"installed": ver_str(ver), "developer_mode": d}
    if kind == 2:
        return {"manifest_only": ver_str(ver), "developer_mode": d, "resources": "as before"}
    if kind == 3:
        return "any (partial changes allowed)"
    if kind == 4:
        return {"manifest_present": ver_str(ver), "developer_mode": d, "resources": "any"}
    if kind == 5:
        return {"unchanged_or_present": ver_str(ver), "developer_mode": d}
    return {"unchanged_or_installed": ver_str(ver), "developer_mode": d}


QUALIFIED_ROW = "required unless Q alternative; may be missing"
OK_DEC = {"0 (in progress)": 0, "OK": 1, "error": 2, "any": 3}
COM_DEC = {0: 0, 1: 1, "any": 2, "equals whether the action's changes were kept": 3}
PHST_DEC = {"open": 0, "OK": 1, "error": 2, "any": 255}


def inv(d):
    return {v: k for k, v in d.items()}


def decode_rexp(r):
    """Inverse of explore.rs rexp_json. Returns a dict with the RExp fields (contract.rs:66)."""
    if r == {"row": False}:
        # norec(): only `ex` matters, because rec_exists_ok requires no row and rec_fields_ok is
        # vacuous without one.
        return {"ex": 0, "ok": 3, "com": 2, "tend": 2, "fin": 2, "pkg": 2, "ph": 255, "phst": 255}
    if set(r) != {"row", "success", "committed", "time_end_set", "finalized", "package_set", "phases_started", "last_phase_status"}:
        raise ValueError(f"unknown record expectation keys {sorted(r)}")
    ex = 1 if r["row"] is True else (2 if r["row"] == QUALIFIED_ROW else None)
    if ex is None:
        raise ValueError(f"unknown row expectation {r['row']!r}")
    ph = r["phases_started"]
    return {"ex": ex, "ok": OK_DEC[r["success"]], "com": COM_DEC[r["committed"]], "tend": tri_dec(r["time_end_set"]),
            "fin": tri_dec(r["finalized"]), "pkg": tri_dec(r["package_set"]), "ph": 255 if ph == "any" else int(ph),
            "phst": PHST_DEC[r["last_phase_status"]]}


def encode_rexp(e):
    if e["ex"] == 0:
        return {"row": False}
    return {"row": True if e["ex"] == 1 else QUALIFIED_ROW, "success": inv(OK_DEC)[e["ok"]],
            "committed": inv(COM_DEC)[e["com"]], "time_end_set": tri_enc(e["tend"]), "finalized": tri_enc(e["fin"]),
            "package_set": tri_enc(e["pkg"]), "phases_started": "any" if e["ph"] == 255 else e["ph"],
            "last_phase_status": inv(PHST_DEC)[e["phst"]]}


def decode_expect(l1):
    """Inverse of explore.rs expect_json, for the slots the case renders."""
    st = {"any": 0, "OK": 1, "error": 2}[l1["status"]]
    alts = [{s: decode_mexp(spec) for s, spec in a.items()} for a in l1["modules_one_of"]]
    recs = {s: decode_rexp(r) for s, r in l1["history"].items()}
    return {"st": st, "alts": alts, "recs": recs, "verdict": VERDICTS.index(l1["verdict"])}


def encode_expect(e):
    return {"verdict": VERDICTS[e["verdict"]], "status": {0: "any", 1: "OK", 2: "error"}[e["st"]],
            "modules_one_of": [{s: encode_mexp(m) for s, m in a.items()} for a in e["alts"]],
            "history": {s: encode_rexp(r) for s, r in e["recs"].items()}}


# ---------------------------------------------------------------- projection of observations

ABSENT = "<absent>"   # field has no meaning while the module record is absent (see PROJECTION.md)
NOPHASE = "<no phase>"


def bit(v):
    if v is None:
        return None
    if v in ("1", "0"):
        return v == "1"
    return ("bad", v)   # never equal to a boolean: a malformed value fails rather than passes


def obs_ver(v, snapshot):
    """Native version string -> model ordinal. A snapshot dependency is published as 1.0.0+snapshot."""
    if v is None:
        return None
    if v == "<none>":
        return 0
    s = v
    if snapshot and s.endswith("+snapshot"):
        s = s[: -len("+snapshot")]
    parts = s.split(".")
    if len(parts) == 3 and parts[0].isdigit() and parts[1] == "0" and parts[2] == "0":
        return int(parts[0])
    return ("bad", v)


def project_mod(o, pre, slot, snapshot=False):
    """Mod (model.rs:52-64) for one slot. `pre` is 'before.' or ''.

    present  <- modules.S.manifest_present (%IPM_Storage.ModuleItem row exists)
    ver, dev <- modules.S.version, developer_mode (only while present)
    res      <- modules.S.resources_loaded (class <Pkg>.Main compiled; one class per fixture)
    doc      <- modules.S.zpm_document_version (<none> -> 0)
    wr       <- not observable; modules.S.x.last_updated is its proxy (only while present)
    """
    g = lambda k: o.get(f"{pre}modules.{slot}.{k}")
    present = bit(g("manifest_present"))
    m = {"present": present, "res": bit(g("resources_loaded")), "doc": obs_ver(g("zpm_document_version"), snapshot)}
    if present is True:
        m["ver"] = obs_ver(g("version"), snapshot)
        m["dev"] = bit(g("developer_mode"))
        m["wr_proxy"] = g("x.last_updated")
    elif present is False:
        m["ver"] = m["dev"] = m["wr_proxy"] = ABSENT
    else:
        m["ver"] = m["dev"] = m["wr_proxy"] = None
    return m


def mod_eq(i, m):
    """`m == i` over all six Mod fields (contract.rs:294, 300, 319-320).

    present, res and doc are compared directly. ver, dev and wr are compared while the record is
    present before and after; wr through LastUpdated. When the record is absent before and after,
    ver, dev and wr are equal in every reachable model state (PROJECTION.md, INV-ABSENT)."""
    base = AND(EQ(i["present"], m["present"]), EQ(i["res"], m["res"]))
    if base is False:
        return False
    if i["present"] is True and m["present"] is True:
        return AND(base, EQ(i["ver"], m["ver"]), EQ(i["dev"], m["dev"]), EQ(i["wr_proxy"], m["wr_proxy"]))
    return base


def dev_ok(e, d):
    """contract.rs:289"""
    if e == 2:
        return True
    if d is ABSENT:
        return False
    return EQ(d, e == 1)


def present_at(e_ver, m):
    if m["present"] is False:
        return False
    return AND(m["present"], EQ(m["ver"], e_ver))


def installed_ok(e, m):
    """contract.rs:291 m.present && m.ver == e.ver && m.res && dev_ok(e.dev, m.dev)"""
    kind, ver, dev = e
    p = present_at(ver, m)
    if p is False:
        return False
    return AND(p, m["res"], dev_ok(dev, m["dev"]))


def mexp_ok(e, i, m):
    """contract.rs:293-301"""
    kind, ver, dev = e
    if kind == 0:
        return mod_eq(i, m)
    if kind == 1:
        return installed_ok(e, m)
    if kind == 2:
        p = present_at(ver, m)
        if p is False:
            return False
        return AND(p, EQ(m["doc"], ver), EQ(m["res"], i["res"]), dev_ok(dev, m["dev"]))
    if kind == 3:
        return True
    if kind == 4:
        p = present_at(ver, m)
        if p is False:
            return False
        return AND(p, dev_ok(dev, m["dev"]))
    if kind == 5:
        p = present_at(ver, m)
        return OR(mod_eq(i, m), AND(p, dev_ok(dev, m["dev"])) if p is not False else False)
    return OR(mod_eq(i, m), installed_ok(e, m))


def alt_ok(alt, before, after, slots):
    """contract.rs:305-307 over the slots realized natively. Returns (verdict, per-slot detail)."""
    detail = {s: mexp_ok(alt[s], before[s], after[s]) for s in slots if s in alt}
    missing = [s for s in slots if s not in alt]
    if missing:
        # The rendering omits slots beyond the case's dependencies; a realized slot without an
        # expectation is a binding error, never a pass.
        return None, {**detail, **{s: "no expectation for a realized slot" for s in missing}}
    return AND(*detail.values()), detail


def alts_ok(alts, before, after, slots):
    """contract.rs:311-313: one of the first three alternatives holds."""
    per = [alt_ok(a, before, after, slots) for a in alts[:3]]
    return OR(*[v for v, _ in per]) if per else False, per


def project_twin(o, slot):
    """Twin (model.rs:91-110) from %IPM_General.History, normalized by the driver."""
    g = lambda k: o.get(f"history.{slot}.{k}")
    ex = bit(g("row"))
    ok = {"OK": 1, "ERR": 2, "ZERO": 0}.get(g("success"), None if g("success") is None else ("bad", g("success")))
    phs = g("last_phase_status")
    phst = {"OPEN": 0, "OK": 1, "ERR": 2, "<none>": NOPHASE}.get(phs, None if phs is None else ("bad", phs))
    ph = g("phases_started")
    return {"ex": ex, "ok": ok, "com": bit(g("committed")), "tend": bit(g("time_end_set")), "fin": bit(g("finalized")),
            "pkg": bit(g("package_set")), "ph": (int(ph) if ph is not None and ph.isdigit() else (None if ph is None else ("bad", ph))),
            "phst": phst}


def v3(e, b):
    """contract.rs:315"""
    return True if e == 2 else EQ(b, e == 1)


def rec_exists_ok(e, t):
    """contract.rs:324-326"""
    if e["ex"] == 0:
        return NOT(t["ex"])
    if e["ex"] == 1:
        return t["ex"]
    return True


def rec_fields_ok(e, t):
    """contract.rs:329-333, with the failing sub-fields."""
    parts = {
        "success": True if e["ok"] == 3 else EQ(t["ok"], e["ok"]),
        "time_end_set": v3(e["tend"], t["tend"]),
        "finalized": v3(e["fin"], t["fin"]),
        "package_set": v3(e["pkg"], t["pkg"]),
        "phases_started": True if e["ph"] == 255 else EQ(t["ph"], e["ph"]),
        "last_phase_status": True if e["phst"] == 255 else EQ(t["phst"], e["phst"]),
    }
    return OR(NOT(t["ex"]), AND(*parts.values())), parts


def rec_com_ok(e, t, k):
    """contract.rs:336-338"""
    return OR(NOT(t["ex"]), e["com"] == 2, AND(e["com"] == 3, EQ(t["com"], k)), AND(e["com"] <= 1, EQ(t["com"], e["com"] == 1)))


def kept(before, after, slots, i):
    """contract.rs:318-321: the top action kept changes if any module changed; dependency i only its own."""
    if i == "top":
        return OR(*[NOT(mod_eq(before[s], after[s])) for s in slots])
    return NOT(mod_eq(before[i], after[i]))


def st_ok(e, o):
    """contract.rs:346-348. `status` is OK or ERR after a return, <none> after a process end."""
    if e["st"] == 0:
        return True
    s = o.get("status")
    if s is None or s == "<none>":
        return None
    return EQ(s == "OK", e["st"] == 1)


def l1_clauses(expect, o, slots, snapshot_slots=()):
    """Evaluate every L1 clause with the identities used by explore.rs violated_clauses.

    Returns {clause: {"ok": True/False/None, "detail": ...}}."""
    before = {s: project_mod(o, "before.", s, s in snapshot_slots) for s in slots}
    after = {s: project_mod(o, "", s, s in snapshot_slots) for s in slots}
    out = {}
    out["L1-STATUS"] = {"ok": st_ok(expect, o), "detail": {"required": expect["st"], "observed": o.get("status")}}
    ns, per = alts_ok(expect["alts"], before, after, slots)
    out["L1-NAMESPACE"] = {"ok": ns, "detail": {"alternatives": [{"ok": v, "slots": d} for v, d in per],
                                                "alternatives_beyond_third_ignored": max(0, len(expect["alts"]) - 3)}}
    for s in slots:
        e = expect["recs"].get(s)
        if e is None:
            out[f"H-RECORD-EXISTS[{s}]"] = {"ok": None, "detail": "no expectation for a realized slot"}
            continue
        t = project_twin(o, s)
        out[f"H-RECORD-EXISTS[{s}]"] = {"ok": rec_exists_ok(e, t), "detail": {"required_ex": e["ex"], "row": t["ex"]}}
        f, parts = rec_fields_ok(e, t)
        out[f"H-RECORD-FIELDS[{s}]"] = {"ok": f, "detail": {k: v for k, v in parts.items()}}
        k = kept(before, after, slots, s)
        out[f"H-COMMITTED[{s}]"] = {"ok": rec_com_ok(e, t, k), "detail": {"required_com": e["com"], "committed": t["com"], "kept": k}}
    return out
