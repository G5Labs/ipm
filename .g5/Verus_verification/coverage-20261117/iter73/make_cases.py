#!/usr/bin/env python3
"""Writes cases.json for iteration73: mutants of grouping.rs (judged by its general theorems alone), dependency
controls for instances.rs, the native session and the replay of every native field against an instances.rs
expression. Run from this directory: python3 make_cases.py"""
import json
from pathlib import Path

here = Path(__file__).resolve().parent
root = here.parents[3]
mutants = {
 "name-top-level-only": ["match prev { None => Some(package(f)), Some(p) => Some(shared(p, package(f))) }", "Some(seq![t])"],
 "seed-with-top-level": ["None => Some(package(f)),", "None => Some(seq![f.name[0]]),"],
 "directory-only-key": ["kept(f, ex) && grouped(f) && directory(f) == d && f.name[0] == t", "kept(f, ex) && grouped(f) && directory(f) == d"],
 "top-level-only-key": ["kept(f, ex) && grouped(f) && directory(f) == d && f.name[0] == t", "kept(f, ex) && grouped(f) && f.name[0] == t"],
 "drop-leftovers": ["if single(f, ex) { rest.push(single_entry(f)) } else { rest }", "rest"],
 "writer-keys-by-directory": ["pub open spec fn same_key(a: Entry, b: Entry) -> bool { a.name == b.name && a.ext == b.ext }", "pub open spec fn same_key(a: Entry, b: Entry) -> bool { a.name == b.name && a.ext == b.ext && a.dir == b.dir }"],
 "tests-leftovers-kept": ["resources: collapse(singles(s, ex) + add_packages(s, ex, keys, None).0),", "resources: collapse(singles(s, ex) + add_packages(s, ex, keys, None).0 + if tests.is_some() && tests_exist { singles(ts, Excl { base: tests.unwrap(), tests: None }) } else { Seq::empty() }),"],
 "listing-ignored": ["!under_tests(f, ex) && f.listed && f.name.len() >= 1", "!under_tests(f, ex) && f.name.len() >= 1"],
 "unsupported-kept": ["pub open spec fn supported(ext: int) -> bool { ext == CLS || ext == MAC || ext == INC || ext == INT }", "pub open spec fn supported(ext: int) -> bool { true }"],
 "tests-not-excluded": ["supported(f.ext) && !under_tests(f, ex) && f.listed", "supported(f.ext) && f.listed"],
 "reader-root-before-ext-dir": ["} else if ext_dir_exists { ext_dir(if e.ext == PKG { CLS } else { e.ext }) } else { Seq::empty() }", "} else { Seq::empty() }"],
 "missing-source-continues": ["if !src_exists { None } else {", "if false { None } else {"],
 "directory-keeps-whole-path": ["if f.path.len() > pkg_len(f) { f.path.subrange(0, f.path.len() - pkg_len(f)) } else { Seq::empty() }", "f.path"],
 "tests-prefix-from-scan-folder": ["is_prefix(ex.tests.unwrap(), ex.base + f.path)", "is_prefix(ex.tests.unwrap(), f.path)"],
 "tests-branch-swapped": ["if tests_folder.is_none() { (pkg_entries(s, ex, keys), Seq::empty()) }", "if tests_folder.is_some() { (pkg_entries(s, ex, keys), Seq::empty()) }"],
 "unittest-name-without-folder": ["TestEntry { name: tf + k.0,", "TestEntry { name: k.0,"],
 "unittest-phase-other": ["package: group(ts, ex, k.0, k.1).unwrap(), phase: PHASE_TEST }", "package: group(ts, ex, k.0, k.1).unwrap(), phase: 0 }"],
 "collapse-keeps-first": ["drop_key(collapse(es.subrange(0, es.len() - 1)), last).push(last)", "if has_name(collapse(es.subrange(0, es.len() - 1)), last.name, last.ext) { collapse(es.subrange(0, es.len() - 1)) } else { collapse(es.subrange(0, es.len() - 1)).push(last) }"],
 "keys-sorted-descending": ["lex_lt(order_key(s, ex, keys[keys.len() - 2]), order_key(s, ex, keys[keys.len() - 1]))", "lex_lt(order_key(s, ex, keys[keys.len() - 1]), order_key(s, ex, keys[keys.len() - 2]))"],
 "export-nests-all": ["path: if ext == CLS { seq![ext] + name.subrange(0, name.len() - 1) } else { seq![ext] }, name, ext,", "path: seq![ext] + name.subrange(0, name.len() - 1), name, ext,"],
 "export-classes-flat": ["path: if ext == CLS { seq![ext] + name.subrange(0, name.len() - 1) } else { seq![ext] }, name, ext,", "path: seq![ext], name, ext,"],
 "no-backup": ["backups: d.backups.push(d.module_xml.unwrap()) }", "backups: d.backups }"],
}
grouping = '.g5/Verus_verification/coverage-20261117/iter73/grouping.rs'
dependency = {
 # The key string's directory/package separator decides m7 and m8. These alter only the rendering, so the general
 # theorems (stated over order_key) still hold; the native key lists stop being in $order.
 "key-separator-below-slash": {"path": grouping, "old": "join(k.0, 47) + seq![58]", "new": "join(k.0, 47) + seq![45]"},
 "key-separator-above-letters": {"path": grouping, "old": "join(k.0, 47) + seq![58]", "new": "join(k.0, 47) + seq![126]"},
 "export-nests-all-instances": {"path": grouping, "old": mutants["export-nests-all"][0], "new": mutants["export-nests-all"][1]},
 "unittest-name-without-folder-instances": {"path": grouping, "old": mutants["unittest-name-without-folder"][0], "new": mutants["unittest-name-without-folder"][1]},
}

E = 'Seq::<int>::empty()'
def has(case, name, dir_, ext='PKG'):
    return f'b(has_entry({case}.resources, Entry {{ name: seq![{name}], ext: {ext}, dir: {dir_} }}))'
def hasname(case, name, ext):
    return f'b(has_name({case}.resources, seq![{name}], {ext}))'
def loads(out, files, i):
    return f'b(loads_class({out}.resources, {files}(), {files}()[{i}]))'
def unit(case, name, package):
    o = f'{case}_out()'
    return {
        f'{case}.unittest': (f'b(has_test({o}.tests, ut(seq![{name}], seq![{package}])))', 1),
        f'{case}.unittest.tag': (f'b(one_test({o}) && only_test({o}).processor == UNITTEST)', 1),
        f'{case}.unittest.name': (f'b(one_test({o}) && only_test({o}).name == seq![{name}int])', 1),
        f'{case}.unittest.package': (f'b(one_test({o}) && only_test({o}).package == seq![{package}int])', 1),
        f'{case}.unittest.phase': (f'b(one_test({o}) && only_test({o}).phase == PHASE_TEST)', 1),
        f'{case}.loaded.unittest': (f'b(load_ok({o}.resources, {case}()) && has_test({o}.tests, ut(seq![{name}], seq![{package}])))', 1),
    }
X1, X2, X3, X4 = 'xout(x1(), 901)', 'xout(x2(), 902)', 'xout(x3(), 903)', 'xout(x4(), 904)'
replay = {
 # Flip targets first: the C:D73 load fields, collapse winners, UnitTest fields, export placement, backup.
 'm2a.load.ok': ('b(load_ok(m2a_out().resources, m2a()))', 0),
 'm2b.load.ok': ('b(load_ok(m2b_out().resources, m2b()))', 0),
 'x1.load.ok': (f'b(load_ok({X1}.resources, x1()))', 0),
 'x4.load.ok': (f'b(load_ok({X4}.resources, x4()))', 0),
 'x2.load.ok': (f'b(load_ok({X2}.resources, x2()))', 1),
 'm3.Id1.B.PKG@y': (has('m3_out()', '301, 12', 'seq![62]'), 1),
 'm7.Im7.PKG@a': (has('m7_out()', '701', 'seq![66]'), 1),
 'm8.Im8.PKG@ab': (has('m8_out()', '801', 'seq![68]'), 1),
 'class.Id1.B.C': (loads('m3_out()', 'm3', 0), 0),
 'class.Im7.Y': (loads('m7_out()', 'm7', 1), 0),
}
replay.update({k: v for k, v in unit('m4', '65, 64', '402').items()})
replay.update({k: v for k, v in unit('m1', '64', '211').items()})
replay.update({
 'x1.file.Ixa.R.Rtn.flat': ('b(exported(seq![901, 27, 28], MAC).path == seq![2int])', 1),
 'x1.Ixa.R.Rtn.MAC': (has(X1, '901, 27, 28', E, 'MAC'), 1),
 'm8.backup.old': ('b(m8_disk().backups == seq![1int])', 1),
 # iteration69 fields
 'm1.Ia1.B.PKG@cls': (has('m1_out()', '201, 12', 'seq![1]'), 1),
 'm1.Ia1.PKG': (hasname('m1_out()', '201', 'PKG'), 0),
 'm1.Ia2.PKG@cls': (has('m1_out()', '202', 'seq![1]'), 1),
 'm1.Ia2.B.PKG': (hasname('m1_out()', '202, 12', 'PKG'), 0),
 'm1.Ia3.B.PKG@cls': (has('m1_out()', '203, 12', 'seq![1]'), 1),
 'm1.Ia3.PKG': (hasname('m1_out()', '203', 'PKG'), 0),
 'm1.Ia3.B.C.cls': (hasname('m1_out()', '203, 12, 13', 'CLS'), 0),
 'm1.Ia4.PKG@cls': (has('m1_out()', '204', 'seq![1]'), 1),
 'm1.Ia5.PKG@cls': (has('m1_out()', '205', 'seq![1]'), 1),
 'm1.Ir69.MAC@mac': (has('m1_out()', '206', 'seq![2]', 'MAC'), 1),
 'm1.Ii69.INC@inc': (has('m1_out()', '207', 'seq![3]', 'INC'), 1),
 'm1.notes': (hasname('m1_out()', '208', 'TXT'), 0),
 'm1.Ij1': ('b(has_name(m1_out().resources, seq![209], PKG) || has_name(m1_out().resources, seq![209, 26], CLS))', 0),
 'm1.unittest.count': ('m1_out().tests.len() as int', 1),
 'm1.Ih69': ('b(has_name(m1_out().resources, seq![212], MAC) || has_test_package(m1_out().tests, seq![212]))', 0),
 'm1.Inopkg69': ('b(has_name(m1_out().resources, seq![213], CLS) || has_test_package(m1_out().tests, seq![213]))', 0),
 'm1.resource.count': ('m1_out().resources.len() as int', 7),
 'm3.Id1.B.PKG@x': (has('m3_out()', '301, 12', 'seq![61]'), 0),
 'm3.Id1.B.PKG.count': ('count_name(m3_out().resources, seq![301, 12], PKG)', 1),
 'm3.Id2.B.PKG@x': (has('m3_out()', '302, 12', 'seq![61]'), 1),
 'm3.Id2.D.PKG@y': (has('m3_out()', '302, 14', 'seq![62]'), 1),
 'm3.Id2.PKG': (hasname('m3_out()', '302', 'PKG'), 0),
 'm3.unittest.count': ('m3_out().tests.len() as int', 0),
 'm4.Ie1.PKG@cls': (has('m4_out()', '401', 'seq![1]'), 1),
 'm4.Ie1T.resource': ('b(has_name(m4_out().resources, seq![402], PKG) || has_name(m4_out().resources, seq![402, 18], CLS))', 0),
 'm4.Ihelp69': ('b(has_name(m4_out().resources, seq![403], MAC) || has_test_package(m4_out().tests, seq![403]))', 0),
 'm4.Inopkg69b': ('b(has_name(m4_out().resources, seq![404], CLS) || has_test_package(m4_out().tests, seq![404]))', 0),
 'm4.resource.count': ('m4_out().resources.len() as int', 1),
 'm2a.Ir2a.cls': (has('m2a_out()', '501', E, 'CLS'), 1),
 'm2a.Ib2a.P.PKG@cls': (has('m2a_out()', '502, 21', 'seq![1]'), 1),
 'm2e.Ir2e.cls': (has('m2e_out()', '504', E, 'CLS'), 1),
 'm2b.Ik2b.P.PKG': (has('m2b_out()', '503, 21', E), 1),
 'm5.Ig1.PKG@cls': (has('m5_out()', '601', 'seq![1]'), 1),
 'm5.Ig2.PKG@cls': (has('m5_out()', '602', 'seq![1]'), 1),
 'm5.Ig3': ('b(has_name(m5_out().resources, seq![603], PKG) || has_name(m5_out().resources, seq![603, 26], CLS))', 0),
 'm6.xml.exists': (f'b(generate(false, src(), Seq::empty(), none_keys(), Some(seq![64]), true, Seq::empty(), none_keys()).is_some())', 0),
 'm6.error': (f'b(generate(false, src(), Seq::empty(), none_keys(), Some(seq![64]), true, Seq::empty(), none_keys()).is_none())', 1),
 'm1.load.ok': ('b(load_ok(m1_out().resources, m1()))', 1),
 'm3.load.ok': ('b(load_ok(m3_out().resources, m3()))', 1),
 'm4.load.ok': ('b(load_ok(m4_out().resources, m4()))', 1),
 'm2e.load.ok': ('b(load_ok(m2e_out().resources, m2e()))', 1),
 'class.Ia1.B.C': (loads('m1_out()', 'm1', 0), 1),
 'class.Ia2.D.E': (loads('m1_out()', 'm1', 3), 1),
 'class.Ia3.B.C': (loads('m1_out()', 'm1', 4), 1),
 'class.Ia5.Y': (loads('m1_out()', 'm1', 6), 1),
 'class.Id1.B.E': (loads('m3_out()', 'm3', 2), 1),
 'class.Id2.B.C': (loads('m3_out()', 'm3', 1), 1),
 'class.Id2.D.E': (loads('m3_out()', 'm3', 3), 1),
 'class.Ie1.A': (loads('m4_out()', 'm4', 0), 1),
 'class.Ir2a': (loads('m2a_out()', 'm2a', 0), 0),
 'class.Ib2a.P.Q': (loads('m2a_out()', 'm2a', 1), 0),
 'class.Ir2e': (loads('m2e_out()', 'm2e', 0), 1),
 'class.Ik2b.P.Q': (loads('m2b_out()', 'm2b', 0), 0),
 # iteration73: collapse order
 'm7.Im7.PKG@a/b': (has('m7_out()', '701', 'seq![66, 67]'), 0),
 'm7.Im7.PKG.count': ('count_name(m7_out().resources, seq![701], PKG)', 1),
 'm7.load.ok': ('b(load_ok(m7_out().resources, m7()))', 1),
 'class.Im7.X': (loads('m7_out()', 'm7', 0), 1),
 'm8.Im8.PKG@a': (has('m8_out()', '801', 'seq![66]'), 0),
 'm8.Im8.PKG.count': ('count_name(m8_out().resources, seq![801], PKG)', 1),
 'm8.load.ok': ('b(load_ok(m8_out().resources, m8()))', 1),
 'class.Im8.X': (loads('m8_out()', 'm8', 0), 0),
 'class.Im8.Y': (loads('m8_out()', 'm8', 1), 1),
 # module.xml backup
 'm8.backup.count': ('m8_disk().backups.len() as int', 1),
 'm8.xml.old': ('b(m8_disk().module_xml == Some(1int))', 0),
 'm7.backup.count': ('m7_disk().backups.len() as int', 0),
 'm7.backup.old': ('b(m7_disk().backups == seq![1int])', 0),
 'm7.xml.old': ('b(m7_disk().module_xml == Some(1int))', 0),
 # generate -export -> load
 'x1.file.Ixa.P.Q': ('b(exported(seq![901, 21, 22], CLS).path == seq![1int, 901, 21])', 1),
 'x1.file.Ixa.R.Rtn.nested': ('b(exported(seq![901, 27, 28], MAC).path == seq![2int, 901, 27])', 0),
 'x1.Ixa.P.PKG@cls': (has(X1, '901, 21', 'seq![1]'), 1),
 'x1.resource.count': (f'{X1}.resources.len() as int', 2),
 'after.Ixa.P.Q': (loads(X1, 'x1', 0), 0),
 'after.rtn.Ixa.R.Rtn.MAC': (loads(X1, 'x1', 1), 0),
 'x2.file.Ixb.P.Q': ('b(exported(seq![902, 21, 22], CLS).path == seq![1int, 902, 21])', 1),
 'x2.file.IxbRtn': ('b(exported(seq![905], MAC).path == seq![2int])', 1),
 'x2.Ixb.P.PKG@cls': (has(X2, '902, 21', 'seq![1]'), 1),
 'x2.IxbRtn.MAC@mac': (has(X2, '905', 'seq![2]', 'MAC'), 1),
 'x2.resource.count': (f'{X2}.resources.len() as int', 2),
 'after.Ixb.P.Q': (loads(X2, 'x2', 0), 1),
 'after.rtn.IxbRtn.MAC': (loads(X2, 'x2', 1), 1),
 'x3.file.Ixc.P.Q': ('b(exported(seq![903, 21, 22], CLS).path == seq![1int, 903, 21])', 1),
 'x3.file.IxcInc': ('b(exported(seq![906], INC).path == seq![3int])', 1),
 'x3.Ixc.P.PKG@cls': (has(X3, '903, 21', 'seq![1]'), 1),
 'x3.IxcInc.INC@inc': (has(X3, '906', 'seq![3]', 'INC'), 1),
 'x3.resource.count': (f'{X3}.resources.len() as int', 2),
 'x3.load.ok': (f'b(load_ok({X3}.resources, x3()))', 1),
 'after.Ixc.P.Q': (loads(X3, 'x3', 0), 1),
 'after.rtn.IxcInc.INC': (loads(X3, 'x3', 1), 1),
 'x4.file.Ixd.P.Q': ('b(exported(seq![904, 21, 22], CLS).path == seq![1int, 904, 21])', 1),
 'x4.file.Ixd.I.Inc.flat': ('b(exported(seq![904, 30, 29], INC).path == seq![3int])', 1),
 'x4.file.Ixd.I.Inc.nested': ('b(exported(seq![904, 30, 29], INC).path == seq![3int, 904, 30])', 0),
 'x4.Ixd.P.PKG@cls': (has(X4, '904, 21', 'seq![1]'), 1),
 'x4.Ixd.I.Inc.INC': (has(X4, '904, 30, 29', E, 'INC'), 1),
 'x4.resource.count': (f'{X4}.resources.len() as int', 2),
 'after.Ixd.P.Q': (loads(X4, 'x4', 0), 0),
 'after.rtn.Ixd.I.Inc.INC': (loads(X4, 'x4', 1), 0),
})
flip_fields = ['m2a.load.ok', 'm2b.load.ok', 'x1.load.ok', 'x4.load.ok', 'x2.load.ok', 'm3.Id1.B.PKG@y', 'm7.Im7.PKG@a',
               'm8.Im8.PKG@ab', 'class.Id1.B.C', 'class.Im7.Y', 'm4.unittest.name', 'm4.unittest.phase', 'm1.unittest.package',
               'x1.file.Ixa.R.Rtn.flat', 'x1.Ixa.R.Rtn.MAC', 'm8.backup.old']
# Checks on the run itself, not replayed: the seed loaded, the delete took effect, no generated INT was exported.
sanity = {**{f'seed.{c}': 1 for c in ['Ixa.P.Q', 'Ixb.P.Q', 'Ixc.P.Q', 'Ixd.P.Q']},
          **{f'seed.rtn.{r}': 1 for r in ['Ixa.R.Rtn.MAC', 'IxbRtn.MAC', 'IxcInc.INC', 'Ixd.I.Inc.INC']},
          **{f'pre.{c}': 0 for c in ['Ixa.P.Q', 'Ixb.P.Q', 'Ixc.P.Q', 'Ixd.P.Q']},
          **{f'pre.rtn.{r}': 0 for r in ['Ixa.R.Rtn.MAC', 'IxbRtn.MAC', 'IxcInc.INC', 'Ixd.I.Inc.INC']},
          'x1.dir.int': 0, 'x2.dir.int': 0}

def gen(path, name, src='src', tests='tests', flags=''):
    return [f'zpm "generate /tmp/iter73/{path}{flags}"', name, '1.0.0', '', '', src, tests, '', 'N', 'N', '']
session = ['do $system.OBJ.Load("/tmp/iter73/Reader.cls","ck-d")',
           'for f="Ixa.P.Q.cls","Ixb.P.Q.cls","Ixc.P.Q.cls","Ixd.P.Q.cls","Ixa.R.Rtn.mac","IxbRtn.mac","IxcInc.inc","Ixd.I.Inc.inc" { do $system.OBJ.Load("/tmp/iter73/seed/"_f,"ck-d") }',
           'do ##class(IPMCov.GenReader73).Seeded("seed")']
for m in ['m1', 'm3']: session += gen(m, m)
session += gen('m4', 'm4', tests='src/tests')
for m in ['m2a', 'm2e', 'm2b', 'm5']: session += gen(m, m)
session += ['write !,"BEGIN|m6",!'] + gen('m6', 'm6', src='nosrc') + ['write !,"END|m6",!']
for m in ['m7', 'm8']: session += gen(m, m)
for x, p in [('x1', 'Ixa'), ('x2', 'Ixb'), ('x3', 'Ixc'), ('x4', 'Ixd')]:
    session += gen(x, x, flags=f' -export 00000,{p}')
session += ['do ##class(IPMCov.GenReader73).Manifests()', 'do ##class(IPMCov.GenReader73).DeleteExported()']
for m in ['m1', 'm3', 'm4', 'm2a', 'm2e', 'm2b', 'm7', 'm8', 'x1', 'x2', 'x3', 'x4']:
    session += [f'write !,"BEGIN|load-{m}",!', f'zpm "load /tmp/iter73/{m}"', f'write !,"END|load-{m}",!']
session += ['do ##class(IPMCov.GenReader73).Loaded()']

cfg = {
 'native_dir': 'iter73',
 'native_image_id': 'sha256:7da4480b6916c2205bb839c342d35ceed69abd6ef771f948922e7dd29963f903',
 'session_timeout': 1500,
 'models': {'grouping.rs': mutants, 'instances.rs': {}},
 'dependency_controls': {'instances.rs': dependency},
 'replay': {k: v[0] for k, v in replay.items()},
 'replay_preludes': {k: f'assert(({v[0]}) == {v[1]}) by (compute_only);' for k, v in replay.items()},
 'replay_grouped': True,
 'replay_prelude': '',
 'replay_model': 'instances.rs',
 'flip_fields': flip_fields,
 'output_observations': [{'command': 'm6', 'key': 'm6.error', 'pattern': "Directory /tmp/iter73/m6/nosrc/ doesn't exist"}],
 'session': session,
 'native_expected': {**{k: v[1] for k, v in replay.items()}, **sanity},
 'expectation_basis': 'Model values of instances.rs (rules in grouping.rs), computed by compute_only in the replay preludes. '
                      'Predictions for the export round trip, the collapse winners of m7/m8 and the module.xml backup were '
                      'written before any iteration73 native run in .g5/uplift/coverage-20261117/iter73/predictions.md; '
                      'the iteration69 fields repeat that run\'s observations, so they are not blind.',
}
assert all(k in replay for k in flip_fields)
(here / 'cases.json').write_text(json.dumps(cfg, indent=1) + '\n')
print(len(mutants), 'mutants;', len(dependency), 'dependency controls;', len(replay), 'replayed fields;', len(flip_fields), 'flips;', len(sanity), 'sanity')
