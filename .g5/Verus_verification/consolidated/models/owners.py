"""Family R4: owners of transactions, locks and system routines outside the load path.
- `module-version` (Main.ModuleVersion in its own transaction; SemanticVersion With* builders; the stored manifest
  rewrite StudioDocument.Module.UpdateVersion).
- The filesystem repository cache (Repo/Filesystem/Definition.cls BuildCache in its own transaction, AddCacheItem,
  ScanDirectory) and the lock manager it uses (Utils/LockManager.cls) against a second process holding the lock.
- The language-extension routines %ZLANGC00/%ZLANGF00 rewritten around user code (Main.UpdateLanguageExtensionsOne).
Native runs: `20261114T033125Z-r4-owners` (MV0-MV10, LK1-LK5, LE0-LE3), plus earlier receipts for the same members:
`20261114T030608Z-r2-semver` (B1 builders), `20261114T025102Z-r1-temp-repo` (TR7: an invalid module name fails the
cache build and its save is rolled back), `20261114T025551Z-r1-eval-postcommit` (D.setup: Depth=1 skips a deeper
invalid manifest).
Premise: the lock timeout of %LockId is at least 5 s (observed "waited=timeout"); the second process is a JOB.
"""
import re
ROUND = 'R4'
FAMILY = 'owners-outside-load'
RUNS = ['20261114T033125Z-r4-owners', '20261114T030608Z-r2-semver', '20261114T025102Z-r1-temp-repo', '20261114T025551Z-r1-eval-postcommit', '20261114T034941Z-r4-lock-depth']
def case_run(case):
    if case in ('LK5', 'LK5.setup') or case.startswith('DC'): return RUNS[4]
    if case == 'B1': return RUNS[1]
    if case in ('TR7',): return RUNS[2]
    if case == 'D.setup': return RUNS[3]
    return RUNS[0]
SV, FS, LM = 'General/SemanticVersion.cls', 'Repo/Filesystem/Definition.cls', 'Utils/LockManager.cls'
MEMBERS = {
    'Main.cls::ModuleVersion': {'slice': 'Main.cls:4011-4098', 'fn': 'module_version', 'behavior': 'new version from bump/explicit/modifiers; refused unless strictly higher; validated; saved and manifest rewritten in one transaction (MV1-MV9)'},
    SV + '::WithBumpMajor': {'slice': SV + ':212-221', 'fn': 'with_bump', 'behavior': 'major+1, minor and patch 0, no prerelease or build (MV8, B1)'},
    SV + '::WithBumpMinor': {'slice': SV + ':223-232', 'fn': 'with_bump', 'behavior': 'minor+1, patch 0 (MV1, B1)'},
    SV + '::WithBumpPatch': {'slice': SV + ':234-243', 'fn': 'with_bump', 'behavior': 'patch+1, prerelease and build cleared (MV4, B1)'},
    SV + '::WithPrerelease': {'slice': SV + ':245-254', 'fn': 'with_prerelease', 'behavior': 'replaces the prerelease, keeps the build (B1)'},
    SV + '::WithBuild': {'slice': SV + ':256-265', 'fn': 'with_build', 'behavior': 'replaces the build, keeps the prerelease (MV4, B1)'},
    'StudioDocument/Module.cls::UpdateVersion': {'slice': 'StudioDocument/Module.cls:403-420', 'fn': 'update_version', 'behavior': 'rewrites <Version> of the stored .ZPM document; the file on disk is untouched (MV, MV10)'},
    FS + '::BuildCache': {'slice': FS + ':117-182', 'fn': 'build_cache', 'behavior': 'locks an existing repository, purges and rebuilds its cache in one transaction; any failure rolls back (LK2, LK3, TR7)'},
    FS + '::AddCacheItem': {'slice': FS + ':184-241', 'fn': 'add_cache_item', 'behavior': 'an unparseable manifest is skipped; a manifest whose save fails (invalid name) fails the build (LK1, TR7)'},
    FS + '::ScanDirectory': {'slice': FS + ':313-357', 'fn': 'scan_directory', 'behavior': 'Depth limits how far below the root manifests are found, hidden .modules folders included (DC0 fails, DC1 and D.setup build)'},
    LM + '::LockClassId': {'slice': LM + ':67-79', 'fn': 'lock_class_id', 'behavior': 'takes the object lock; another holder makes it fail after the timeout (LK2)'},
    LM + '::UnlockClassId': {'slice': LM + ':81-92', 'fn': 'unlock_class_id', 'behavior': 'releases a held object lock (LK4)'},
    LM + '::UnlockAll': {'slice': LM + ':94-141', 'fn': 'unlock_all', 'behavior': 'releases every held name and object lock (LK4)'},
    'Main.cls::UpdateLanguageExtensionsOne': {'slice': 'Main.cls:3420-3462', 'fn': 'zlang_one', 'behavior': 'replaces only the generated block, keeps user lines, appends the block once if absent (LE1, LE2, LE3)'},
}
V = {'v': 'as-built'}
def var(n): return V['v'] == n
VARIANTS = {
    'mv-unchanged-skips': {'member': 'Main.cls::ModuleVersion', 'wrong': 'an unchanged version is skipped with OK (the unreachable branch; intended reading)'},
    'mv-force-ignored': {'member': 'Main.cls::ModuleVersion', 'wrong': '-force does not allow a lower version'},
    'mv-no-validation': {'member': 'Main.cls::ModuleVersion', 'wrong': 'an invalid version string is accepted'},
    'mv-modifiers-ignored': {'member': 'Main.cls::ModuleVersion', 'wrong': '-prerelease and -build are ignored'},
    'bump-major-keeps-minor': {'member': SV + '::WithBumpMajor', 'wrong': 'major bump keeps minor'},
    'bump-minor-keeps-patch': {'member': SV + '::WithBumpMinor', 'wrong': 'minor bump keeps patch'},
    'bump-patch-keeps-prerelease': {'member': SV + '::WithBumpPatch', 'wrong': 'patch bump keeps the prerelease'},
    'withpre-drops-build': {'member': SV + '::WithPrerelease', 'wrong': 'setting a prerelease drops the build'},
    'withbuild-drops-pre': {'member': SV + '::WithBuild', 'wrong': 'setting a build drops the prerelease'},
    'updateversion-noop': {'member': 'StudioDocument/Module.cls::UpdateVersion', 'wrong': 'the stored manifest keeps the old version'},
    'updateversion-writes-file': {'member': 'StudioDocument/Module.cls::UpdateVersion', 'wrong': 'module.xml on disk is rewritten too'},
    'buildcache-no-lock': {'member': FS + '::BuildCache', 'wrong': 'a refresh does not lock the repository'},
    'buildcache-no-purge': {'member': FS + '::BuildCache', 'wrong': 'a refresh keeps entries for removed modules'},
    'buildcache-keeps-save-on-failure': {'member': FS + '::BuildCache', 'wrong': 'a failed build keeps the saved repository row'},
    'addcache-malformed-fails': {'member': FS + '::AddCacheItem', 'wrong': 'an unparseable manifest fails the build'},
    'addcache-invalid-name-skipped': {'member': FS + '::AddCacheItem', 'wrong': 'an invalid name is skipped like an unparseable manifest'},
    'scan-ignores-depth': {'member': FS + '::ScanDirectory', 'wrong': 'Depth is ignored'},
    'lock-released-inside-transaction': {'member': None, 'wrong': 'a refresh inside an open outer transaction releases the lock before that transaction ends'},
    'lockclassid-noop': {'member': LM + '::LockClassId', 'wrong': 'no lock is taken'},
    'unlockclassid-noop': {'member': LM + '::UnlockClassId', 'wrong': 'the object lock is not released'},
    'unlockall-names-only': {'member': LM + '::UnlockAll', 'wrong': 'only name locks are released, object locks are kept'},
    'zlang-overwrite': {'member': 'Main.cls::UpdateLanguageExtensionsOne', 'wrong': 'the routine is replaced, user lines lost'},
    'zlang-append-always': {'member': 'Main.cls::UpdateLanguageExtensionsOne', 'wrong': 'the block is appended on every run'},
}
def sv(s):
    m = re.match(r'^(\d+)\.(\d+)\.(\d+)(?:-([^+]*))?(?:\+(.*))?$', s)
    return {'Major': int(m[1]), 'Minor': int(m[2]), 'Patch': int(m[3]), 'Pre': m[4] or '', 'Build': m[5] or ''}
def tostr(v): return f"{v['Major']}.{v['Minor']}.{v['Patch']}" + (f"-{v['Pre']}" if v['Pre'] else '') + (f"+{v['Build']}" if v['Build'] else '')
def with_bump(v, part):
    n = dict(v, Pre='', Build='')
    if part == 'major': n.update(Major=v['Major'] + 1, Minor=v['Minor'] if var('bump-major-keeps-minor') else 0, Patch=0)
    if part == 'minor': n.update(Minor=v['Minor'] + 1, Patch=v['Patch'] if var('bump-minor-keeps-patch') else 0)
    if part == 'patch':
        n.update(Patch=v['Patch'] + 1)
        if var('bump-patch-keeps-prerelease'): n['Pre'] = v['Pre']
    return n
def with_prerelease(v, p): return dict(v, Pre=p, Build='' if var('withpre-drops-build') else v['Build'])
def with_build(v, b): return dict(v, Build=b, Pre='' if var('withbuild-drops-pre') else v['Pre'])
def follows(a, b):   # SemanticVersion.Follows for the forms used here (modeled in family semantic-versions)
    ta, tb = (a['Major'], a['Minor'], a['Patch']), (b['Major'], b['Minor'], b['Patch'])
    if a['Pre'] or b['Pre']:
        if ta != tb: return False if (a['Pre'] and b['Pre']) else ta > tb
        if a['Pre'] == b['Pre']: return False
        if a['Pre'] and not b['Pre']: return False
        if not a['Pre'] and b['Pre']: return True
        return a['Pre'] > b['Pre']
    return ta > tb
NOTE = "  Note: when either version has a nonempty prerelease value, and differ from the other by major.minor.patch, neither version is considered strictly higher than the other.        See https://github.com/npm/node-semver?tab=readme-ov-file#prerelease-tags for more information."
def update_version(st, v):
    if not var('updateversion-noop'): st['doc'] = tostr(v)
    if var('updateversion-writes-file'): st['file'] = tostr(v)
def module_version(st, args):
    toks = args.split(); name = toks[0]; mods = {}
    rest = [t for t in toks[1:]]
    pos = []; i = 0
    while i < len(rest):
        if rest[i] == '-force': mods['force'] = 1
        elif rest[i] in ('-prerelease', '-build'): mods[rest[i][1:]] = rest[i + 1]; i += 1
        else: pos.append(rest[i])
        i += 1
    if name != 'mv-a': return 'ERR', f'ERROR #5001: Module {name} not found.'
    cur = sv(st['mv']); ver = pos[0] if pos else ''
    if ver in ('major', 'minor', 'patch'): new = with_bump(cur, ver)
    elif ver == '': new = dict(cur)
    else:
        if re.match(r'^0\d', ver) and not var('mv-no-validation'): return 'ERR', f'ERROR #5001: Invalid version string: {ver}'
        new = sv(ver.lstrip('0') or '0')
    if not var('mv-modifiers-ignored'):
        if 'prerelease' in mods: new = with_prerelease(new, mods['prerelease'])
        if 'build' in mods: new = with_build(new, mods['build'])
    unchanged = tostr(new) == tostr(cur)
    if var('mv-unchanged-skips') and unchanged: return 'OK', None
    if not (mods.get('force') and not var('mv-force-ignored')) and not follows(new, cur):
        return 'ERR', f"ERROR #5001: Cannot set version to {tostr(new)} as it is not strictly higher than the current version {tostr(cur)} (Use -force to override){NOTE}"
    if unchanged: return 'OK', None
    st['mv'] = tostr(new); update_version(st, new)
    return 'OK', None

def scan_directory(tree, depth):
    # Hidden folders such as .modules are listed and descended (DC0 vs DC1).
    return [p for p in tree if depth == 0 or p.count('/') + 1 <= depth or var('scan-ignores-depth')]
def add_cache_item(item):
    """item: 'ok', 'malformed' or 'badname'. Returns 'cached', 'skipped' or 'error'."""
    if item == 'malformed': return 'error' if var('addcache-malformed-fails') else 'skipped'
    if item == 'badname': return 'skipped' if var('addcache-invalid-name-skipped') else 'error'
    return 'cached'
def lock_manager_new(): return {'held': 0}
def lock_class_id(lm, other_holds):
    if var('lockclassid-noop'): return True
    if other_holds: return False
    lm['held'] += 1; return True
def unlock_class_id(lm):
    if not var('unlockclassid-noop'): lm['held'] -= 1
def unlock_all(lm):
    if not var('unlockall-names-only'): unlock_class_id(lm)
def build_cache(repo, other_holds=False, outer_tx=False):
    """Returns (ok, held_after): the cache is rebuilt from scratch in one transaction; the lock manager closes on exit."""
    lm = lock_manager_new()
    if repo['exists'] and not var('buildcache-no-lock'):
        if not lock_class_id(lm, other_holds): return False, 0
    results = {n.split('/')[-1]: add_cache_item(k) for n, k in repo['tree'].items() if n in scan_directory(list(repo['tree']), repo.get('depth', 0))}
    ok = 'error' not in results.values()
    if ok:
        cached = sorted(n for n, r in results.items() if r == 'cached')
        repo['cache'] = sorted(set(repo['cache']) | set(cached)) if var('buildcache-no-purge') else cached
    # On exit the manager closes and unlocks. Observed premise (LK5 rerun): inside an open outer transaction the object
    # lock stays held until that transaction ends, whatever the release mode; LockManager:%OnNew is therefore not credited.
    deferred = lm['held'] if (outer_tx and not var('lock-released-inside-transaction')) else 0
    if lm['held']: unlock_all(lm)
    return ok, max(deferred, lm['held'])

def zlang_one(lines, gen):
    if var('zlang-overwrite'): return list(gen)
    out, found, i = [], False, 0
    while i < len(lines):
        if lines[i] == 'START' and not var('zlang-append-always'):
            while i < len(lines) and lines[i] != 'END': i += 1
            out += gen; found = True
        else: out.append(lines[i])
        i += 1
    if not found: out += gen
    return out

def predict(variant):
    V['v'] = variant; out = {}
    st = {'mv': '1.2.3', 'doc': '1.2.3', 'file': '1.2.3'}
    def obs(case): out[(case, 'mv')] = f"{st['mv']} doc={st['doc']}"
    out[('MV0', 'status')] = 'OK'; out[('MV0', 'tlevel.before')] = '0'; out[('MV0', 'tlevel.after')] = '0'; obs('MV0')
    for case, args in (('MV1', 'mv-a minor'), ('MV2', 'mv-a 1.0.0'), ('MV3', 'mv-a 1.0.0 -force'), ('MV4', 'mv-a patch -prerelease rc.1 -build b7'),
                       ('MV5', 'mv-a'), ('MV6', 'mv-a 2.0.0-beta'), ('MV7', 'mv-a 01.0.0'), ('MV8', 'mv-a major'), ('MV9', 'no-such-module')):
        status, err = module_version(st, args)
        out[(case, 'status')] = status
        if err: out[(case, 'error')] = err
        out[(case, 'tlevel.before')] = '0'; out[(case, 'tlevel.after')] = '0'; obs(case)
    out[('MV10', 'file')] = st['file']
    b = sv('1.2.3-beta+b')
    out[('B1', 'major')] = f"{tostr(with_bump(b, 'major'))} minor={tostr(with_bump(b, 'minor'))} patch={tostr(with_bump(b, 'patch'))} pre={tostr(with_prerelease(b, 'rc.1'))} build={tostr(with_build(b, 'x'))} nobuild=1.2.3-beta"
    # LK: reglock holds lk-a, lk-b and a malformed lk-garbage
    repo = {'exists': False, 'tree': {'lk-a': 'ok', 'lk-b': 'ok', 'lk-garbage': 'malformed'}, 'cache': []}
    ok, _ = build_cache(repo); repo['exists'] = ok
    out[('LK1', 'build')] = f"{int(ok)} cached={' '.join(repo['cache'])}"
    out[('LK2.setup', 'held')] = '1'
    ok, _ = build_cache(repo, other_holds=True)
    out[('LK2', 'build')] = f"{int(ok)} waited={'short' if ok else 'timeout'} tlevel=0"
    if not ok: out[('LK2', 'error')] = "ERROR #5803: Failed to acquire exclusive lock on instance of '2:%IPM.Repo.Filesystem.Definition'"
    del repo['tree']['lk-b']
    ok, held = build_cache(repo)
    out[('LK3', 'build')] = f"{int(ok)} cached={' '.join(repo['cache'])} tlevel=0"
    out[('LK4', 'lockable')] = '0' if held else '1'
    # LK5 rerun (`r4-lock-depth`): the refresh runs inside a transaction opened in a method (the terminal refuses TSTART;
    # the first LK5 in `r4-owners` never opened it and is not claimed).
    repo5 = {'exists': True, 'tree': {'lk-a': 'ok', 'lk-b': 'ok', 'lk-garbage': 'malformed'}, 'cache': []}
    out[('LK5.setup', 'build')] = '1'
    ok, held = build_cache(repo5, outer_tx=True)
    out[('LK5', 'build')] = f"{int(ok)} tlevel.inside=1 lockable={'0' if held else '1'}"
    out[('LK5', 'tlevel.after')] = '0'; out[('LK5', 'entry')] = '0'
    for d in (0, 1):
        t = {'exists': False, 'depth': d, 'tree': {'dd-dep-txn': 'ok', 'dd-dep-dev': 'ok', 'dd-dep-ok': 'ok',
             'dd-dep-txn/.modules/broken': 'badname', 'dd-dep-dev/.modules/broken': 'badname'}, 'cache': []}
        ok, _ = build_cache(t); out[(f'DC{d}', 'buildcache')] = str(int(ok))
    # TR7 (R1 run): .modules/broken has an invalid module name; D.setup (R1b run): Depth=1 hides deeper manifests
    tr = {'exists': False, 'tree': {'broken': 'badname'}, 'cache': []}
    ok, _ = build_cache(tr)
    out[('TR7', 'status')] = 'ERR' if not ok else 'OK'
    out[('TR7', 'repos.count')] = '1' if (ok or var('buildcache-keeps-save-on-failure')) else '0'
    regd = {'exists': False, 'depth': 1, 'tree': {'dd-dep-txn': 'ok', 'dd-dep-dev': 'ok', 'dd-dep-ok': 'ok',
            'dd-dep-txn/.modules/broken': 'badname', 'dd-dep-dev/.modules/broken': 'badname'}, 'cache': []}
    ok, _ = build_cache(regd)
    out[('D.setup', 'buildcache')] = str(int(ok))
    # LE: %ZLANGC00 with IPM's block and a user line added after it
    gen = ['START', 'g1', 'g2', 'END']
    c00 = ['user0'] + gen + ['user-added']
    def zl(lines): return f"%ZLANGC00=starts:{lines.count('START')},user:{sum(1 for l in lines if l == 'user-added')} %ZLANGF00=starts:1,user:0"
    out[('LE1.setup', 'zlang')] = zl(c00)
    c00 = zlang_one(c00, gen); out[('LE1', 'zlang')] = zl(c00)
    c00 = zlang_one(c00, gen); out[('LE2', 'zlang')] = zl(c00)
    out[('LE3', 'testonly.found')] = '1'
    return out

def _unchanged_skips(p): return (p[('MV5', 'status')] == 'OK', p.get(('MV5', 'error'), ''))
def _file_follows(p): return (p[('MV10', 'file')] == '2.0.0', p[('MV10', 'file')])
CONTRACT = [
    ('module-version with the current version reports it unchanged (the code has a branch for it)', _unchanged_skips, False, ['C:D13']),
    ('module-version updates the module.xml it was loaded from', _file_follows, False, ['C:Q4']),
]
