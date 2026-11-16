"""Family R1-B: attribute expression evaluation (module defaults, ${..}/{$..} placeholders, system expressions,
$$$macros, {name}, #{expr}) and the after-install message evaluated after the final commit.
Twin of IPM 571e04a1 compared with native runs `20261114T025636Z-r1-evaluator` (corpus E01-E36, no install) and
`20261114T025551Z-r1-eval-postcommit` (P1-P4 loads; its E part is superseded by the corpus run).

Premises: instance values are those the run printed (OBS ENV): namespace USER, manager directory /usr/irissys/mgr/,
module root /usr/irissys/ipm/. ICU replacement text: a `$` in the replacement raises <REGULAR EXPRESSION> when a
match is replaced (observed E18, E32 vs E33); a backslash is escaped by the product. #{expr} expressions are the
corpus forms only (integer arithmetic, string literal, undefined variable). $$$OK expands to 1; $$$ThrowOnError takes
arguments, so `Quit $$$ThrowOnError` does not compile; $$$NoSuchMacroXyz is undefined.
"""
import re
ROUND = 'R1'
FAMILY = 'attribute-evaluator'
RUNS = ['20261114T025636Z-r1-evaluator', '20261114T025551Z-r1-eval-postcommit']
LOG_TAGS = {'aftermsg': r'^(Done .*)$'}
U, S, D = 'Utils/Module.cls', 'Storage/Module.cls', 'Storage/ModuleSetting/Default.cls'
MEMBERS = {
    U + '::%RegExReplace': {'slice': U + ':2804-2815', 'fn': 'regex_replace', 'behavior': 'case-insensitive {$n}/${n} replacement; name is a regex; only backslash escaped in the value (E04, E21, E18, E32)'},
    U + '::%EvaluateSystemExpression': {'slice': U + ':2697-2744', 'fn': 'system_expression', 'behavior': 'namespace, namespaceLower, mgrDir, ... substitution (E02-E05, E23)'},
    U + '::%EvaluateMacro': {'slice': U + ':2746-2759', 'fn': 'evaluate_macro', 'behavior': '$$$name replaced by its value, undefined -> empty (E12, E16, E17)'},
    U + '::EvaluateMacroHelper': {'slice': U + ':2763-2785', 'fn': 'macro_helper', 'behavior': 'compiles %IPM.Generated.MacroEval; a compile error throws and leaves the class (P2)'},
    D + '::GetCustomParameters': {'slice': D + ':14-31', 'fn': 'custom_parameters', 'behavior': 'module defaults, overridden by command parameters of the same name (E06, E07)'},
    D + '::EvaluateAttribute': {'slice': D + ':47-58', 'fn': 'evaluate_attribute', 'behavior': 'applies every custom parameter to the attribute (E06, E22)'},
    D + '::ResolvePlaceholders': {'slice': D + ':63-117', 'fn': 'resolve_placeholders', 'behavior': 'fixed point over parameters, local first then system expression; never raises on a cycle (E19, E22, E30, E31, E36)'},
    S + '::%Evaluate': {'slice': S + ':1833-1870', 'fn': 'evaluate', 'behavior': 'returns the value unchanged without "{"; built-ins override defaults; {name}; #{expr} (E15, E20, E08, E10)'},
    S + '::%EvaluateExpression': {'slice': S + ':1885-1894', 'fn': 'evaluate_expression', 'behavior': 'strips leading blanks; an error yields empty (E11, E13, E14)'},
}
VARIANTS = {
    'regex-escape-dollar': {'member': U + '::%RegExReplace', 'wrong': 'escapes $ in the value, so a value with $ is substituted literally'},
    'regex-case-sensitive': {'member': U + '::%RegExReplace', 'wrong': 'matches names case-sensitively'},
    'regex-literal-name': {'member': U + '::%RegExReplace', 'wrong': 'treats the parameter name literally (no regex)'},
    'regex-dollar-brace-only': {'member': U + '::%RegExReplace', 'wrong': 'supports only ${name}'},
    'sysexpr-no-lower': {'member': U + '::%EvaluateSystemExpression', 'wrong': 'has no namespaceLower'},
    'sysexpr-mgr-is-install': {'member': U + '::%EvaluateSystemExpression', 'wrong': 'mgrDir resolves to the install directory'},
    'macro-skip': {'member': U + '::%EvaluateMacro', 'wrong': 'does not expand $$$ macros'},
    'macro-undefined-kept': {'member': U + '::%EvaluateMacro', 'wrong': 'keeps an undefined macro as text'},
    'macrohelper-swallow': {'member': U + '::EvaluateMacroHelper', 'wrong': 'a macro that does not compile yields empty, no error'},
    'macrohelper-delete-on-error': {'member': U + '::EvaluateMacroHelper', 'wrong': 'removes the generated class even when compilation fails'},
    'params-no-override': {'member': D + '::GetCustomParameters', 'wrong': 'command parameters do not override defaults'},
    'evalattr-skip': {'member': D + '::EvaluateAttribute', 'wrong': 'does not substitute custom parameters'},
    'resolve-single-pass': {'member': D + '::ResolvePlaceholders', 'wrong': 'one substitution pass only'},
    'resolve-raises-on-cycle': {'member': D + '::ResolvePlaceholders', 'wrong': 'raises the circular-reference error its code names (intended reading)'},
    'resolve-no-system-fallback': {'member': D + '::ResolvePlaceholders', 'wrong': 'does not resolve system expressions inside parameter values'},
    'evaluate-no-early-return': {'member': S + '::%Evaluate', 'wrong': 'expands macros even without "{"'},
    'evaluate-defaults-override-builtins': {'member': S + '::%Evaluate', 'wrong': 'a default named version overrides the module version'},
    'evaluate-no-name': {'member': S + '::%Evaluate', 'wrong': 'does not replace {name}'},
    'expr-error-propagates': {'member': S + '::%EvaluateExpression', 'wrong': 'an expression error propagates'},
    'expr-no-strip': {'member': S + '::%EvaluateExpression', 'wrong': 'does not strip leading blanks'},
    'aftermsg-raw': {'member': None, 'wrong': 'prints the message unevaluated'},
    'aftermsg-guarded': {'member': None, 'wrong': 'an evaluation error is caught and the load succeeds'},
    'aftermsg-before-commit': {'member': None, 'wrong': 'the message is evaluated before the final commit, so its failure removes the module'},
}
ENV = {'namespace': 'USER', 'mgrDir': '/usr/irissys/mgr/', 'moduleroot': '/usr/irissys/ipm/'}
MACROS = {'OK': '1'}
NOT_COMPILABLE = {'ThrowOnError'}
class RegexError(Exception): pass
class CompileError(Exception): pass
V = {'v': 'as-built'}

def regex_replace(s, name, value):
    flags = 0 if V['v'] == 'regex-case-sensitive' else re.I
    pat = re.escape(name) if V['v'] == 'regex-literal-name' else name
    forms = [r'\$\{' + pat + r'\}'] if V['v'] == 'regex-dollar-brace-only' else [r'\{\$' + pat + r'\}', r'\$\{' + pat + r'\}']
    for f in forms:
        if re.search(f, s, flags):
            if '$' in value and V['v'] != 'regex-escape-dollar': raise RegexError()
            s = re.sub(f, lambda m: value, s, flags=flags)
    return s

def system_expression(s):
    if '{' not in s: return s
    vals = [('namespace', ENV['namespace']), ('ns', ENV['namespace'])]
    if V['v'] != 'sysexpr-no-lower': vals.append(('namespaceLower', ENV['namespace'].lower()))
    vals.append(('mgrDir', '/usr/irissys/' if V['v'] == 'sysexpr-mgr-is-install' else ENV['mgrDir']))
    for n, v in vals: s = regex_replace(s, n, v)
    return s

def macro_helper(name):
    if name in NOT_COMPILABLE and V['v'] != 'macrohelper-swallow':
        if V['v'] != 'macrohelper-delete-on-error': STATE['macroclass'] = 1
        raise CompileError()
    return MACROS.get(name, '' if V['v'] != 'macro-undefined-kept' else '$$$' + name)

def evaluate_macro(s):
    if '$$$' not in s or V['v'] == 'macro-skip': return s
    for g in re.findall(r'\$\$\$[a-zA-Z0-9]+', s):
        s = s.replace(g, macro_helper(g[3:]))
    return s

def custom_parameters(defaults, params):
    cp = dict(defaults)
    if V['v'] != 'params-no-override':
        for k in cp:
            if k in params: cp[k] = params[k]
    return cp

def osort(d): return sorted(d)   # ObjectScript string subscript collation for these ASCII names

def evaluate_attribute(s, cp):
    if V['v'] == 'evalattr-skip': return s
    for n in osort(cp): s = regex_replace(s, n, cp[n])
    return s

def resolve_placeholders(cp):
    found, levels = True, 20
    while found and levels > 0:
        found = False; levels -= 1
        for p in osort(cp):
            data = cp[p]
            if '{' not in data: continue
            initial = data
            for delim in ('${', '{$'):
                if delim not in data: continue
                seen = set()
                for chunk in data.split(delim)[1:]:
                    key = chunk.split('}')[0]
                    if not key or key in seen: continue
                    seen.add(key); search = delim + key + '}'
                    if key in cp: data = data.replace(search, cp[key])
                    elif V['v'] != 'resolve-no-system-fallback':
                        r = system_expression(search)
                        if r != search: data = data.replace(search, r)
            if data != initial:
                cp[p] = data; found = True
        if V['v'] == 'resolve-single-pass': break
        if V['v'] == 'resolve-raises-on-cycle' and found and levels == 0: raise RuntimeError('circular')
    if V['v'] == 'resolve-raises-on-cycle' and any('{' in v and ('${' + k + '}' in v or '{$' + k + '}' in v) for k, v in cp.items()):
        raise RuntimeError('circular')

def evaluate_expression(expr):
    e = expr if V['v'] == 'expr-no-strip' else expr.lstrip()
    try:
        if e != e.strip(): raise SyntaxError()
        if re.fullmatch(r'"[^"]*"', e): return e[1:-1]
        if re.fullmatch(r'\d+([+\-*/]\d+)*', e): 
            v = int(re.match(r'\d+', e).group())
            for op, n in re.findall(r'([+\-*/])(\d+)', e):
                n = int(n); v = v + n if op == '+' else v - n if op == '-' else v * n if op == '*' else v / n
            return str(int(v)) if v == int(v) else str(v)
        raise NameError(e)
    except Exception:
        if V['v'] == 'expr-error-propagates': raise
        return ''

def evaluate(mod, s, params):
    if '{' not in s and V['v'] != 'evaluate-no-early-return': return s
    cp = custom_parameters(mod['defaults'], params)
    builtins = {'root': (mod['root'].rstrip('/') + '/') if mod['root'] else '', 'packagename': mod['name'], 'version': mod['version'],
                'verbose': str(int(params.get('Verbose', 0))), 'ipmDir': ENV['moduleroot'] + mod['name'] + '/' + mod['version'] + '/'}
    for k, v in builtins.items():
        if not (k == 'version' and V['v'] == 'evaluate-defaults-override-builtins' and k in cp): cp[k] = v
    s = evaluate_macro(s)
    resolve_placeholders(cp)
    s = evaluate_attribute(s, cp)
    s = system_expression(s)
    if V['v'] != 'evaluate-no-name': s = re.sub(r'(?i)\{name\}', lambda m: mod['name'], s)
    while True:
        m = re.search(r'#\{([^}]+)\}', s)
        if not m: break
        s = s[:m.start()] + evaluate_expression(m.group(1)) + s[m.end():]
    return s

STATE = {}
def after_install_message(mod, msg):
    """Returns (printed text or None, error or None). Runs after the final commit (Utils/Module.cls:1348-1354)."""
    if not msg: return None, None
    if V['v'] == 'aftermsg-raw': return msg, None
    try:
        return evaluate(mod, msg, {}), None
    except CompileError:
        if V['v'] == 'aftermsg-guarded': return None, None
        return None, 'ERROR #5475: Error compiling routine: %IPM.Generated.MacroEval.  Errors:  %IPM.Generated.MacroEval.cls'

EVALMOD = {'name': 'evalmod', 'version': '1.2.3', 'root': '/tmp/evalroot',
           'defaults': {'greeting': 'hello', 'a': '${b}', 'b': '${a}', 'c': '${d}', 'd': '${namespace}', 'version': '9.9.9',
                        'a.b': 'DOT', 'e': '{$f}x', 'f': '${greeting}', 'win': 'C:\\dir\\sub'}}
PRICEMOD = {'name': 'pricemod', 'version': '1.0.0', 'root': '', 'defaults': {'price': '$5'}}
CORPUS = [('E01', 'plain text'), ('E02', '{$namespace}'), ('E03', '${namespace}'), ('E04', '{$NAMESPACE}'), ('E05', '{$namespaceLower}'),
    ('E06', '${greeting} world'), ('E08', '{name}/{NAME}'), ('E09', '{$packagename}-{$version}'), ('E10', '#{1+2}'), ('E11', '#{ 1+2}'),
    ('E12', '#{$$$OK}'), ('E13', '#{1/0}'), ('E14', '#{undefinedvar}'), ('E15', '$$$OK no brace'), ('E16', '{$x} $$$OK'),
    ('E17', '$$$NoSuchMacroXyz {}'), ('E18', '${a}'), ('E19', '${c}'), ('E20', '{$version}'), ('E21', '{$axb}'), ('E22', '${e}'),
    ('E23', '{$mgrDir}'), ('E25', '${verbose}'), ('E26', '{$root}'), ('E27', '{$ipmDir}'), ('E28', '{$namespace}#{"-"}{$NAMESPACELOWER}'),
    ('E34', '${win}'), ('E35', '{$b}')]

def _obs(mod, s, params=None):
    try: return 'value', evaluate(mod, s, params or {})
    except RegexError: return 'error', '<REGULAR EXPRESSION>'
    except (ZeroDivisionError, NameError, SyntaxError): return 'error', '<DIVIDE>'
    except RuntimeError: return 'error', 'circular'

def predict(var):
    V['v'] = var; STATE.clear(); out = {}
    for cid, s in CORPUS:
        k, v = _obs(EVALMOD, s); out[(cid, k)] = v
    k, v = _obs(EVALMOD, '${greeting}', {'greeting': 'hi', 'Verbose': 1}); out[('E07', k)] = v
    k, v = _obs(EVALMOD, '${verbose}', {'greeting': 'hi', 'Verbose': 1}); out[('E24', k)] = v
    k, v = _obs(PRICEMOD, '{$price}'); out[('E32', k)] = v
    k, v = _obs(PRICEMOD, '{$namespace}'); out[('E33', k)] = v
    for case, cp in (('E30', {'a': '${b}', 'b': '${a}'}),):
        try: resolve_placeholders(cp); out[(case, 'resolve.raised')] = f"0 a={cp['a']} b={cp['b']}"
        except RuntimeError: out[(case, 'resolve.raised')] = '1 a= b='
    cp = {'x': '${y}', 'y': '${z}', 'z': 'end'}; resolve_placeholders(cp); out[('E31', 'x')] = f"end y={cp['y']}" if cp['x'] == 'end' else f"{cp['x']} y={cp['y']}"
    cp = {'p': 'a', 'q': '${p}${p}{$p}'}; resolve_placeholders(cp); out[('E36', 'q')] = cp['q']
    out[('E29', 'macroclass')] = '0'
    # P: loads whose after-install message is evaluated after the final commit (txn, notxn, dev)
    loads = [('P1', 'pc-ok', 'Done {$namespace} {name}', 0), ('P2txn', 'pc-bad-txn', 'Done {x} $$$ThrowOnError', 0),
             ('P2notxn', 'pc-bad-notxn', 'Done {x} $$$ThrowOnError', 0), ('P2dev', 'pc-bad-dev', 'Done {x} $$$ThrowOnError', 1),
             ('P3', 'pc-neighbor', 'Done {x} $$$OK', 0), ('P4', 'pc-nobrace', 'Done $$$OK', 0)]
    for case, name, msg, dev in loads:
        STATE['macroclass'] = 0
        mod = {'name': name, 'version': '1.0.0', 'root': '', 'defaults': {}}
        text, err = after_install_message(mod, msg)
        installed = not (err and var == 'aftermsg-before-commit')
        out[(case, 'status')] = 'ERR' if err else 'OK'
        if err: out[(case, 'error')] = err
        out[(case, 'tlevel.before')] = '0'; out[(case, 'tlevel.after')] = '0'
        out[(case, 'log.aftermsg')] = text if text is not None else 'none'
        if case != 'P4':
            out[(case, 'mod.' + name)] = f'present version=1.0.0 repo= desc= dev={dev}' if installed else 'absent'
            out[(case, 'hist.' + name)] = f"action:load success:{0 if err else 1} committed:{1 if installed else 0} finalized:1"
        if case == 'P2txn': out[(case, 'macroclass')] = str(STATE['macroclass'])
    return out

def _cycle_raises(p):
    return (p[('E30', 'resolve.raised')].startswith('1'), p[('E30', 'resolve.raised')])
def _dollar_value_substituted(p):
    return (p.get(('E32', 'value')) == '$5', p.get(('E32', 'value'), p.get(('E32', 'error'))))
def _late_error_truthful(p):
    return (not (p[('P2txn', 'status')] == 'ERR' and p[('P2txn', 'mod.pc-bad-txn')].startswith('present')), '')
CONTRACT = [
    ('a circular placeholder reference raises the error the code names', _cycle_raises, False, ['C:D3']),
    ('a parameter value containing $ is substituted literally', _dollar_value_substituted, False, ['C:D4']),
    ('a failed after-install message does not report a committed load as failed', _late_error_truthful, False, ['P:F-LOAD-4', 'G:D13 family']),
]

def case_run(case):
    """E cases come from the corpus run; the P cases from the load run."""
    return RUNS[0] if case.startswith('E') else RUNS[1]
