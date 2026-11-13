#!/usr/bin/env python3
"""Bounded model of the history consumers: find, details, delete, global variants, GlobalRecords and
retention purge. Run with python3 -I -B:  python3 -I -B history_readers.py OUT_DIR

As built (571e04a1, src/cls/IPM/General/History.cls unless stated):
  filter      `history` passes every -D key/value (and, per native H2, the namespace's default
              modifiers) as filter(col)=value (Main.cls:4135). ConstructSQLWhere (:93-126) walks the
              keys in collation order and STOPS at the first key with no value or a name that is not
              ^[a-zA-Z][a-zA-Z0-9]*$ (an underscore column such as Version_Major). Earlier keys become
              conjuncts: >=, <=, <> / >, <, = / * -> LIKE / equality. An unknown column is an SQL error.
  find        GetHistory (:132-147): SELECT [TOP limit] ... WHERE ... ORDER BY ID DESC|ASC in the
              current namespace. An SQL error shows no rows and returns OK (native H2).
  delete      Main.History (:4161-4173): refuses only when no key was given, not -globally and not
              -confirm. DeleteHistory (:151-159) runs DELETE with the same WHERE; an SQL error throws.
  global      DeleteHistoryGlobally (:199-230) runs DeleteHistory in every namespace that has the class
              and ignores every exception there. GetHistoryGlobally (:295-317) collects one result set per
              namespace; `find -globally` displays each with its namespace (native H10). The GlobalRecords
              query (:233-293) fetches every namespace's result set in the caller's namespace, so it
              returns the caller's rows once per namespace (native H10). Its ROWSPEC has no namespace and
              no ID column.
  details     DisplayOneRecord(id) reads the current namespace. IDs restart in each namespace.
  purge       Shell (Main.cls:862) calls AutoPurgeOldHistory (:162-181) before every command: skipped
              if it ran today or HistoryRetain <= 0 (instance-wide, %SYS ^IPM.settings); otherwise
              filter TimeStart = "<='" _ cutoff _ "'" in every namespace. The quotes become part of the
              compared value (premise N-R04-DATEADD decides the cutoff's format).

Contract (L1) used here, with open choices as parameters:
  SEL       the selection is exactly the rows in scope that satisfy every given filter on a real
            column. A key that is not a column (or a default modifier, Q-18) is refused, not dropped.
  DEL       delete removes exactly the selection and nothing else, and refuses a local delete with no
            effective filter unless -confirm. QG: a global delete with no filter (documented as
            needing no -confirm) is permitted; errors other than permission in one namespace are
            reported (QE: alternative = silently skipped).
  PURGE     with HistoryRetain = n > 0, purge removes exactly the rows started more than n days ago,
            in every namespace, at most once a day. QA: in-progress rows are preserved (alternative:
            removed like any other).
  GLOBAL    global readers return every namespace's rows once, each attributed to its namespace.
  DETAILS   details(id) shows the current namespace's row with that ID.
"""
import itertools, json, os, re, sys

TODAY = "2026-11-13 12:00:00"
OLD, RECENT = "2026-09-01 10:00:00", "2026-11-13 10:00:00"
COLUMNS = {"action", "package", "version_major", "version_minor", "timestart", "timeend", "success", "committed", "finalized", "username", "commandstring"}

# Distinguishable records. Each namespace numbers its rows from 1 (IDs collide across namespaces).
DB = {
    "USER": [
        {"ID": 1, "Package": "a", "Version_Major": 1, "Action": "load", "TimeStart": OLD, "Finalized": 1, "Committed": 1},
        {"ID": 2, "Package": "b", "Version_Major": 2, "Action": "install", "TimeStart": RECENT, "Finalized": 1, "Committed": 0},
        {"ID": 3, "Package": "a", "Version_Major": 2, "Action": "load", "TimeStart": OLD, "Finalized": 0, "Committed": 0},   # in progress
    ],
    "OTHER": [
        {"ID": 1, "Package": "b", "Version_Major": 1, "Action": "load", "TimeStart": RECENT, "Finalized": 1, "Committed": 1},
        {"ID": 2, "Package": "a", "Version_Major": 1, "Action": "load", "TimeStart": OLD, "Finalized": 1, "Committed": 1},
    ],
    "LOCKED": [
        {"ID": 1, "Package": "a", "Version_Major": 1, "Action": "load", "TimeStart": OLD, "Finalized": 1, "Committed": 1},
    ],
    "NOIPM": None,     # namespace without the class
}
CURRENT = "USER"


# ---------------------------------------------------------------- SQL predicate shared by both levels


def sql_match(row, col, op, val):
    c = {k.lower(): k for k in row}
    if col.lower() not in c:
        raise LookupError(col)
    v = row[c[col.lower()]]
    if isinstance(v, int):
        try:
            x = int(val)
        except ValueError:
            return False
    else:
        x = val
    if op == "LIKE":
        return re.fullmatch(re.escape(val).replace("%", ".*"), str(v)) is not None
    return {"=": v == x, "<>": v != x, ">": v > x, "<": v < x, ">=": v >= x, "<=": v <= x}[op]


def parse_value(value):
    for op in (">=", "<=", "<>"):
        if value.startswith(op):
            return op, value[2:]
    for op in (">", "<", "="):
        if value.startswith(op):
            return op, value[1:]
    if "*" in value:
        return "LIKE", value.replace("*", "%")
    return "=", value


# ---------------------------------------------------------------- as built (L2)


def where_as_built(filt, variant):
    """ConstructSQLWhere: conjuncts in collation order, stopping at the first unusable key."""
    out = []
    for col in sorted(filt):
        value = filt[col]
        if value is None or not re.fullmatch(r"[a-zA-Z][a-zA-Z0-9]*", col):
            if variant == "M-SKIP-INVALID":
                continue
            break
        op, v = parse_value(value)
        out.append((col, op, v))
    return out


def select(rows, conj):
    return [r for r in rows if all(sql_match(r, c, o, v) for c, o, v in conj)]


def as_built(db, cmd, variant="as-built"):
    """Returns {"status": ok|error, "rows": [(ns, id)], "deleted": {(ns, id)}}."""
    kind = cmd["kind"]
    filt = dict(cmd.get("filter", {}))
    filt.update(cmd.get("defaults", {}))         # native H2: default modifiers reach the filter
    conj = where_as_built(filt, variant)
    if kind == "find":
        rows = db[CURRENT]
        try:
            sel = select(rows, conj)
        except LookupError:
            return {"status": "ok", "rows": [], "deleted": set()}
        sel = sorted(sel, key=lambda r: r["ID"], reverse=not cmd.get("asc"))
        if cmd.get("limit"):
            sel = sel[: cmd["limit"]]
        return {"status": "ok", "rows": [(CURRENT, r["ID"]) for r in sel], "deleted": set()}
    if kind == "find-global":
        out = []
        for ns in sorted(db):
            if db[ns] is None:
                continue
            try:
                sel = sorted(select(db[ns], conj), key=lambda r: r["ID"], reverse=not cmd.get("asc"))
            except LookupError:
                sel = []
            out += [(ns, r["ID"]) for r in (sel[: cmd["limit"]] if cmd.get("limit") else sel)]
        return {"status": "ok", "rows": out, "deleted": set()}
    if kind == "globalrecords":
        out = []
        for ns in sorted(db):
            if db[ns] is None:
                continue
            out += [(None, r["ID"]) for r in sorted(db[CURRENT], key=lambda r: -r["ID"])]   # caller's rows, unattributed
        return {"status": "ok", "rows": out, "deleted": set()}
    if kind == "details":
        r = [x for x in db[CURRENT] if x["ID"] == cmd["id"]]
        return {"status": "ok" if r else "error", "rows": [(CURRENT, cmd["id"])] if r else [], "deleted": set()}
    if kind == "delete":
        if not filt and not cmd.get("confirm"):
            return {"status": "error", "rows": [], "deleted": set()}
        try:
            sel = select(db[CURRENT], conj)
        except LookupError:
            return {"status": "error", "rows": [], "deleted": set()}
        return {"status": "ok", "rows": [], "deleted": {(CURRENT, r["ID"]) for r in sel}}
    if kind == "delete-global":
        gone = set()
        for ns in sorted(db):
            if db[ns] is None:
                continue
            if ns == "LOCKED":
                continue                                   # permission error, ignored
            try:
                gone |= {(ns, r["ID"]) for r in select(db[ns], conj)}
            except LookupError:
                continue                                   # any error ignored
        return {"status": "ok", "rows": [], "deleted": gone}
    if kind == "purge":
        if cmd["retain"] <= 0 or cmd.get("ran_today"):
            return {"status": "ok", "rows": [], "deleted": set()}
        cutoff = cmd["cutoff"]
        value = "'" + cutoff + "'" if variant != "M-NO-QUOTES" else cutoff
        gone = set()
        for ns in sorted(db):
            if db[ns] is None or ns == "LOCKED":
                continue
            gone |= {(ns, r["ID"]) for r in db[ns] if r["TimeStart"] <= value}
        return {"status": "ok", "rows": [], "deleted": gone}
    raise ValueError(kind)


# ---------------------------------------------------------------- contract (L1)


def contract(db, cmd, qa="preserve-active", qe="report"):
    """Returns the required {"status", "rows", "deleted"}; rows/deleted may be None (unconstrained)."""
    kind = cmd["kind"]
    filt = dict(cmd.get("filter", {}))
    bad = [k for k in list(filt) + list(cmd.get("defaults", {})) if k.lower() not in COLUMNS]
    conj = [(k, *parse_value(v)) for k, v in filt.items() if k.lower() in COLUMNS]
    if kind in ("find", "find-global", "delete", "delete-global") and bad:
        return {"status": "error", "rows": [], "deleted": set()}
    if kind == "find":
        sel = sorted(select(db[CURRENT], conj), key=lambda r: r["ID"], reverse=not cmd.get("asc"))
        return {"status": "ok", "rows": [(CURRENT, r["ID"]) for r in (sel[: cmd["limit"]] if cmd.get("limit") else sel)], "deleted": set()}
    if kind == "find-global":
        out = []
        for ns in sorted(db):
            if db[ns] is not None:
                sel = sorted(select(db[ns], conj), key=lambda r: r["ID"], reverse=not cmd.get("asc"))
                out += [(ns, r["ID"]) for r in (sel[: cmd["limit"]] if cmd.get("limit") else sel)]
        return {"status": "ok", "rows": out, "deleted": set()}
    if kind == "globalrecords":
        return {"status": "ok", "rows": [(ns, r["ID"]) for ns in sorted(db) if db[ns] is not None for r in sorted(db[ns], key=lambda r: -r["ID"])], "deleted": set()}
    if kind == "details":
        r = [x for x in db[CURRENT] if x["ID"] == cmd["id"]]
        return {"status": "ok" if r else "error", "rows": [(CURRENT, cmd["id"])] if r else [], "deleted": set()}
    if kind == "delete":
        if not conj and not cmd.get("confirm"):
            return {"status": "error", "rows": [], "deleted": set()}
        return {"status": "ok", "rows": [], "deleted": {(CURRENT, r["ID"]) for r in select(db[CURRENT], conj)}}
    if kind == "delete-global":
        gone = {(ns, r["ID"]) for ns in sorted(db) if db[ns] is not None and ns != "LOCKED" for r in select(db[ns], conj)}
        return {"status": "ok", "rows": [], "deleted": gone}
    if kind == "purge":
        if cmd["retain"] <= 0 or cmd.get("ran_today"):
            return {"status": "ok", "rows": [], "deleted": set()}
        gone = {(ns, r["ID"]) for ns in sorted(db) if db[ns] is not None and ns != "LOCKED" for r in db[ns]
                if r["TimeStart"] <= cmd["cutoff"] and (qa != "preserve-active" or r["Finalized"])}
        return {"status": "ok", "rows": [], "deleted": gone}
    raise ValueError(kind)


# ---------------------------------------------------------------- domain


KEYS = ["Package", "package", "Version_Major", "Action", "NoSuchCol", "Committed"]
VALUES = {"Package": ["a", "*"], "package": ["b"], "Version_Major": ["1", ">=2"], "Action": ["load"], "NoSuchCol": ["1"], "Committed": ["1", "<>1"]}


def filters(max_keys=2):
    yield {}
    for n in range(1, max_keys + 1):
        for keys in itertools.combinations(KEYS, n):
            for vals in itertools.product(*[VALUES[k] for k in keys]):
                yield dict(zip(keys, vals))


def commands():
    for f in filters():
        for defaults in ({}, {"NoTransaction": "1"}):
            yield {"kind": "find", "filter": f, "defaults": defaults}
            yield {"kind": "find", "filter": f, "defaults": defaults, "asc": True, "limit": 1}
            yield {"kind": "find-global", "filter": f, "defaults": defaults}
            for confirm in (False, True):
                yield {"kind": "delete", "filter": f, "defaults": defaults, "confirm": confirm}
            yield {"kind": "delete-global", "filter": f, "defaults": defaults}
    yield {"kind": "globalrecords"}
    for i in (1, 2, 3):
        yield {"kind": "details", "id": i}
    for retain in (0, 30):
        for ran in (False, True):
            yield {"kind": "purge", "retain": retain, "ran_today": ran, "cutoff": "2026-10-14 12:00:00"}


def compare(got, want):
    v = []
    if got["status"] != want["status"]:
        v.append("STATUS")
    if want["rows"] is not None and got["rows"] != want["rows"]:
        v.append("ROWS")
    if want["deleted"] is not None:
        if got["deleted"] - want["deleted"]:
            v.append("DELETED-UNSELECTED")       # an unrelated row removed: data loss
        if want["deleted"] - got["deleted"]:
            v.append("PRESERVED-SELECTED")       # a selected row kept
    return v


def explore(variant, qa="preserve-active"):
    out = {"commands": 0, "violations": {}, "examples": {}, "clean_neighbors": 0}
    for cmd in commands():
        out["commands"] += 1
        got = as_built(DB, cmd, variant)
        want = contract(DB, cmd, qa)
        v = compare(got, want)
        if not v:
            out["clean_neighbors"] += 1
        for x in v:
            key = f"{cmd['kind']}:{x}"
            out["violations"][key] = out["violations"].get(key, 0) + 1
            ex = out["examples"].setdefault(key, [])
            if len(ex) < 3:
                ex.append({"cmd": cmd, "as_built": {k: sorted(v) if isinstance(v, set) else v for k, v in got.items()},
                           "contract": {k: sorted(v) if isinstance(v, set) else v for k, v in want.items()}})
    return out


def archived_correspondence():
    """Archived native observations and what the as-built model predicts for the same command."""
    rows = []
    # X2 (20261113T184621Z-repro-history-filter-widened): GetHistory with filter Version_Major=1 returned every row.
    db = {"USER": [{"ID": 1, "Package": "p", "Version_Major": 1, "Action": "load", "TimeStart": RECENT, "Finalized": 1, "Committed": 1},
                   {"ID": 2, "Package": "q", "Version_Major": 2, "Action": "load", "TimeStart": RECENT, "Finalized": 1, "Committed": 1}]}
    for variant in ("as-built", "M-SKIP-INVALID"):
        got = as_built(db, {"kind": "find", "filter": {"Version_Major": "1"}}, variant)
        rows.append({"evidence": "X2 find -DVersion_Major=1 returns every row", "variant": variant, "holds": len(got["rows"]) == 2})
        got = as_built(db, {"kind": "delete", "filter": {"Version_Major": "99"}}, variant)
        rows.append({"evidence": "X3 delete -DVersion_Major=99 deletes every row", "variant": variant, "holds": len(got["deleted"]) == 2})
        got = as_built(db, {"kind": "find", "filter": {"Version_Major": "1", "package": "q"}}, variant)
        rows.append({"evidence": "N-R04-SEL (new, read-only): find -DVersion_Major=1 -Dpackage=q (lowercase sorts after the underscore key)",
                     "variant": variant, "prediction_rows": len(got["rows"])})
    # X4 (20261113T184626Z-repro-autopurge-noop): a 40-day-old finalized row survives HistoryRetain 30.
    db = {"USER": [{"ID": 1, "Package": "p", "Version_Major": 1, "Action": "load", "TimeStart": "2026-10-04 00:00:00", "Finalized": 1, "Committed": 1}]}
    for variant in ("as-built", "M-NO-QUOTES"):
        got = as_built(db, {"kind": "purge", "retain": 30, "cutoff": "2026-10-14 12:00:00"}, variant)
        rows.append({"evidence": "X4 purge keeps a 40-day-old row", "variant": variant, "holds": not got["deleted"]})
    # H2 (20261113T204152Z-stagec): default modifier NoTransaction -> find shows nothing with OK, delete fails.
    db = {"USER": DB["USER"]}
    got = as_built(db, {"kind": "find", "filter": {}, "defaults": {"NoTransaction": "1"}})
    rows.append({"evidence": "H2 find with a default modifier shows no rows, status OK", "variant": "as-built", "holds": got == {"status": "ok", "rows": [], "deleted": set()}})
    got = as_built(db, {"kind": "delete", "filter": {}, "defaults": {"NoTransaction": "1"}, "confirm": True})
    rows.append({"evidence": "H2 delete -confirm with a default modifier fails", "variant": "as-built", "holds": got["status"] == "error"})
    # H10 (20261113T204152Z-stagec): GlobalRecords returns the caller's rows once per namespace with the class.
    got = as_built(DB, {"kind": "globalrecords"})
    rows.append({"evidence": "H10 GlobalRecords returns the caller's rows once per IPM namespace", "variant": "as-built",
                 "holds": len(got["rows"]) == 3 * len(DB["USER"]) and all(ns is None for ns, _ in got["rows"])})
    return rows


def main(argv):
    out = argv[0]
    os.makedirs(out, exist_ok=True)
    res = {"database": DB, "current": CURRENT, "bounds": "4 namespaces (one locked, one without IPM), 6 rows with colliding IDs, one in progress "
           "and three older than the cutoff; filters of up to two keys from 6 keys (incl. an underscore column, a case variant, an unknown "
           "column), 2-3 values each; default modifiers none or NoTransaction; find (two orders/limits), global find, local delete "
           "with and without -confirm, global delete, GlobalRecords, details 1-3, purge with retain 0/30 and ran-today true/false"}
    for variant in ("as-built", "M-SKIP-INVALID", "M-NO-QUOTES"):
        res[variant] = explore(variant)
    res["as-built (QA: active rows may be purged)"] = explore("as-built", "remove-active")
    res["archived_correspondence"] = archived_correspondence()
    # Valid neighbor: the same commands restricted to filters on real columns without underscores,
    # no default modifiers, are exactly the contract.
    clean = [c for c in commands() if c["kind"] in ("find", "delete", "find-global") and not c.get("defaults")
             and all(k in ("Package", "Action", "Committed") for k in c.get("filter", {})) and (c.get("filter") or c.get("confirm") or c["kind"] != "delete")]
    res["valid_neighbor"] = {"commands": len(clean), "violations": sum(1 for c in clean if compare(as_built(DB, c), contract(DB, c)))}
    json.dump(res, open(os.path.join(out, "history_readers.json"), "w"), indent=1, sort_keys=True, default=lambda o: sorted(o) if isinstance(o, set) else str(o))
    for k in ("as-built", "M-SKIP-INVALID", "M-NO-QUOTES", "as-built (QA: active rows may be purged)"):
        r = res[k]
        print(k, "commands", r["commands"], "clean", r["clean_neighbors"])
        for vk, n in sorted(r["violations"].items()):
            print("  ", vk, n)
    print("valid neighbor", res["valid_neighbor"])
    for r in res["archived_correspondence"]:
        print("ARCHIVED", r)
    return 0 if res["valid_neighbor"]["violations"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
