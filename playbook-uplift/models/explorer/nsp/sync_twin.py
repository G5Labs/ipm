"""Literal transcription of the spec functions in models/verus/sync_baseline.rs (same names, same branches).

M = dict(files={p: F}, dev=bool); F = dict(disk, server, hash, failed, pending, owned, test).
"""
from __future__ import annotations


def upd(f, **kw):
    x = dict(f)
    x.update(kw)
    return x


def has_baseline(m):
    return any(f["hash"] is not None for f in m["files"].values())


def detected(f, run_tests):
    return f["owned"] and (f["hash"] != f["disk"] or f["failed"] or (run_tests and f["pending"]))


def any_detected(m, run_tests):
    return any(detected(f, run_tests) for f in m["files"].values())


def stamp_all(m):
    return dict(m, files={p: upd(f, hash=f["disk"]) for p, f in m["files"].items()})


def commit_one(f, run_tests):
    if detected(f, run_tests):
        return upd(f, server=f["disk"], hash=f["disk"], failed=False,
                   pending=False if run_tests else (f["pending"] or f["test"]))
    return f


def fail_one(f, run_tests, was_loaded):
    if detected(f, run_tests):
        return upd(f, server=f["disk"] if was_loaded else f["server"], failed=f["failed"] or f["hash"] is not None)
    return f


def sync_state(m, run_tests, ok, loaded):
    if not m["dev"]:
        return m
    if not has_baseline(m):
        return stamp_all(m)
    if not any_detected(m, run_tests):
        return m
    if ok:
        return dict(m, files={p: commit_one(f, run_tests) for p, f in m["files"].items()})
    return dict(m, files={p: fail_one(f, run_tests, p in loaded) for p, f in m["files"].items()})


def sync_ok(m, run_tests, ok, tests_ok):
    if not m["dev"]:
        return False
    if not has_baseline(m) or not any_detected(m, run_tests):
        return True
    return ok and (not run_tests or tests_ok)


def compile_dev(m, ok, skip, loaded):
    m1 = dict(m, files={p: (upd(f, server=f["disk"]) if f["owned"] and (ok or p in loaded) else f) for p, f in m["files"].items()})
    return stamp_all(m1) if ok and not skip else m1


def reload(m):
    if not m["dev"]:
        return m
    return dict(m, files={p: (upd(f, server=f["disk"]) if f["owned"] else f) for p, f in m["files"].items()})


def edit(m, p, b):
    if p in m["files"]:
        files = dict(m["files"])
        files[p] = upd(files[p], disk=b)
        return dict(m, files=files)
    return m


def inv_f(f):
    return not (f["owned"] and f["hash"] is not None and not f["failed"]) or f["server"] == f["hash"]


def inv(m):
    return all(inv_f(f) for f in m["files"].values())


def premise_out(m, ok, skip, loaded):
    if ok and not skip:
        return True
    return all(f["hash"] == f["disk"] for p, f in m["files"].items()
               if f["owned"] and (ok or p in loaded) and f["hash"] is not None and not f["failed"])


def premise_out_reload(m):
    return (not m["dev"]) or all(f["hash"] == f["disk"] for f in m["files"].values()
                                 if f["owned"] and f["hash"] is not None and not f["failed"])


def premise_base(m):
    return not (m["dev"] and not has_baseline(m)) or all(f["server"] == f["disk"] for f in m["files"].values() if f["owned"])


def abs_(m):
    return {p: dict(disk=f["disk"], server=f["server"], owned=f["owned"]) for p, f in m["files"].items()}


def l1_load(a, b):
    return set(a) == set(b) and all(b[p]["disk"] == a[p]["disk"] and b[p]["owned"] == a[p]["owned"]
                                    and (b[p]["server"] == a[p]["server"] or b[p]["server"] == a[p]["disk"]) for p in a)


def l1_sync(a, b, ok):
    return l1_load(a, b) and (not ok or all(b[p]["server"] == b[p]["disk"] for p in b if b[p]["owned"]))


def sync_ran(m, run_tests):
    return has_baseline(m) and any_detected(m, run_tests)


def l1_outcome(dev, ran, ok, run_tests, tests_ok, reported):
    """L1 outcome, both directions (REVIEW finding 1): success exactly when in developer mode and every step run worked."""
    return ((dev or not reported)
            and (not (dev and ran and not ok) or not reported)
            and (not (dev and ran and run_tests and not tests_ok) or not reported)
            and (not (dev and (not ran or (ok and (not run_tests or tests_ok)))) or reported))


def l1_edit(a, b, p, x):
    if p in a:
        c = dict(a)
        c[p] = dict(a[p], disk=x)
        return b == c
    return b == a
