"""Obligation family R1-C (no member credit): a load whose second class resource fails to compile.
Native run `20261114T030000Z-r1-two-resources`. Distinguishes "every resource usable" from "first resource usable"
and shows that the existence of a compiled-class descriptor does not mean the class runs.
Members whose behavior decides these outcomes (Utils/Module.cls::LoadNewModule commit points, the IRIS compiler) are
already in the baseline or outside the product; this family adds obligations and a checker control, not members."""
ROUND = 'R1'
FAMILY = 'partial-compile'
RUNS = ['20261114T030000Z-r1-two-resources']
MEMBERS = {}
VARIANTS = {
    'compiled-means-usable': {'member': None, 'wrong': 'projection: a compiled-class descriptor means the class runs'},
    'everything-rolls-back': {'member': None, 'wrong': 'impossibly broad rollback: every mode removes both classes on failure'},
    'stop-at-first-error': {'member': None, 'wrong': 'compilation stops before the remaining resources (A missing)'},
    'txn-keeps-first': {'member': None, 'wrong': 'transactional mode keeps the class that compiled'},
}
ERR = 'ERROR #5475: Error compiling routine: TwoRes{P}.B.1.  Errors:  ERROR: TwoRes{P}.B.cls(Bad+2) #1054: Invalid expression : \'}}\' : Offset:13 [Bad+1^TwoRes{P}.B.1]'
def predict(var):
    out = {}
    for case, mode, pkg in (('R2txn', 'txn', 'Txn'), ('R2notxn', 'notxn', 'Notxn'), ('R2dev', 'dev', 'Dev'), ('R2ok', 'ok', 'Ok')):
        bad = mode != 'ok'
        kept = mode in ('notxn', 'dev', 'ok') and not (var == 'everything-rolls-back' and bad)
        if var == 'txn-keeps-first' and mode == 'txn': kept = True
        out[(case, 'status')] = 'ERR' if bad else 'OK'
        if bad: out[(case, 'error')] = ERR.format(P=pkg)
        out[(case, 'tlevel.before')] = '0'; out[(case, 'tlevel.after')] = '0'
        name = 'tworec-' + mode
        out[(case, 'mod.' + name)] = f"present version=1.0.0 repo= desc= dev={1 if mode == 'dev' else 0}" if kept else 'absent'
        out[(case, 'hist.' + name)] = f"action:load success:{0 if bad else 1} committed:{1 if (kept and not (var == 'txn-keeps-first' and mode == 'txn')) else 0} finalized:1"
        a = kept and not (var == 'stop-at-first-error' and bad and False)
        out[(case, f'cls.TwoRes{pkg}.A')] = f"def:{int(kept)} compiled:{int(kept)} runs:{1 if kept else 0}"
        if var == 'stop-at-first-error' and bad and kept:
            out[(case, f'cls.TwoRes{pkg}.A')] = 'def:1 compiled:0 runs:0'
        brun = ('2' if not bad else ('1' if var == 'compiled-means-usable' else 'err')) if kept else '0'
        bkept = kept and not (var == 'txn-keeps-first' and mode == 'txn')
        out[(case, f'cls.TwoRes{pkg}.B')] = f"def:{int(bkept)} compiled:{int(bkept)} runs:{brun if bkept else 0}"
    return out
def _all_usable_or_none(p):
    bad = [c for c in ('R2notxn', 'R2dev') if p[(c, 'status')] == 'ERR' and p[(c, f"cls.TwoRes{c[2:].capitalize()}.A")].endswith('runs:1')]
    return (not bad, bad)
CONTRACT = [('a failed load leaves either every resource usable or none (atomic resource set)', _all_usable_or_none, False, ['G:D4-family', 'P:F-NSP-12-family'])]
