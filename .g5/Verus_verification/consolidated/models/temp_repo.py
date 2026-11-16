"""Family R1-A: the temporary `.modules` repository around a top-level load, and the repository search order that
decides which copy of a dependency is installed. Executable twin of IPM 571e04a1, compared field by field with the
native run `20261114T025102Z-r1-temp-repo` (script native/scripts/r1-temp-repo.cos, fixtures native/fixtures/c1).

Abstractions (premises): module-name validity is modeled as "no whitespace" (the fixture's invalid name has a space);
version expressions are only the forms the fixtures use (exact `X.Y.Z` and `>=X.Y.Z`); repository IDs increase by one
for every repository save, including a save later rolled back (IRIS $INCREMENT is not undone by TROLLBACK; observed:
IDs 1..5 in TR2..TR5); a filesystem repository caches every module.xml under its root.
"""
import re
from pathlib import Path
ROUND = 'R1'
FAMILY = 'temp-repo-and-search-order'
RUNS = ['20261114T025102Z-r1-temp-repo', '20261114T025551Z-r1-eval-postcommit']
FIX = Path(__file__).resolve().parents[1] / 'native' / 'fixtures' / 'c1'
FIXE = Path(__file__).resolve().parents[1] / 'native' / 'fixtures' / 'c1e'

def case_run(case):
    return RUNS[1] if case.startswith('D') else RUNS[0]

M = 'General/TempLocalRepoManager.cls'
MEMBERS = {
    M + '::SkipCreate': {'slice': M + ':33-47', 'fn': 'skip_create', 'behavior': 'no temporary repository when another repository already has the .modules root (TR4)'},
    M + '::Create': {'slice': M + ':13-31', 'fn': 'create', 'behavior': 'first free ipm-temp-modules-N name; BuildCache failure leaves no row (TR3, TR7)'},
    M + '::%OnNew': {'slice': M + ':49-71', 'fn': 'on_new', 'behavior': 'creation failure runs CleanUp and returns an error, so %New yields no object (TR7)'},
    M + '::CleanUp': {'slice': M + ':73-85', 'fn': 'cleanup', 'behavior': 'deletes the repository it created; reports a failure for a row already rolled back (TR2, TR7)'},
    'Repo/Manager.cls::SearchRepositoriesForModule': {'slice': 'Repo/Manager.cls:55-192', 'fn': 'search', 'behavior': 'candidates ordered by version descending, then repository ID ascending; first wins (TR5, TR5b, TR5c)'},
    'Utils/Module.cls::LoadModuleReference': {'slice': 'Utils/Module.cls:135-140', 'fn': 'load_internal', 'behavior': 'dependency-level temporary repository (useFirst=0); CleanUp on an empty reference fails the dependency inside the parent transaction (Dtxn, Ddev)'},
    'Main.cls::LoadInternal': {'slice': 'Main.cls:2525-2532', 'fn': 'load_internal', 'behavior': 'temporary repository around LoadNewModule; unguarded CleanUp after the commit (TR7)'},
}
VARIANTS = {
    'skip-ignores-existing-root': {'member': M + '::SkipCreate', 'wrong': 'tries to create a temporary repository on a root another repository already has (the unique Root index then fails it)'},
    'skip-ignores-missing-dir': {'member': M + '::SkipCreate', 'wrong': 'creates (and removes) an empty repository for a missing folder; expected to survive: no fixture observes it'},
    'create-always-first-name': {'member': M + '::Create', 'wrong': 'always names the repository ipm-temp-modules-1'},
    'create-sort-last': {'member': M + '::Create', 'wrong': 'ignores useFirst (sort 1000); expected to survive: sort order never decides a result'},
    'onnew-no-cleanup-in-catch': {'member': M + '::%OnNew', 'wrong': 'does not call CleanUp when Create fails'},
    'onnew-ok-on-failure': {'member': M + '::%OnNew', 'wrong': 'returns OK after a Create failure, so a manager object exists'},
    'cleanup-noop': {'member': M + '::CleanUp', 'wrong': 'never deletes the repository'},
    'cleanup-quiet': {'member': M + '::CleanUp', 'wrong': 'reports no message when the delete fails'},
    'search-tie-by-sortorder': {'member': 'Repo/Manager.cls::SearchRepositoriesForModule', 'wrong': 'breaks version ties by repository sort order (the temporary repository first)'},
    'search-first-repo': {'member': 'Repo/Manager.cls::SearchRepositoriesForModule', 'wrong': 'takes the first repository (by sort order) that has any satisfying version'},
    'search-lowest-version': {'member': 'Repo/Manager.cls::SearchRepositoriesForModule', 'wrong': 'orders versions ascending'},
    'lmr-guarded-cleanup': {'member': 'Utils/Module.cls::LoadModuleReference', 'wrong': 'the dependency path guards CleanUp, so the dependency load succeeds'},
    'lmr-failure-keeps-dependency': {'member': 'Utils/Module.cls::LoadModuleReference', 'wrong': 'the failed dependency stays installed (no parent rollback)'},
    'loadinternal-guarded-cleanup': {'member': 'Main.cls::LoadInternal', 'wrong': 'skips CleanUp when the manager is missing, so the load succeeds'},
    'late-error-rolls-back': {'member': 'Main.cls::LoadInternal', 'wrong': 'an error after the commit removes the module'},
    'late-error-reports-success': {'member': 'Main.cls::LoadInternal', 'wrong': 'an error after the commit is reported as success'},
}

def parse_module(path):
    t = (path / 'module.xml').read_text()
    name = re.search(r'<Name>([^<]*)</Name>', t).group(1)
    ver = re.search(r'<Version>([^<]*)</Version>', t).group(1)
    desc = (re.search(r'<Description>([^<]*)</Description>', t) or [None, ''])[1]
    deps = re.findall(r'<ModuleReference>\s*<Name>([^<]*)</Name>\s*(?:<Version>([^<]*)</Version>)?', t)
    return {'name': name, 'version': ver, 'desc': desc, 'deps': deps, 'dir': path}

def tmp(p):  # container path -> fixture path
    if p.startswith('/tmp/c1e'): return FIXE / Path(p).relative_to('/tmp/c1e')
    return FIX / Path(p).relative_to('/tmp/c1') if p.startswith('/tmp/c1') else None

def vkey(v): return tuple(int(x) for x in v.split('.'))

def satisfies(v, expr):
    if expr.startswith('>='): return vkey(v) >= vkey(expr[2:])
    return v == expr

class State:
    def __init__(s, var):
        s.var, s.repos, s.next_id, s.mods, s.hist = var, {}, 1, {}, {}
    def scan(s, root, depth=0):
        r = tmp(root)
        if not (r and r.exists()): return []
        found = [p for p in sorted(r.rglob('module.xml')) if depth == 0 or len(p.parent.relative_to(r).parts) <= depth]
        return [parse_module(p.parent) for p in found]

def skip_create(s, location):
    exists = tmp(location) is not None and tmp(location).exists()
    if not exists and s.var != 'skip-ignores-missing-dir': return True
    if s.var != 'skip-ignores-existing-root' and any(r['root'] == location for r in s.repos.values()): return True
    return False

def create(s, location, use_first):
    """Returns (repo dict or None, ok, id_assigned). BuildCache: save inside its own transaction, then scan;
    an invalid module name rolls the save back but the in-memory object keeps its ID."""
    n = 1
    if s.var != 'create-always-first-name':
        while any(r['name'] == f'ipm-temp-modules-{n}' for r in s.repos.values()): n += 1
    name = f'ipm-temp-modules-{n}'
    rid = s.next_id; s.next_id += 1
    sort = -1000 if (use_first and s.var != 'create-sort-last') else 1000
    repo = {'id': rid, 'name': name, 'root': location, 'sort': sort, 'mods': s.scan(location)}
    if any(r['name'] == name for r in s.repos.values()):
        return repo, False      # unique name violated: save fails, nothing stored
    if any(r['root'] == location for r in s.repos.values()):
        return repo, False      # unique Root index (Repo/Filesystem/Definition.cls:10): save fails, nothing stored
    if any(re.search(r'\s', m['name']) for m in repo['mods']):
        return repo, False      # cache validation fails: rolled back
    s.repos[rid] = repo
    return repo, True

def cleanup(s, repo, log):
    if repo is None: return 'OK'
    if s.var == 'cleanup-noop': return 'OK'
    if repo['id'] in s.repos and s.repos[repo['id']] is repo:
        del s.repos[repo['id']]; return 'OK'
    if s.var != 'cleanup-quiet': log.append(repo['name'])
    return 'ERR'

def on_new(s, location, use_first, log):
    """Returns the manager's repository (None when nothing was created) or the string 'FAILED' when %New returns ''."""
    if skip_create(s, location): return None
    repo, ok = create(s, location, use_first)
    if ok: return repo
    if s.var != 'onnew-no-cleanup-in-catch': cleanup(s, repo, log)
    if s.var == 'onnew-ok-on-failure': return repo
    return 'FAILED'

def search(s, name, expr):
    cands = []
    for r in s.repos.values():
        for m in r['mods']:
            if m['name'] == name and satisfies(m['version'], expr): cands.append((m, r))
    if not cands: return None
    if s.var == 'search-first-repo':
        first = min(cands, key=lambda c: (c[1]['sort'], c[1]['id']))[1]
        cands = [c for c in cands if c[1] is first]
        return max(cands, key=lambda c: vkey(c[0]['version']))
    if s.var == 'search-lowest-version':
        return min(cands, key=lambda c: (vkey(c[0]['version']), c[1]['id']))
    tie = (lambda c: c[1]['sort']) if s.var == 'search-tie-by-sortorder' else (lambda c: c[1]['id'])
    return sorted(cands, key=lambda c: ([-x for x in vkey(c[0]['version'])], tie(c)))[0]

def load_internal(s, d, dev=0):
    log = []
    mgr = on_new(s, d.rstrip('/') + '/.modules/', 1, log)
    main = parse_module(tmp(d))
    status, err = 'OK', None
    for dn, dv in main['deps']:
        hit = search(s, dn, dv)
        if hit is None: status, err = 'ERR', f'Could not find satisfactory version of {dn}'; break
        m, r = hit
        dep_mgr = on_new(s, str(m['dir']).replace(str(FIXE), '/tmp/c1e').replace(str(FIX), '/tmp/c1') + '/.modules/', 0, log)
        s.mods[m['name']] = {'version': m['version'], 'repo': r['name'], 'desc': m['desc']}
        if dep_mgr == 'FAILED' and s.var != 'lmr-guarded-cleanup':   # CleanUp() on '' inside LoadModuleReference's try
            status, err = 'ERR', 'ERROR #5002: ObjectScript error: <INVALID OREF>LoadModuleReference+80^%IPM.Utils.Module.1'
            break
        cleanup(s, dep_mgr if isinstance(dep_mgr, dict) else None, log)
    if status == 'OK':
        s.mods[main['name']] = {'version': main['version'], 'repo': '', 'desc': main['desc'], 'dev': dev}
        s.hist[main['name']] = {'success': 1, 'committed': 1}
    elif 'LoadModuleReference' in err:
        # The parent's catch rolls back to its entry level. Normal mode: all of it, including the dependency and
        # the dependency's history row. Developer mode: the manifest committed before the dependencies survives.
        if s.var != 'lmr-failure-keeps-dependency':
            for dn, _ in main['deps']: s.mods.pop(dn, None)
        if dev: s.mods[main['name']] = {'version': main['version'], 'repo': '', 'desc': main['desc'], 'dev': 1}
        s.hist[main['name']] = {'success': 0, 'committed': dev}
    if mgr == 'FAILED':
        if s.var == 'loadinternal-guarded-cleanup':
            return status, err, log
        status, err = 'ERR', 'ERROR #5002: ObjectScript error: <INVALID OREF>LoadInternal+83^%IPM.Main.1'
        if s.var == 'late-error-reports-success': status, err = 'OK', None
        if s.var == 'late-error-rolls-back': s.mods.pop(main['name'], None)
        s.hist[main['name']] = {'success': 0, 'committed': 1}
        return status, err, log
    sc = cleanup(s, mgr if isinstance(mgr, dict) else None, log)
    if sc == 'ERR' and status == 'OK': status, err = 'ERR', 'cleanup error'
    return status, err, log

STEPS = [('repos', 'init'),
    ('run', 'TR1', 'load /tmp/c1/tr1/'), ('mod', 'TR1', 'tr1-main'), ('repos', 'TR1'),
    ('run', 'TR2', 'load /tmp/c1/tr2/'), ('mod', 'TR2', 'tr2-dep'), ('repos', 'TR2'),
    ('run', 'TR3.setup', 'repo -fs -name ipm-temp-modules-1 -path /tmp/c1/elsewhere/'),
    ('run', 'TR3', 'load /tmp/c1/tr3/'), ('mod', 'TR3', 'tr3-dep'), ('repos', 'TR3'),
    ('run', 'TR3.teardown', 'repo -delete -n ipm-temp-modules-1'),
    ('run', 'TR4.setup', 'repo -fs -name keepme -path /tmp/c1/tr4/.modules/'),
    ('run', 'TR4', 'load /tmp/c1/tr4/'), ('mod', 'TR4', 'tr4-dep'), ('repos', 'TR4'),
    ('run', 'TR4.teardown', 'repo -delete -n keepme'),
    ('run', 'TR5.setup', 'repo -fs -name reg5 -path /tmp/c1/reg5/'),
    ('run', 'TR5', 'load /tmp/c1/tr5/'), ('mod', 'TR5', 'tr5-dep'), ('repos', 'TR5'),
    ('run', 'TR5b', 'load /tmp/c1/tr5b/'), ('mod', 'TR5b', 'tr5b-dep'),
    ('run', 'TR5c', 'load /tmp/c1/tr5c/'), ('mod', 'TR5c', 'tr5c-dep'), ('repos', 'TR5c'),
    ('run', 'TR5.teardown', 'repo -delete -n reg5'),
    ('run', 'TR7', 'load /tmp/c1/tr7/'), ('mod', 'TR7', 'tr7-main'), ('hist', 'TR7', 'tr7-main'), ('repos', 'TR7'),
    ('run', 'TR7b', 'load /tmp/c1/tr7b/'), ('mod', 'TR7b', 'tr7b-main'), ('mod', 'TR7b', 'tr7b-unused'),
    ('hist', 'TR7b', 'tr7b-main'), ('repos', 'TR7b'),
    # second run (same container, state carried over): dependency-level .modules failure, normal vs developer mode
    ('regd', 'D.setup'),
    ('run', 'Dtxn', 'load /tmp/c1e/d-txn/'), ('mod', 'Dtxn', 'dd-parent-txn'), ('mod', 'Dtxn', 'dd-dep-txn'),
    ('hist', 'Dtxn', 'dd-parent-txn'), ('hist', 'Dtxn', 'dd-dep-txn'), ('repos', 'Dtxn'),
    ('run', 'Ddev', 'load /tmp/c1e/d-dev/ -dev'), ('mod', 'Ddev', 'dd-parent-dev'), ('mod', 'Ddev', 'dd-dep-dev'),
    ('hist', 'Ddev', 'dd-parent-dev'), ('hist', 'Ddev', 'dd-dep-dev'), ('repos', 'Ddev'),
    ('run', 'Dok', 'load /tmp/c1e/d-neighbor/'), ('mod', 'Dok', 'dd-parent-ok'), ('mod', 'Dok', 'dd-dep-ok'), ('repos', 'Dok')]

def predict(var):
    s, out = State(var), {}
    for st in STEPS:
        kind, case = st[0], st[1]
        if kind == 'run':
            cmd = st[2].split()
            log = []
            if cmd[0] == 'load':
                status, err, log = load_internal(s, cmd[1], 1 if '-dev' in cmd else 0)
            elif cmd[:2] == ['repo', '-fs']:
                name, path = cmd[3], cmd[5]
                s.repos[s.next_id] = {'id': s.next_id, 'name': name, 'root': path, 'sort': 2, 'mods': s.scan(path)}
                s.next_id += 1; status, err = 'OK', None
            elif cmd[:2] == ['repo', '-delete']:
                rid = next(k for k, r in s.repos.items() if r['name'] == cmd[3]); del s.repos[rid]; status, err = 'OK', None
            out[(case, 'status')] = status
            if err and 'INVALID OREF' in err: out[(case, 'error')] = err
            out[(case, 'tlevel.before')] = '0'; out[(case, 'tlevel.after')] = '0'
            out[(case, 'log.cleanup_failed')] = ','.join(log) if log else 'none'
        elif kind == 'mod':
            m = s.mods.get(st[2])
            out[(case, 'mod.' + st[2])] = 'absent' if m is None else f"present version={m['version']} repo={m['repo']} desc={m['desc']} dev={m.get('dev', 0)}"
        elif kind == 'hist':
            h = s.hist.get(st[2])
            out[(case, 'hist.' + st[2])] = 'none' if h is None else f"action:load success:{h['success']} committed:{h['committed']} finalized:1"
        elif kind == 'regd':   # ObjectScript setup: filesystem repository regd with Depth=1 (D.setup)
            s.repos[s.next_id] = {'id': s.next_id, 'name': 'regd', 'root': '/tmp/c1e/regd/', 'sort': 2, 'mods': s.scan('/tmp/c1e/regd/', 1)}
            s.next_id += 1
        elif kind == 'repos':
            for r in sorted(s.repos.values(), key=lambda r: r['id']):
                out[(case, 'repo.' + r['name'])] = f"id:{r['id']} sort:{r['sort']} root:{r['root']}"
            out[(case, 'repos.count')] = str(len(s.repos))
    return out

def _no_temp_left(p):
    bad = [c for (c, f), v in p.items() if f.startswith('repo.ipm-temp-modules') and not c.startswith('TR3')]
    return (not bad, bad)
def _foreign_kept(p):
    return (p[('TR3', 'repo.ipm-temp-modules-1')].endswith('/tmp/c1/elsewhere/') and ('TR4', 'repo.keepme') in p, '')
def _dotmodules_preferred(p):
    v = p[('TR5', 'mod.tr5-dep')]
    return ('desc=from-dotmodules' in v, v)
def _error_means_not_installed(p):
    v = p[('TR7', 'mod.tr7-main')]
    return (not (p[('TR7', 'status')] == 'ERR' and v.startswith('present')), f"TR7 status={p[('TR7','status')]} {v}")
def _cleanup_msg_names_registered(p):
    names = p[('TR7', 'log.cleanup_failed')]
    return (names == 'none' or all(('TR7', 'repo.' + n) in p for n in names.split(',')), names)
CONTRACT = [
    ('a load leaves no temporary repository registered', _no_temp_left, True, ['C:R1-1']),
    ('a load keeps repositories it did not create', _foreign_kept, True, ['C:R1-2']),
    ('the .modules copy wins over an equal version elsewhere (TempLocalRepoManager comment intent)', _dotmodules_preferred, False, ['C:D1']),
    ('an error status means the module was not installed', _error_means_not_installed, False, ['G:D13', 'P:F-LOAD-4 family']),
    ('a cleanup-failure message names a repository that still exists', _cleanup_msg_names_registered, False, ['C:D2']),
]
