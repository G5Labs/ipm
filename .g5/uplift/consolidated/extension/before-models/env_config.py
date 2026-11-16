"""Family R5: install-time environment configuration (`-env a.json;b.json`), a per-process singleton that every
command clears and reloads in its Reload phase, merges file by file, fills ${VAR} from the OS environment, and that
module code reads at configure time and at compile time (objectgenerator methods, single-process compile).
Native runs: `20261114T033831Z-r5-env` (U1-U11 direct API; S1-S4 one interactive shell session and later commands;
container started with CONS_VAR=hello, CONS_DOLLAR='a$1b', CONS_DOLLAR9='a$9b', CONS_BS='a\\b'),
`20261114T033908Z-r5-env-b` (M0-M4: the IRIS premise that a multi-process compile hides the configuration from a
generator, checked directly). Premise kept: ICU replacement text, where `\\` escapes the next character, `$n` is a
group reference (the pattern has one group) and any other `$` raises <REGULAR EXPRESSION>.
"""
import re, json
from pathlib import Path
ROUND = 'R5'
FAMILY = 'environment-config'
RUNS = ['20261114T033831Z-r5-env', '20261114T033908Z-r5-env-b']
def case_run(case): return RUNS[1] if case.startswith('M') else RUNS[0]
FIX = Path(__file__).resolve().parents[1] / 'native' / 'fixtures' / 'c5'
E, LB = 'General/EnvironmentConfig.cls', 'Lifecycle/Base.cls'
MEMBERS = {
    E + '::Load': {'slice': E + ':9-32', 'fn': 'load', 'behavior': 'missing, malformed or array file raises and clears everything loaded before (U3-U5)'},
    E + '::GetArg': {'slice': E + ':35-48', 'fn': 'get_arg', 'behavior': 'nested lookup; a non-object on the path gives empty (U9)'},
    E + '::%EvaluateHelper': {'slice': E + ':59-90', 'fn': 'evaluate_helper', 'behavior': '${VAR} from the OS environment, recursively; value inserted as ICU replacement text (U7, U8)'},
    E + '::Clear': {'slice': E + ':92-96', 'fn': 'clear', 'behavior': 'resets to empty (U2r, U3)'},
    E + '::IsEmpty': {'slice': E + ':98-101', 'fn': 'is_empty', 'behavior': 'decides the single-process compile (M3, M4)'},
    E + '::MergeDynamicObjects': {'slice': E + ':105-129', 'fn': 'merge', 'behavior': 'deep merge, arrays by index, scalar replaces object, object onto scalar fails (U2, U6)'},
    LB + '::LoadEnvironmentConfig': {'slice': LB + ':2188-2200', 'fn': 'load_environment_config', 'behavior': 'clears, then loads each ;-separated file in order, trimmed (U2r, S1)'},
    'Utils/LegacyCompat.cls::UpdateSuperclassAndCompile': {'slice': 'Utils/LegacyCompat.cls:36-41', 'fn': 'compile_list', 'behavior': 'compiles in one process while a configuration is loaded, so generators see it (M3, M4, S3)'},
    'General/Singleton.cls::%Get': {'slice': 'General/Singleton.cls:36-127', 'fn': 'singleton_get', 'behavior': 'the same in-process instance while referenced, so Configure reads what Reload loaded (S1, U10, U11)'},
}
V = {'v': 'as-built'}
def var(n): return V['v'] == n
VARIANTS = {
    'load-keeps-on-error': {'member': E + '::Load', 'wrong': 'a failing file keeps what earlier files loaded'},
    'load-array-accepted': {'member': E + '::Load', 'wrong': 'a JSON array is merged'},
    'getarg-error-on-scalar': {'member': E + '::GetArg', 'wrong': 'a scalar on the path is returned'},
    'evaluate-literal-value': {'member': E + '::%EvaluateHelper', 'wrong': 'the variable value is inserted literally (intended reading)'},
    'evaluate-no-recursion': {'member': E + '::%EvaluateHelper', 'wrong': 'values inside arrays are not filled'},
    'clear-noop': {'member': E + '::Clear', 'wrong': 'clear keeps the values'},
    'isempty-always-true': {'member': E + '::IsEmpty', 'wrong': 'always reports empty'},
    'merge-arrays-replace': {'member': E + '::MergeDynamicObjects', 'wrong': 'a later array replaces the earlier one'},
    'merge-objects-replace': {'member': E + '::MergeDynamicObjects', 'wrong': 'a later object replaces the earlier one'},
    'merge-object-onto-scalar-ok': {'member': E + '::MergeDynamicObjects', 'wrong': 'an object replaces an earlier scalar'},
    'lec-no-clear': {'member': LB + '::LoadEnvironmentConfig', 'wrong': 'files are merged onto what the previous call loaded'},
    'lec-no-trim': {'member': LB + '::LoadEnvironmentConfig', 'wrong': 'paths are not trimmed'},
    'compile-always-multi': {'member': 'Utils/LegacyCompat.cls::UpdateSuperclassAndCompile', 'wrong': 'always compiles in several processes'},
    'singleton-new-each-get': {'member': 'General/Singleton.cls::%Get', 'wrong': 'every %Get returns a new instance'},
}
ENV = {'CONS_VAR': 'hello', 'CONS_DOLLAR': 'a$1b', 'CONS_DOLLAR9': 'a$9b', 'CONS_BS': 'a\\b'}
class Fail(Exception): pass
class RegexFail(Exception): pass
def icu_replacement(text, whole, group1):
    out, i = '', 0
    while i < len(text):
        c = text[i]
        if c == '\\' and i + 1 < len(text): out += text[i + 1]; i += 2; continue
        if c == '$':
            if i + 1 < len(text) and text[i + 1].isdigit():
                d = int(text[i + 1])
                if d == 0: out += whole
                elif d == 1: out += group1
                else: raise RegexFail()
                i += 2; continue
            raise RegexFail()
        out += c; i += 1
    return out
def evaluate_helper(o):
    if isinstance(o, dict): return {k: evaluate_helper(v) for k, v in o.items()}
    if isinstance(o, list): return o if var('evaluate-no-recursion') else [evaluate_helper(v) for v in o]
    if not isinstance(o, str): return o
    while True:
        m = re.search(r'\$\{([a-zA-Z0-9_]+)\}', o)
        if not m: return o
        val = ENV.get(m.group(1), '')
        rep = val if var('evaluate-literal-value') else icu_replacement(val, m.group(0), m.group(1))
        o = o[:m.start()] + rep + o[m.end():]
def merge(dest, src):
    if isinstance(src, list):
        for i, v in enumerate(src):
            if var('merge-arrays-replace'): dest[:] = src; return
            if i < len(dest): dest[i] = v
            else: dest.append(v)
        return
    for k, v in src.items():
        if isinstance(v, (dict, list)):
            if var('merge-objects-replace') and isinstance(v, dict): dest[k] = json.loads(json.dumps(v)); continue
            if k not in dest: dest[k] = type(v)()
            if not isinstance(dest[k], (dict, list)):
                if var('merge-object-onto-scalar-ok'): dest[k] = json.loads(json.dumps(v)); continue
                raise Fail(f'Invalid arguments. Expected two dynamic objects. Got {dest[k]} and 9@%Library.DynamicObject')
            merge(dest[k], v)
        else:
            dest[k] = v
class Config:
    def __init__(s): s.c = {}
def clear(cfg):
    if not var('clear-noop'): cfg.c = {}
def is_empty(cfg): return True if var('isempty-always-true') else len(cfg.c) == 0
def load(cfg, path):
    try:
        f = FIX / Path(path).name if path.startswith('/tmp/c5/') else None
        if f is None or not f.exists(): raise Fail(f"File '{path}' does not exist")
        try: new = json.loads(f.read_text())
        except ValueError: raise Fail(f'Invalid configuration file: {path}. Expected JSON object.')
        if isinstance(new, list) and not var('load-array-accepted'): raise Fail('Invalid configuration file. Expected a JSON object, not a JSON array')
        if isinstance(new, list): return
        merge(cfg.c, new)
        cfg.c = evaluate_helper(cfg.c)
    except (Fail, RegexFail):
        if not var('load-keeps-on-error'): clear(cfg)
        raise
def load_environment_config(cfg, paths):
    if not var('lec-no-clear'): clear(cfg)
    for p in (paths.split(';') if paths else []):
        p = p if var('lec-no-trim') else p.strip(' \t\r\n')
        load(cfg, p)
def get_arg(cfg, pkg, *args):
    o = cfg.c.get(pkg, '')
    for a in args:
        if not isinstance(o, (dict, list)): return o if var('getarg-error-on-scalar') else ''
        o = o.get(a, '') if isinstance(o, dict) else (o[a] if a < len(o) else '')
    return o
def compile_list(cfg):
    """Returns what an objectgenerator reading the configuration bakes in."""
    single = not is_empty(cfg) and not var('compile-always-multi')
    return get_arg(cfg, 'envmod', 'greeting') if single else ''
SINGLE = {'inst': None}
def singleton_get():
    if var('singleton-new-each-get') or SINGLE['inst'] is None: SINGLE['inst'] = Config()
    return SINGLE['inst']

def fmt(cfg, ok):
    l = get_arg(cfg, 'envmod', 'list'); ls = ''.join(f'{x},' for x in l) if isinstance(l, list) else ''
    k = cfg.c.get('envmod', {}).get('k', '') if isinstance(cfg.c.get('envmod'), dict) else ''
    k = 'object' if isinstance(k, dict) else k
    return (f"{int(ok)} empty={int(is_empty(cfg))} greeting={get_arg(cfg, 'envmod', 'greeting')} x={get_arg(cfg, 'envmod', 'nested', 'x')} "
            f"y={get_arg(cfg, 'envmod', 'nested', 'y')} list={ls} k={k}")
def predict(variant):
    V['v'] = variant; SINGLE['inst'] = None; out = {}
    cfg = singleton_get()
    for case, paths in (('U1', '/tmp/c5/a.json'), ('U2', '/tmp/c5/a.json;/tmp/c5/b.json'), ('U2r', ' /tmp/c5/b.json ; /tmp/c5/a.json '),
                        ('U3', '/tmp/c5/a.json;/tmp/c5/missing.json'), ('U4', '/tmp/c5/a.json;/tmp/c5/bad.json'), ('U5', '/tmp/c5/a.json;/tmp/c5/arr.json'),
                        ('U6', '/tmp/c5/s1.json;/tmp/c5/s2.json'), ('U6r', '/tmp/c5/s2.json;/tmp/c5/s1.json'), ('U7.setup', '/tmp/c5/e.json'),
                        ('U8', '/tmp/c5/e9.json'), ('U9', None), ('U10', '/tmp/c5/a.json')):
        if paths is None:
            # U9 ran right after U8 had cleared the configuration, so it cannot tell GetArg's scalar rule apart.
            out[('U9', 'missing')] = f"[{get_arg(cfg, 'envmod', 'greeting', 'deeper')}] none=[{get_arg(cfg, 'nope')}]"
            continue
        err = None
        try: load_environment_config(cfg, paths); ok = True
        except Fail as ex: ok, err = False, 'ERROR #5012: ' + str(ex) if 'does not exist' in str(ex) else 'ERROR #5001: ' + str(ex)
        except RegexFail: ok, err = False, 'ERROR #5002: ObjectScript error: <REGULAR EXPRESSION>ReplaceFirst+3^%Regex.Matcher.1'
        out[(case, 'ok')] = fmt(cfg, ok)
        if err: out[(case, 'error')] = err
        if case == 'U7.setup':
            e = cfg.c.get('envmod', {})
            lst = e.get('list', [''])
            out[('U7', 'v')] = f"{e.get('v', '')} u=[{e.get('u', '')}] list0={lst[0] if lst else ''} d1={e.get('d1', '')} bs={e.get('bs', '')}"
    out[('U10', 'after.list')] = get_arg(cfg, 'envmod', 'greeting')
    SINGLE['inst'] = None; out[('U11', 'fresh.empty')] = str(int(is_empty(singleton_get())))
    # S: one interactive session. Each command's Reload clears and loads its own -env files; Configure (the Invoke)
    # reads through %Get; the compile reads through the single-process rule.
    recs, gen = [], ''
    def command(paths):
        nonlocal gen
        c = singleton_get()
        try: load_environment_config(c, paths)
        except (Fail, RegexFail): return 'ERR'
        gen = compile_list(c)
        recs.append(get_arg(singleton_get(), 'envmod', 'greeting'))
        return 'OK'
    SINGLE['inst'] = None
    command('/tmp/c5/a.json'); command('/tmp/c5/a.json;/tmp/c5/missing.json'); command(''); command('')
    out[('S1', 'records')] = ''.join(f'[{r}]' for r in recs)
    out[('S2', 'gen')] = f'[{gen}]'
    st = command('/tmp/c5/b.json')
    out[('S3', 'status')] = st; out[('S3', 'tlevel.before')] = '0'; out[('S3', 'tlevel.after')] = '0'
    out[('S3', 'gen')] = f'[{gen}] last=[{recs[-1]}]'
    st = command('/tmp/c5/a.json;/tmp/c5/bad.json')
    out[('S4', 'status')] = st; out[('S4', 'tlevel.before')] = '0'; out[('S4', 'tlevel.after')] = '0'
    if st == 'ERR': out[('S4', 'error')] = 'ERROR #5001: Invalid configuration file: /tmp/c5/bad.json. Expected JSON object.'
    out[('S4', 'mod.envmod')] = 'present version=1.0.0 repo= desc= dev=0'
    out[('S4', 'gen')] = f'[{gen}]'
    # M: compile-mode premise and IPM's rule
    SINGLE['inst'] = None; c = singleton_get(); load_environment_config(c, '/tmp/c5/a.json')
    out[('M0', 'greeting')] = get_arg(c, 'envmod', 'greeting')
    out[('M1', 'multi')] = '1 gen=[]'                       # IRIS premise, observed directly
    out[('M2', 'single')] = f"1 gen=[{get_arg(c, 'envmod', 'greeting')}]"
    out[('M3', 'ipm')] = f'1 gen=[{compile_list(c)}]'
    clear(c); out[('M4', 'ipm.empty')] = f'1 gen=[{compile_list(c)}]'
    return out
def _later_command_keeps(p): return (p[('S1', 'records')] == '[first][first][first]', p[('S1', 'records')])
def _value_literal(p): return ('d1=a$1b' in p[('U7', 'v')] and 'bs=a\\b' in p[('U7', 'v')], p[('U7', 'v')])
def _failed_file_keeps_earlier(p): return (p[('U3', 'ok')].startswith('0 empty=0 greeting=first'), p[('U3', 'ok')])
CONTRACT = [
    ('an OS variable value is inserted literally', _value_literal, False, ['C:D14']),
    ('a failing second file keeps the first file\'s values (one of the plan\'s policy alternatives)', _failed_file_keeps_earlier, False, ['C:Q6']),
    ('a later command in the same shell session sees the earlier -env values', _later_command_keeps, False, ['plan: environment row']),
]
