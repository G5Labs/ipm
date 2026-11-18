#!/usr/bin/env python3
"""Writes cases.json for iteration74: one-line mutants of actions.rs, dependency controls on the shared history.rs and
uninstall.rs, the native session, and the replay of every replayed native field against an actions.rs expression
whose model value is checked in the replay prelude. Run from this directory: python3 make_cases.py"""
import json
from pathlib import Path

here = Path(__file__).resolve().parent

mutants = {
 # Main.Install creates its record after the name check (API caller with no name gets none).
 "install-init-after-name-check": ["pub open spec fn api_install(i: InstallIn) -> Seq<Rec> { seq![main_install(i)] }",
                                   "pub open spec fn api_install(i: InstallIn) -> Seq<Rec> { if i.name.len() == 0 { seq![] } else { seq![main_install(i)] } }"],
 # UpdateInit before the -PermitDowngrade refusal / before the installed check.
 "update-init-before-permit-check": ["    else if u.permit_downgrade { seq![] }", "    else if false { seq![] }"],
 "update-init-before-installed-check": ["    else if !has_name(u.installed, n) { seq![] }", "    else if false { seq![] }"],
 # Reinstall of a name that is not installed reaches Install.
 "reinstall-enters-install": ["if i.name.len() == 0 { seq![] } else if !has_name(installed, i.name) { seq![] } else { cli_install(i) }",
                              "if i.name.len() == 0 { seq![] } else { cli_install(i) }"],
 # Uninstall marks its record committed at the start.
 "uninstall-sets-committed": ["    let r0 = init(h::Action::Uninstall, pkg_of(k));", "    let r0 = set_committed(init(h::Action::Uninstall, pkg_of(k)));"],
 # The top uninstall record is created after Clean (uninstall.rs's own order).
 "uninstall-top-created-last": ["seq![top_unin(w, k, o.ok)] + hs.drop_last().map_values(|hr: unin::HRow| dep_rec(hr))",
                                "hs.drop_last().map_values(|hr: unin::HRow| dep_rec(hr)) + seq![top_unin(w, k, o.ok)]"],
 # The record is created only once the module is found.
 "uninstall-init-after-lookup": ["seq![top_unin(w, k, o.ok)] + hs.drop_last()", "(if unin::lookup(w, k) is Some { seq![top_unin(w, k, o.ok)] } else { seq![] }) + hs.drop_last()"],
 # Uninstall with no name is recorded.
 "uninstall-no-name-recorded": ["        None => seq![],\n        Some(k) => {\n            let o = unin_out", "        None => seq![top_rec(0, false, false, false)],\n        Some(k) => {\n            let o = unin_out"],
 # Update runs the Activate chain only (no ApplyUpdateSteps).
 "update-without-apply-update-steps": ["let pr = run_from(run_list(if upd { Ph::ApplyUpdateSteps } else { Ph::Activate }), 0, l.phase_fail);", "let pr = run_from(run_list(Ph::Activate), 0, l.phase_fail);"],
 "chain-drops-apply-update-steps": ["Ph::ApplyUpdateSteps => act.push(Ph::ApplyUpdateSteps),", "Ph::ApplyUpdateSteps => act,"],
 # "*" logged as a phase.
 "star-logged": ["if ps.last() == Ph::Star { rest } else { rest.push(ps.last()) }", "rest.push(ps.last())"],
 # A failed phase does not stop the loop / a thrown failure leaves the phase open.
 "failure-does-not-stop": ["(seq![PE { name: ps[i as int], ok: false, ended: (fail->0).1 == Fail::Thrown }], false)",
                           "(seq![PE { name: ps[i as int], ok: false, ended: (fail->0).1 == Fail::Thrown }] + run_from(ps, i + 1, fail).0, false)"],
 "thrown-failure-left-open": ["ended: (fail->0).1 == Fail::Thrown }], false)", "ended: false }], false)"],
 # SetCommitted sites in LoadNewModule.
 "no-commit-mark-without-transaction": ["let r1 = if m == Mode::NoTx { set_committed(r) } else { r };", "let r1 = r;"],
 "no-commit-mark-dev": ["let r3 = if m == Mode::Dev { set_committed(r2) } else { r2 };", "let r3 = r2;"],
 "guard-after-commit-mark": ["if guard_refuses(l, upd) { Res { rec: r, ok: false, kept: false } }",
                             "if guard_refuses(l, upd) { Res { rec: if m == Mode::NoTx { set_committed(r) } else { r }, ok: false, kept: false } }"],
 # Main.Install keeps the placeholder version until LoadNewModule names the module.
 "install-version-not-from-search": ["                let r1 = set_version(r0, v.text);", "                let r1 = r0;"],
 # Phase commands: clean writes a record of its own; reload/compile write one; test installs scoped deps silently;
 # scoped dependency records labeled install.
 "clean-writes-own-record": ["    appended(w, unin::clean_cmd(w, k, force, global, fuel)).map_values(|hr: unin::HRow| dep_rec(hr))",
                             "    seq![top_unin(w, k, unin::clean_cmd(w, k, force, global, fuel).ok)] + appended(w, unin::clean_cmd(w, k, force, global, fuel)).map_values(|hr: unin::HRow| dep_rec(hr))"],
 "phase-command-records": ["            else { seq![] },\n    }", "            else { seq![dep_load_row(seq![], lnm0(seq![], None, v100(), false))] },\n    }"],
 "test-scoped-deps-unrecorded": ["{ scoped_rows(scoped) }", "{ seq![] }"],
 "scoped-dep-recorded-as-install": ["let r = lnm(set_version(init(h::Action::Load, n), l.to.text), l, false);",
                                    "let r = lnm(set_version(init(h::Action::Install, n), l.to.text), l, false);"],
}

dependency = {
 # AbstractHistory.cls:131 without the ISOK term: a successful uninstall would read uncommitted.
 "finalize-without-isok": {"path": ".g5/Verus_verification/consolidated/verus/history.rs",
                           "old": "committed:r.committed || status == 1", "new": "committed:r.committed"},
 # Committed derived from the final status alone (SetCommitted ignored).
 "finalize-status-only": {"path": ".g5/Verus_verification/consolidated/verus/history.rs",
                          "old": "committed:r.committed || status == 1", "new": "committed:status == 1"},
 # The Clean recursion writes no dependency record (Lifecycle/Base.cls:440 removed).
 "clean-recursion-no-dependency-record": {"path": "playbook-uplift/models/verus/uninstall.rs",
     "old": "Out { w: W { hist: w4.hist.push(HRow { m: Some(d), ok: true }), ..w4 }, ok: true, skip: false }",
     "new": "Out { w: w4, ok: true, skip: false }"},
}

# ------------------------------------------------------------------------ replay
replay = {}   # key -> (expression, model value)
prelude = {}  # key -> extra prelude (non-compute lemmas)

def rec_fields(case, R, rows_expr, rows, name, ver, nph, phases, ok, committed, placeholder):
    replay[f'{case}.rows'] = (rows_expr, rows)
    if rows == 0:
        return
    act = {'install': 2, 'update': 4, 'load': 1, 'uninstall': 3}
    a = 'install' if 'install' in rows_expr or 'api_install' in rows_expr else 'update'
    p = f'{case}.r1'
    replay[f'{p}.action'] = (f'act({R})', act[a])
    replay[f'{p}.pkg'] = (f'b({R}.row.package == {name})', 1)
    replay[f'{p}.pkgempty'] = (f'b({R}.row.package.len() == 0)', 1 if name == 'seq![]' else 0)
    replay[f'{p}.verplaceholder'] = (f'b(ver_is({R}, placeholder()))', 1 if placeholder else 0)
    replay[f'{p}.ver'] = (f'b(ver_is({R}, {ver}))', 0 if ver == 'seq![]' or placeholder else 1)
    replay[f'{p}.inprogress'] = (f'b({R}.row.status == 0)', 0)
    replay[f'{p}.ok'] = (f'b(ok_of({R}))', ok)
    replay[f'{p}.committed'] = (f'b({R}.row.committed)', committed)
    replay[f'{p}.ended'] = (f'b({R}.row.ended)', 1)
    replay[f'{p}.finalized'] = (f'b({R}.row.finalized)', 1)
    replay[f'{p}.nph'] = (f'nph({R})', nph)
    for i, (code, pok, pend) in enumerate(phases, 1):
        replay[f'{p}.ph{i}'] = (f'code({R}.phases[{i-1}].name)', code)
        replay[f'{p}.ph{i}.ok'] = (f'b({R}.phases[{i-1}].ok)', pok)
        replay[f'{p}.ph{i}.ended'] = (f'b({R}.phases[{i-1}].ended)', pend)

five = [(1, 1, 1), (2, 1, 1), (3, 1, 1), (4, 1, 1), (5, 1, 1)]
six = five + [(6, 1, 1)]
six_fail = five + [(6, 0, 1)]
V100, V110 = 'v100().text', 'v110().text'
rec_fields('i1', 'rec1(cli_install(i1()))', 'cli_install(i1()).len()', 1, 'n_h74a()', V100, 5, five, 1, 1, False)
rec_fields('i2', 'rec1(cli_install(i2()))', 'cli_install(i2()).len()', 1, 'n_h74a()', V100, 5, five, 1, 1, False)
rec_fields('i3', 'rec1(cli_install(i3()))', 'cli_install(i3()).len()', 1, 'n_nosuch()', 'seq![]', 0, [], 0, 0, True)
rec_fields('i4', '', 'cli_install(i4()).len()', 0, '', '', 0, [], 0, 0, True)
rec_fields('i4api', 'rec1(api_install(i4()))', 'api_install(i4()).len()', 1, 'seq![]', 'seq![]', 0, [], 0, 0, True)
rec_fields('i5', 'rec1(cli_install(i5()))', 'cli_install(i5()).len()', 1, 'n_h74c()', V100, 4, five[:3] + [(4, 0, 1)], 0, 0, False)
replay['i5.installed.h74c'] = ('b(after_version(None, i5(), false) is Some)', 0)
replay['r1.rows'] = ('cli_reinstall(installed0(), missing(n_nosuch())).len()', 0)
rec_fields('u1', 'rec1(cli_update(u1()))', 'cli_update(u1()).len()', 1, 'n_h74u()', V110, 6, six, 1, 1, False)
replay['u1.version.h74u'] = ('b(after_version(Some(v100()), u1().inst, true) == Some(v110()))', 1)
rec_fields('u2', 'rec1(cli_update(u2()))', 'cli_update(u2()).len()', 1, 'n_h74f()', V110, 6, six_fail, 0, 0, False)
replay['u2.version.h74f'] = ('b(after_version(Some(v100()), u2().inst, true) == Some(v100()))', 1)
rec_fields('u3', 'rec1(cli_update(u3()))', 'cli_update(u3()).len()', 1, 'n_h74f()', V110, 6, six_fail, 0, 1, False)
replay['u3.version.h74f'] = ('b(after_version(Some(v100()), u3().inst, true) == Some(v110()))', 1)
rec_fields('u4', '', 'cli_update(u4()).len()', 0, '', '', 0, [], 0, 0, True)
rec_fields('u5', '', 'cli_update(u5()).len()', 0, '', '', 0, [], 0, 0, True)
rec_fields('i6', 'rec1(cli_install(i6()))', 'cli_install(i6()).len()', 1, 'n_h74u()', V100, 0, [], 0, 0, False)
replay['i6.version.h74u'] = ('b(after_version(Some(v110()), i6(), false) == Some(v110()))', 1)
rec_fields('i7', 'rec1(cli_install(i7()))', 'cli_install(i7()).len()', 1, 'n_h74u()', V100, 0, [], 0, 0, False)
replay['i7.version.h74u'] = ('b(after_version(Some(v110()), i7(), false) == Some(v110()))', 1)

for key, a, n, t in [('install', 'Install', 'n_h74a()', 'Activate'), ('update', 'Update', 'n_h74u()', 'ApplyUpdateSteps'),
                     ('uninstall', 'Uninstall', 'pkg_of(5)', 'Clean')]:
    replay[f'mid.{key}'] = (f'b(running_ok(twin_while_running(h::Action::{a}, {n}), h::Action::{a}, {n}))', 1)
    replay[f'mid.{key}.permph'] = (f'nph(twin_while_running(h::Action::{a}, {n}))', 0)
    replay[f'mid.{key}.tempph'] = (f'started_through(Ph::{t}, Ph::{t})', {'install': 5, 'update': 6, 'uninstall': 1}[key])

# uninstall-family records: proved lemma facts (uninstall.rs worlds use `choose`, so no compute).
def unin_fields(case, rs, idx, pkg_match, pkg_val, version_known, ok, nph, phases):
    p = f'{case}.r{idx+1}'
    R = f'{rs}[{idx}]'
    replay[f'{p}.action'] = (f'act({R})', 3)
    replay[f'{p}.pkg'] = (f'b({R}.row.package == {pkg_match})', pkg_val)
    replay[f'{p}.pkgempty'] = (f'b({R}.row.package.len() == 0)', 0)
    replay[f'{p}.verplaceholder'] = (f'b(ver_is({R}, placeholder()))', 0 if version_known else 1)
    replay[f'{p}.ver'] = (f'b(ver_is({R}, fix_ver()))', 1 if version_known else 0)
    replay[f'{p}.inprogress'] = (f'b({R}.row.status == 0)', 0)
    replay[f'{p}.ok'] = (f'b(ok_of({R}))', ok)
    replay[f'{p}.committed'] = (f'b({R}.row.committed)', ok)
    replay[f'{p}.ended'] = (f'b({R}.row.ended)', 1)
    replay[f'{p}.finalized'] = (f'b({R}.row.finalized)', 1)
    replay[f'{p}.nph'] = (f'nph({R})', nph)
    for i, (code, pok, pend) in enumerate(phases, 1):
        replay[f'{p}.ph{i}'] = (f'code({R}.phases[{i-1}].name)', code)
        replay[f'{p}.ph{i}.ok'] = (f'b({R}.phases[{i-1}].ok)', pok)
        replay[f'{p}.ph{i}.ended'] = (f'b({R}.phases[{i-1}].ended)', pend)
    for k in list(replay):
        if k.startswith(p + '.'):
            prelude[k] = 'uninstall_fields();'

replay['n1.rows'] = ('n1_rs().len()', 1); prelude['n1.rows'] = 'uninstall_fields();'
unin_fields('n1', 'n1_rs()', 0, 'pkg_of(2)', 1, True, 1, 1, [(7, 1, 1)])
replay['n1.installed.h74s'] = ('b(unin_out(w_single(), 2, false, false, false, 3).w.mods.contains_key(2))', 0); prelude['n1.installed.h74s'] = 'uninstall_fields();'
replay['n2.rows'] = ('n2_rs().len()', 1); prelude['n2.rows'] = 'uninstall_fields();'
unin_fields('n2', 'n2_rs()', 0, 'pkg_of(9)', 1, False, 0, 0, [])
replay['n3.rows'] = ('n3_rs().len()', 0); prelude['n3.rows'] = 'uninstall_fields();'
# n4: computed from the observed Clean status (no processor failure in uninstall.rs); compute mode.
replay['n4.rows'] = ('n4_rs().len()', 1)
for f, (e, v) in {'action': ('act(n4_rs()[0])', 3), 'pkg': ('b(n4_rs()[0].row.package == pkg_of(5))', 1),
                  'pkgempty': ('b(n4_rs()[0].row.package.len() == 0)', 0), 'verplaceholder': ('b(ver_is(n4_rs()[0], placeholder()))', 0),
                  'ver': ('b(ver_is(n4_rs()[0], fix_ver()))', 1), 'inprogress': ('b(n4_rs()[0].row.status == 0)', 0),
                  'ok': ('b(ok_of(n4_rs()[0]))', 0), 'committed': ('b(n4_rs()[0].row.committed)', 0),
                  'ended': ('b(n4_rs()[0].row.ended)', 1), 'finalized': ('b(n4_rs()[0].row.finalized)', 1),
                  'nph': ('nph(n4_rs()[0])', 1), 'ph1': ('code(n4_rs()[0].phases[0].name)', 7),
                  'ph1.ok': ('b(n4_rs()[0].phases[0].ok)', 0), 'ph1.ended': ('b(n4_rs()[0].phases[0].ended)', 1)}.items():
    replay[f'n4.r1.{f}'] = (e, v)
replay['n5.rows'] = ('n5_rs().len()', 2); prelude['n5.rows'] = 'uninstall_fields();'
unin_fields('n5', 'n5_rs()', 0, 'pkg_of(3)', 1, True, 0, 1, [(7, 0, 1)])
unin_fields('n5', 'n5_rs()', 1, 'pkg_of(3)', 0, True, 0, 1, [(7, 0, 1)])
for k, (e, v) in {'n5.installed.h74y': ('b(unin_out(w_hookfail(), 3, false, true, false, 3).w.mods.contains_key(3))', 1),
                  'n5.installed.h74z': ('b(unin_out(w_hookfail(), 3, false, true, false, 3).w.mods.contains_key(2))', 1),
                  'n5.class.H74y.A': ('b(unin_out(w_hookfail(), 3, false, true, false, 3).w.present.contains(12))', 0),
                  'n5.class.H74z.A': ('b(unin_out(w_hookfail(), 3, false, true, false, 3).w.present.contains(10))', 1),
                  'c1.rows': ('c1_rs().len()', 1),
                  'c1.installed.h74d': ('b(unin::clean_cmd(unin::w_recremove(), 3, false, false, 3).w.mods.contains_key(2))', 0),
                  'c1.installed.h74m': ('b(unin::clean_cmd(unin::w_recremove(), 3, false, false, 3).w.mods.contains_key(3))', 1),
                  'c2.rows': ('c2_rows()', 0)}.items():
    replay[k] = (e, v); prelude[k] = 'uninstall_fields();'
unin_fields('c1', 'c1_rs()', 0, 'pkg_of(2)', 1, True, 1, 1, [(7, 1, 1)])
replay['t1.rows'] = ('cmd_rows(Cmd::Phase(Ph::Test), w_single(), 2, 3, t1_scoped()).len()', 1); prelude['t1.rows'] = 't1_facts();'
for f, (e, v) in {'action': ('act(t1_row())', 1), 'pkg': ('b(t1_row().row.package == n_h74b())', 1),
                  'pkgempty': ('b(t1_row().row.package.len() == 0)', 0), 'verplaceholder': ('b(ver_is(t1_row(), placeholder()))', 0),
                  'ver': ('b(ver_is(t1_row(), v100().text))', 1), 'inprogress': ('b(t1_row().row.status == 0)', 0),
                  'ok': ('b(ok_of(t1_row()))', 1), 'committed': ('b(t1_row().row.committed)', 1), 'ended': ('b(t1_row().row.ended)', 1),
                  'finalized': ('b(t1_row().row.finalized)', 1), 'nph': ('nph(t1_row())', 5)}.items():
    replay[f't1.r1.{f}'] = (e, v)
for i, c in enumerate([1, 2, 3, 4, 5], 1):
    replay[f't1.r1.ph{i}'] = (f'code(t1_row().phases[{i-1}].name)', c)
    replay[f't1.r1.ph{i}.ok'] = (f'b(t1_row().phases[{i-1}].ok)', 1)
    replay[f't1.r1.ph{i}.ended'] = (f'b(t1_row().phases[{i-1}].ended)', 1)
replay['t1.installed.h74b'] = ('b(lnm(set_version(init(h::Action::Load, n_h74b()), v100().text), t1_dep(), false).kept)', 1)

# n5's second record is h74z's; the Reader compared it with "h74y".
assert replay['n5.r2.pkg'][1] == 0 and replay['n5.r1.pkg'][1] == 1 and replay['n1.r1.pkg'][1] == 1 and replay['c1.r1.pkg'][1] == 1

preludes = {}
for k, (e, v) in replay.items():
    if k in prelude:
        preludes[k] = prelude[k]
    else:
        preludes[k] = f'assert(({e}) == {v}) by (compute_only);'

# Native-only expectations (not replayed): the n4 processor-failure state, which uninstall.rs cannot represent.
sanity = {'n4pre.file': 1, 'n4.file': 0, 'n4.installed.h74x': 1, 'n4.class.H74x.A': 1, 't1pre.installed.h74b': 0}

L = 'IPMCov.Reader74'
def dump(case, pkg, ver):
    return f'do $classmethod(R,"Dump","{case}",m,"{pkg}","{ver}")'
mark = 'set m=$classmethod(R,"Mark")'
session = [
 'do $system.OBJ.Load("/tmp/iter74/Reader74.cls","ck-d")',
 'do $system.OBJ.Load("/tmp/iter74/FailProc74.cls","ck-d")',
 'do $system.OBJ.Load("/tmp/iter74/Inst74.cls","ck-d")',
 f'set R="{L}"',
 'zpm "repo -fs -name h74 -path /tmp/iter74/repo/"',
 mark, 'zpm "install h74a"', dump('i1', 'h74a', '1.0.0'), 'do $classmethod(R,"ShowMid","install")',
 mark, 'zpm "install h74a"', dump('i2', 'h74a', '1.0.0'),
 mark, 'zpm "install h74nosuch"', dump('i3', 'h74nosuch', ''),
 mark, 'zpm "install"', dump('i4', '', ''),
 mark, 'kill info set info("parameters","module")="" try { do ##class(%IPM.Main).Install(.info) } catch e { write !,"API-REFUSED ",e.DisplayString(),! }',
 dump('i4api', '', ''),
 mark, 'zpm "install h74c"', dump('i5', 'h74c', '1.0.0'), 'do $classmethod(R,"Installed","i5","h74c")',
 mark, 'zpm "reinstall h74nosuch"', dump('r1', 'h74nosuch', ''),
 'zpm "install h74u 1.0.0"', 'zpm "install h74f 1.0.0"',
 mark, 'zpm "update h74u 1.1.0"', dump('u1', 'h74u', '1.1.0'), 'do $classmethod(R,"InstalledVersion","u1","h74u","1.1.0")',
 'do $classmethod(R,"ShowMid","update")',
 mark, 'zpm "update h74f 1.1.0"', dump('u2', 'h74f', '1.1.0'), 'do $classmethod(R,"InstalledVersion","u2","h74f","1.0.0")',
 mark, 'zpm "update h74f 1.1.0 -DNoTransaction=1"', dump('u3', 'h74f', '1.1.0'), 'do $classmethod(R,"InstalledVersion","u3","h74f","1.1.0")',
 mark, 'zpm "update h74nosuch"', dump('u4', 'h74nosuch', ''),
 mark, 'zpm "update h74u -DPermitDowngrade=1"', dump('u5', 'h74u', ''),
 mark, 'zpm "install h74u 1.0.0"', dump('i6', 'h74u', '1.0.0'), 'do $classmethod(R,"InstalledVersion","i6","h74u","1.1.0")',
 mark, 'zpm "install h74u 1.0.0 -DNoTransaction=1"', dump('i7', 'h74u', '1.0.0'), 'do $classmethod(R,"InstalledVersion","i7","h74u","1.1.0")',
 'zpm "load /tmp/iter74/local/h74d"', 'zpm "load /tmp/iter74/local/h74m"', 'zpm "load /tmp/iter74/local/h74x"',
 'zpm "load /tmp/iter74/local/h74s"', 'zpm "load /tmp/iter74/local/h74z"', 'zpm "load /tmp/iter74/local/h74y"',
 'zpm "load /tmp/iter74/local/h74t"',
 'do $classmethod(R,"FileThere","n4pre","/tmp/h74x-target/f.txt")',
 mark, 'zpm "uninstall h74s"', dump('n1', 'h74s', '1.0.0'), 'do $classmethod(R,"Installed","n1","h74s")',
 mark, 'zpm "uninstall h74nosuch"', dump('n2', 'h74nosuch', ''),
 mark, 'zpm "uninstall"', dump('n3', '', ''),
 mark, 'zpm "uninstall h74x"', dump('n4', 'h74x', '1.0.0'), 'do $classmethod(R,"Installed","n4","h74x")',
 'do $classmethod(R,"FileThere","n4","/tmp/h74x-target/f.txt")', 'do $classmethod(R,"ClassThere","n4","H74x.A")',
 'do $classmethod(R,"ShowMid","uninstall")',
 mark, 'zpm "uninstall h74y -r"', dump('n5', 'h74y', '1.0.0'), 'do $classmethod(R,"Installed","n5","h74y")',
 'do $classmethod(R,"Installed","n5","h74z")', 'do $classmethod(R,"ClassThere","n5","H74y.A")', 'do $classmethod(R,"ClassThere","n5","H74z.A")',
 mark, 'zpm "h74m clean"', dump('c1', 'h74d', '1.0.0'), 'do $classmethod(R,"Installed","c1","h74d")', 'do $classmethod(R,"Installed","c1","h74m")',
 mark, 'zpm "list"', 'zpm "h74a reload"', 'zpm "h74a compile"', 'zpm "reload h74a"', 'zpm "sync"', dump('c2', '', ''),
 'do $classmethod(R,"Installed","t1pre","h74b")',
 mark, 'zpm "h74t test"', dump('t1', 'h74b', '1.0.0'), 'do $classmethod(R,"Installed","t1","h74b")',
 'zpm "history find -limit 40"',
]

flip_fields = ['i3.rows', 'i4.rows', 'i4api.rows', 'u4.rows', 'u5.rows', 'r1.rows', 'u2.r1.committed', 'u3.r1.committed',
               'u1.r1.nph', 'u1.r1.ph6', 'i5.r1.nph', 'i5.r1.ph4.ended', 'i6.r1.ver', 'i7.r1.committed', 'n1.r1.committed',
               'n2.rows', 'n3.rows', 'n4.r1.committed', 'n5.r1.committed', 'n5.r1.pkg', 'c1.rows', 'c2.rows', 't1.rows',
               't1.r1.action', 'mid.install.permph']
assert all(k in replay for k in flip_fields)

cases = {
 'native_dir': 'iter74',
 'native_image_id': 'sha256:7da4480b6916c2205bb839c342d35ceed69abd6ef771f948922e7dd29963f903',
 'session_timeout': 1200,
 'models': {'actions.rs': mutants},
 'dependency_controls': {'actions.rs': dependency},
 'replay': {k: v[0] for k, v in replay.items()},
 'replay_preludes': preludes,
 'replay_grouped': True,
 'replay_prelude': '',
 'replay_model': 'actions.rs',
 'flip_fields': flip_fields,
 'session': session,
 'native_expected': {**{k: v[1] for k, v in replay.items()}, **sanity},
 'expectation_basis': 'Model values of actions.rs on the fixture inputs, checked by the replay preludes (compute_only, or '
                      'the proved uninstall_fields / t1_facts lemmas for uninstall.rs worlds). Not blind: the iteration74 '
                      'exploration container ran the same cases before the model was written (predictions.md).',
}
(here / 'cases.json').write_text(json.dumps(cases, indent=1) + '\n')
print(len(mutants), 'mutants;', len(dependency), 'dependency controls;', len(replay), 'replayed fields;', len(flip_fields), 'flips;', len(sanity), 'sanity')
