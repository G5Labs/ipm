#!/usr/bin/env python3
"""Reviewer scratch: does the order of JournalManager %OnClose at a Terminate change loadtx results?

  python3 -I -B loadtx_jm_order.py VV_DIR OUT_JSON

The follow-up's loadtx.run() closes the open History logs at a Terminate with whatever journaling
flag is in effect at the cut, and never runs a JournalManager's %OnClose before them. Premise P-06c
says live objects close innermost frame first. The frames are, innermost first:
  dependency LoadNewModule (its JournalManager if the dependency runs NoJournal)
  LoadModuleReference (the dependency's log)
  top LoadNewModule (its JournalManager if the top runs NoJournal)
  Main:Load (the top log)
JournalManager:%OnClose restores the flag it saw at %New (src/cls/IPM/Utils/JournalManager.cls:22-25).
This script loads loadtx.py unchanged, then re-runs explore() with a patched process-end that
interleaves those restores, and reports the per-clause differences. It also counts how many
violating end states are duplicates (same config, end kind and settled store as another cut).
"""
import importlib.util, json, os, sys

VV, OUT = os.path.abspath(sys.argv[1]), sys.argv[2]
spec = importlib.util.spec_from_file_location("loadtx", os.path.join(VV, "followup", "models", "loadtx.py"))
LT = importlib.util.module_from_spec(spec)
spec.loader.exec_module(LT)

ORIG_RUN = LT.run


def run_p06c(cfg, init, cut=None, end="return"):
    """Same as loadtx.run, except that a Terminate restores journaling innermost-first."""
    if cut is None or end != "terminate":
        return ORIG_RUN(cfg, init, cut, end)
    steps, acts, mode, uses_tx = LT.program(cfg)
    st = LT.Store(init)
    if cfg.variant == "M-BEFORE-IMAGE":
        st.before_image = False
    ctx = {"acts": acts, "started": set(), "failed": set(), "completed": set(), "itl": {}, "jprev": {}, "entry": 1 if cfg.caller_tx else 0,
           "kept_now": lambda st, a: True}
    i, unwinding, obs, trace = 0, None, {}, []
    while i < len(steps):
        if i == cut:
            obs["transient"] = st.snapshot()

            def jm_close(a):
                if mode(a)["NoJournal"] and f"{a}:journal-off" in trace and f"{a}:exit" not in trace:
                    st.jrn = ctx["jprev"][a]
            for a in reversed(acts):
                if a == "top":
                    continue
                jm_close(a)
                if a in st.temp:
                    LT.close(a, st, ctx, cfg.variant)
            jm_close("top")
            if "top" in st.temp:
                LT.close("top", st, ctx, cfg.variant)
            st.rollback_all()
            obs["settled"] = st.snapshot()
            obs["end"] = end
            obs["cut_label"] = steps[i][0]
            obs["trace"] = trace
            obs["ctx"] = {k: sorted(v) if isinstance(v, set) else v for k, v in ctx.items() if k != "kept_now"}
            return obs
        label, fn = steps[i]
        if unwinding is not None and fn != "EXIT:" + unwinding:
            i += 1
            continue
        trace.append(label)
        if isinstance(fn, str):
            if fn.startswith("FAULT:"):
                a = fn[6:]
                if cfg.fault == a:
                    ctx["failed"].add(a)
                    unwinding = a
            elif fn.startswith("THROW-IF-FAILED:"):
                d = fn[16:]
                if d in ctx["failed"]:
                    ctx["failed"].add("top")
                    unwinding = "top"
            elif fn.startswith("EXIT:"):
                a = fn[5:]
                while st.tl() > ctx["itl"][a]:
                    st.trollback1()
                if mode(a)["NoJournal"] and a in ctx["jprev"] and cfg.variant != "M-JOURNAL-NOT-RESTORED":
                    st.jrn = ctx["jprev"][a]
                if unwinding == a:
                    unwinding = None
        else:
            fn(st, ctx)
        i += 1
    raise AssertionError("cut beyond the trace")


def tally(variant, runner):
    """Per clause key: number of violating end states, number of distinct (config, end, settled) among them,
    and the list of (config, cut) for terminate ends (to diff the two runners)."""
    LT.run = runner
    counts, distinct, term = {}, {}, set()
    for cfg in LT.domain([variant]):
        steps, acts, _, _ = LT.program(cfg)
        ends = [("return", None)] + [(e, k) for k in range(1, len(steps)) for e in ("terminate", "sigkill")]
        for end, cut in ends:
            if end != "return" and cfg.caller_tx:
                continue
            obs = LT.run(cfg, {}, cut, end)
            for q22, q23 in [("A", "B")]:
                cl = LT.check(cfg, q22, q23, obs, acts)
                if end == "return" and cfg.caller_tx:
                    cl = sorted(set(cl) | {c + "@caller-rollback" for c in LT.check(cfg, q22, q23, obs, acts, "rollback")})
                sig = json.dumps(sorted((repr(k), repr(v)) for k, v in (obs.get("settled") or obs.get("returned"))["v"].items()))
                for c in cl:
                    counts[c] = counts.get(c, 0) + 1
                    distinct.setdefault(c, set()).add((cfg.name(), end, sig))
                    if end == "terminate":
                        term.add((cfg.name(), obs.get("cut_label"), c))
    LT.run = ORIG_RUN
    return counts, {k: len(v) for k, v in distinct.items()}, term


out = {}
for variant in ("as-built", "REPAIR-103", "REPAIR-103-GUARDED"):
    c0, d0, t0 = tally(variant, ORIG_RUN)
    c1, d1, t1 = tally(variant, run_p06c)
    agg = lambda c: {k: sum(v for kk, v in c.items() if kk.split("[")[0] == k) for k in sorted({kk.split("[")[0] for kk in c})}
    only_p06c = sorted(t1 - t0)
    only_orig = sorted(t0 - t1)
    out[variant] = {"followup_run": agg(c0), "followup_run_distinct_settled": agg(d0), "p06c_run": agg(c1), "p06c_run_distinct_settled": agg(d1),
                    "terminate_violations_only_with_p06c": len(only_p06c), "terminate_violations_only_in_followup": len(only_orig),
                    "examples_only_p06c": only_p06c[:4], "examples_only_followup": only_orig[:4]}
    print(variant, json.dumps(out[variant], indent=1))
json.dump(out, open(OUT, "w"), indent=1)
