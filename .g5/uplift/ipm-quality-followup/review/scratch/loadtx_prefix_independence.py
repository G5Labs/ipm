#!/usr/bin/env python3
"""Reviewer scratch: is PREFIX independent of the journal mechanics and of the trace? python3 -I -B loadtx_prefix_independence.py VV_DIR"""
import importlib.util, os, sys
from collections import Counter
VV = os.path.abspath(sys.argv[1])

def fresh():
    spec = importlib.util.spec_from_file_location("loadtx", os.path.join(VV, "followup", "models", "loadtx.py"))
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m); return m

def explore(LT):
    r = LT.explore(["as-built"], [("A", "B")])
    return {k.split(" ")[-1]: v for k, v in r["violations"].items()}, r["retry_failures"]

LT = fresh(); print("baseline            ", explore(LT))

# store mechanics mutants
LT = fresh()
def tcommit_durable(self):
    self.undo.pop()            # inner commit discards the undo entries: inner commits become durable
LT.Store.tcommit = tcommit_durable
print("inner-commit-durable", explore(LT))

LT = fresh()
LT.Store.rollback_all = lambda self: None      # no rollback at a process end
print("no-end-rollback     ", explore(LT))

LT = fresh()
orig = LT.Store.write
def write_unjournaled(self, node, val):        # every module write unjournaled (as if NoJournal everywhere)
    if node[0] == "mod":
        j = self.jrn; self.jrn = False; orig(self, node, val); self.jrn = j
    else:
        orig(self, node, val)
LT.Store.write = write_unjournaled
print("mod-writes-unjournal", explore(LT))

# program mutants: the top's final commit is a no-op, with its label kept / removed
for keep_label in (True, False):
    LT = fresh()
    P = LT.program
    def prog(cfg, P=P, keep_label=keep_label):
        steps, acts, mode, uses_tx = P(cfg)
        out = []
        for lab, fn in steps:
            if lab == "top:C3-commit":
                if keep_label:
                    out.append((lab, lambda st, ctx: None))
                continue_ = True
            else:
                out.append((lab, fn))
        return out, acts, mode, uses_tx
    LT.program = prog
    print(f"no-top-C3 label={'kept' if keep_label else 'gone'}", explore(LT))
