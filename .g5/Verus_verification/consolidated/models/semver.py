"""Family R2-A: semantic versions, comparators, ranges and their intersection — the rules that decide which version
of a dependency satisfies a requirement. Executable twin of IPM 571e04a1 (`General/SemanticVersion*.cls`), compared
with native runs `20261114T030608Z-r2-semver` (corpus: parse V1-V17, ordering matrix F with SemVerPostRelease 0 and 1,
45 expressions X over 19 versions, 12 intersections A, post-release comparators C, bumps B) and
`20261114T031048Z-r2-resolve` (command level: the installed dependency version, S1-S3).

ObjectScript semantics reproduced: $piece, numeric `+` conversion and `>` on strings, string `=`, `]]` collation
(canonical numbers before strings, numbers numerically, strings by code point), $zstrip leading-character masks.
Premise for S1-S3: a filesystem repository offers exactly the fixture versions and the resolver installs the highest
version the requirement's range accepts (Repo/Manager.cls ordering, family temp-repo); the graph search of
Storage/Module.cls:BuildDependencyGraph is not modeled (S4 is recorded but not claimed).
"""
import re
ROUND = 'R2'
FAMILY = 'semantic-versions'
RUNS = ['20261114T030608Z-r2-semver', '20261114T031048Z-r2-resolve', '20261114T031514Z-r2-resolve-order', '20261114T034428Z-r5-heldback', '20261114T034751Z-r2-semver-addendum']
def case_run(case):
    if case[:2] in ('HB', 'HR', 'HM'): return RUNS[3]
    if case[:1] == 'Y' or case in ('B2', 'B3'): return RUNS[4]
    return RUNS[2] if case.startswith('S5') else RUNS[1] if case.startswith('S') else RUNS[0]
G = 'General/'
SV, CP, RG, EX = G + 'SemanticVersion.cls', G + 'SemanticVersionExpression/Comparator.cls', G + 'SemanticVersionExpression/Range.cls', G + 'SemanticVersionExpression.cls'
MEMBERS = {
    SV + '::FromString': {'slice': SV + ':33-54', 'fn': 'sv_from_string', 'behavior': 'parts from a version string; a hyphen inside build metadata becomes a prerelease (V9)'},
    SV + '::%OnValidateObject': {'slice': SV + ':75-96', 'fn': 'sv_validate', 'behavior': 'leading zeros rejected in numbers and numeric prerelease identifiers (V11, V12)'},
    SV + '::IsSnapshot': {'slice': SV + ':98-101', 'fn': 'is_snapshot', 'behavior': 'build "snapshot" in any case (V7, V8)'},
    SV + '::Follows': {'slice': SV + ':123-172', 'fn': 'follows', 'behavior': 'ordering incl. prerelease incomparability across versions, snapshot earlier, post-release mode (F)'},
    SV + '::CompareDotSeparatedStrings': {'slice': SV + ':175-205', 'fn': 'compare_dot', 'behavior': 'identifier precedence: numbers numerically and before words (F rows 5, 6, 13, 14)'},
    SV + '::ToString': {'slice': SV + ':16-23', 'fn': 'sv_to_string', 'behavior': 'canonical text incl. prerelease and build (V)'},
    CP + '::FromString': {'slice': CP + ':31-75', 'fn': 'cmp_from_string', 'behavior': 'one- and two-character operators; error names the remaining text (X41)'},
    CP + '::Evaluate': {'slice': CP + ':77-121', 'fn': 'cmp_evaluate', 'behavior': 'exact match incl. prerelease; build ignored for < >; post-release equality (X, C)'},
    CP + '::ToResolvedString': {'slice': CP + ':21-29', 'fn': 'cmp_resolved', 'behavior': 'operator always printed (=1.2.3)'},
    RG + '::FromString': {'slice': RG + ':31-205', 'fn': 'range_from_string', 'behavior': 'hyphen, x, tilde, caret ranges; an operator on a partial version is dropped (X26-X29); 20NN.N exact (X35)'},
    RG + '::InRange': {'slice': RG + ':207-217', 'fn': 'range_in', 'behavior': 'every comparator must hold (X30)'},
    RG + '::Intersect': {'slice': RG + ':228-326', 'fn': 'range_intersect', 'behavior': 'exact versions must agree; tightest bounds kept, first wins a prerelease-only tie (A1-A4)'},
    RG + '::GroupComparators': {'slice': RG + ':329-345', 'fn': 'group_comparators', 'behavior': '= kept apart from bounds (A7)'},
    RG + '::CompareVersions': {'slice': RG + ':349-389', 'fn': 'compare_versions', 'behavior': 'bounds compared ignoring prerelease (A3/A4 order dependence)'},
    RG + '::IsIntersectionValid': {'slice': RG + ':392-415', 'fn': 'intersection_valid', 'behavior': '>X with <=X empty; >=X with <=X allowed (A11, A12)'},
    RG + '::ToResolvedString': {'slice': RG + ':21-29', 'fn': 'range_resolved', 'behavior': 'comparators joined by a space'},
    EX + '::ExpressionSet': {'slice': EX + ':19-68', 'fn': 'expression_set', 'behavior': '&& intersects, || unions, mixing refused; && conflict message keeps literal %1 %2 (X31-X34, X42)'},
    EX + '::IsSatisfiedBy': {'slice': EX + ':106-117', 'fn': 'expr_satisfied', 'behavior': 'empty expression accepts every version (X1, X2)'},
    EX + '::And': {'slice': EX + ':129-166', 'fn': 'expr_and', 'behavior': 'pairwise intersection of options; conflict message (A5-A12)'},
    EX + '::ToResolvedString': {'slice': EX + ':75-91', 'fn': 'expr_resolved', 'behavior': 'options in parentheses joined by ||'},
}
V = {'v': 'as-built', 'post': 0}
def var(name): return V['v'] == name
VARIANTS = {
    'parse-build-hyphen-correct': {'member': SV + '::FromString', 'wrong': 'a hyphen inside build metadata stays in the build'},
    'parse-four-parts-patch': {'member': SV + '::FromString', 'wrong': 'a fourth dotted part is dropped instead of becoming build'},
    'validate-allow-leading-zero': {'member': SV + '::%OnValidateObject', 'wrong': 'leading zeros accepted'},
    'snapshot-case-sensitive': {'member': SV + '::IsSnapshot', 'wrong': 'only lower-case snapshot'},
    'follows-prerelease-total': {'member': SV + '::Follows', 'wrong': 'prereleases of different versions compare by version (total order)'},
    'follows-snapshot-equal': {'member': SV + '::Follows', 'wrong': 'a +snapshot build is equal to its release'},
    'follows-postrelease-ignored': {'member': SV + '::Follows', 'wrong': 'the SemVerPostRelease setting is ignored'},
    'dot-string-compare': {'member': SV + '::CompareDotSeparatedStrings', 'wrong': 'identifiers compared as plain strings'},
    'dot-longer-is-lower': {'member': SV + '::CompareDotSeparatedStrings', 'wrong': 'a longer identifier list ranks lower'},
    'tostring-no-build': {'member': SV + '::ToString', 'wrong': 'build metadata omitted'},
    'cmp-single-char-op': {'member': CP + '::FromString', 'wrong': 'only the first operator character is read'},
    'cmp-exact-ignores-prerelease': {'member': CP + '::Evaluate', 'wrong': '=1.0.0-beta matches any 1.0.0'},
    'cmp-build-kept': {'member': CP + '::Evaluate', 'wrong': 'build metadata kept when comparing, so <1.0.1 rejects 1.0.0+snapshot... ordering by snapshot'},
    'cmp-postrelease-not-equal': {'member': CP + '::Evaluate', 'wrong': 'with SemVerPostRelease, >=1.0.0 rejects 1.0.0-alpha'},
    'cmp-resolved-no-eq': {'member': CP + '::ToResolvedString', 'wrong': 'the = operator is not printed'},
    'range-partial-keeps-operator': {'member': RG + '::FromString', 'wrong': 'node-semver: >=1.2 means >=1.2.0 and <2 means <2.0.0-0 (intended reading)'},
    'range-caret-major': {'member': RG + '::FromString', 'wrong': 'node-semver caret: ^1.2 means >=1.2.0 <2.0.0-0 (intended reading)'},
    'range-year-as-x': {'member': RG + '::FromString', 'wrong': '2024.1 is an x-range like 2024.12'},
    'range-hyphen-max-inclusive-partial': {'member': RG + '::FromString', 'wrong': 'a partial upper bound in a hyphen range is inclusive'},
    'range-any-comparator': {'member': RG + '::InRange', 'wrong': 'one comparator suffices'},
    'intersect-exact-ignores-bounds': {'member': RG + '::Intersect', 'wrong': 'an exact version is returned without checking the bounds'},
    'intersect-last-wins-tie': {'member': RG + '::Intersect', 'wrong': 'on equal bounds the later one is kept'},
    'group-eq-as-lower': {'member': RG + '::GroupComparators', 'wrong': '= treated as a lower bound'},
    'compare-prerelease-aware': {'member': RG + '::CompareVersions', 'wrong': 'bounds compared with prerelease precedence (commutative intersection)'},
    'valid-strict-equal-ok': {'member': RG + '::IsIntersectionValid', 'wrong': '>X with <=X is accepted'},
    'range-resolved-comma': {'member': RG + '::ToResolvedString', 'wrong': 'comparators joined by a comma'},
    'expr-and-as-or': {'member': EX + '::ExpressionSet', 'wrong': '&& treated as ||'},
    'expr-conflict-message-filled': {'member': EX + '::ExpressionSet', 'wrong': 'the && conflict message names the two ranges'},
    'expr-empty-none': {'member': EX + '::IsSatisfiedBy', 'wrong': 'an empty expression accepts nothing'},
    'and-intersect-error-wrapped': {'member': EX + '::And', 'wrong': 'disagreeing exact versions report the AND conflict message'},
    'and-empty-left-keeps-empty': {'member': EX + '::And', 'wrong': 'an empty left side stays empty'},
    'expr-resolved-no-parens': {'member': EX + '::ToResolvedString', 'wrong': 'options not parenthesized'},
}
class IPMError(Exception): pass

# ---------------------------------------------------------------- ObjectScript helpers
def piece(s, d, i=1, j=None):
    parts = s.split(d)
    if j is None: j = i
    if j == '*': j = len(parts)
    return d.join(parts[i - 1:j]) if i - 1 < len(parts) else ''
def num(s):
    m = re.match(r'\s*([+-]*)(\d*)(\.\d*)?', s or '')
    if not m or (m.group(2) == '' and (m.group(3) or '') in ('', '.')): return 0
    v = float((m.group(2) or '0') + (m.group(3) or ''))
    if m.group(1).count('-') % 2: v = -v
    return int(v) if v == int(v) else v
def canon(s): return str(num(s)) == s
def sorts_after(a, b):
    ka = (0, num(a), '') if canon(a) else (1, 0, a)
    kb = (0, num(b), '') if canon(b) else (1, 0, b)
    return ka > kb
def zstrip_lead(s, chars): return s.lstrip(' \t' + chars)

# ---------------------------------------------------------------- SemanticVersion
def sv_from_string(s):
    v = {'Major': piece(piece(piece(s, '.', 1), '-'), '+'), 'Minor': '0', 'Patch': '0', 'Prerelease': '', 'Build': ''}
    nd = len(piece(s, '-').split('.'))
    if nd > 3:
        v['Patch'] = piece(s, '.', 3)
        if not var('parse-four-parts-patch'): v['Build'] = piece(s, '.', 4)
    if nd > 1: v['Minor'] = piece(piece(piece(s, '.', 2), '-'), '+')
    if nd > 2: v['Patch'] = piece(piece(piece(s, '.', 3), '-'), '+')
    if var('parse-build-hyphen-correct'):
        v['Prerelease'] = piece(piece(piece(s, '+', 1), '-', 2, '*'), '+')
    else:
        v['Prerelease'] = piece(piece(s, '-', 2, '*'), '+')
    if len(s.split('+')) > 1: v['Build'] = piece(s, '+', 2)
    return v
def sv_to_string(v):
    return v['Major'] + '.' + v['Minor'] + ('' if v['Patch'] == '' else '.' + v['Patch']) + \
        ('' if v['Prerelease'] == '' else '-' + v['Prerelease']) + ('' if v['Build'] == '' or var('tostring-no-build') else '+' + v['Build'])
def _intval(x): return x != '' and re.fullmatch(r'[+-]?\d+(\.0*)?|[+-]?\d*\.\d+', x) is not None and num(x) >= 0 and num(x) == int(num(x))
RX = re.compile(r'([0-9A-Za-z-])+(\.([0-9A-Za-z-])+)*')
def props_valid(v):
    return all(_intval(v[k]) for k in ('Major', 'Minor', 'Patch')) and all(v[k] == '' or RX.fullmatch(v[k]) for k in ('Prerelease', 'Build'))
def sv_validate(v):
    if not var('validate-allow-leading-zero'):
        for k in ('Major', 'Minor', 'Patch'):
            if v[k] != str(num(v[k])): return False
        if any(re.fullmatch(r'0[0-9]+', i) for i in v['Prerelease'].split('.') if v['Prerelease']): return False
    return props_valid(v)
def is_snapshot(v): return (v['Build'] == 'snapshot') if var('snapshot-case-sensitive') else v['Build'].lower() == 'snapshot'
def compare_dot(this, other):
    tp, op = this.split('.'), other.split('.')
    if var('dot-longer-is-lower') and len(tp) != len(op) and tp[:len(op)] == op[:len(tp)]: return len(tp) < len(op)
    for i, t in enumerate(tp):
        if i >= len(op): return True
        o = op[i]
        if o == t: continue
        return (t > o) if var('dot-string-compare') else sorts_after(t, o)
    return False
def follows(a, b):
    ma, mb = num(a['Major']), num(b['Major'])
    tf = ma > mb or (a['Major'] == b['Major'] and num(a['Minor']) > num(b['Minor'])) or \
        (a['Major'] == b['Major'] and a['Minor'] == b['Minor'] and num(a['Patch']) > num(b['Patch']))
    if V['post'] and not var('follows-postrelease-ignored'):
        if tf: return True
        if ma < mb or num(a['Minor']) < num(b['Minor']) or num(a['Patch']) < num(b['Patch']): return False
        if a['Prerelease'] == '': return False
        if b['Prerelease'] == '': return True
        return compare_dot(a['Prerelease'], b['Prerelease'])
    eq = a['Major'] == b['Major'] and a['Minor'] == b['Minor'] and a['Patch'] == b['Patch']
    snap = lambda: (is_snapshot(b) and not is_snapshot(a)) and not var('follows-snapshot-equal')
    if a['Prerelease'] != '' or b['Prerelease'] != '':
        if not eq:
            if a['Prerelease'] != '' and b['Prerelease'] != '':
                if var('follows-prerelease-total'): return tf
                return False
            return tf
        if a['Prerelease'] == b['Prerelease']: return snap()
        if a['Prerelease'] != '' and b['Prerelease'] == '': return False
        if a['Prerelease'] == '' and b['Prerelease'] != '': return True
        return compare_dot(a['Prerelease'], b['Prerelease'])
    if eq: return snap()
    return tf

# ---------------------------------------------------------------- Comparator
def cmp_from_string(expr):
    expr = expr.replace('(', '').replace(')', '')
    c = {'Expression': expr, 'Operator': '='}
    f, s = expr[0:1], expr[1:2]
    if f != str(num(f)) or f == '':
        if (s != str(num(s)) or s == '') and not var('cmp-single-char-op'):
            c['Operator'] = f + s; expr = expr[2:]
        else:
            c['Operator'] = f; expr = expr[1:]
            if var('cmp-single-char-op') and s in ('=',): expr = expr[1:]
    def fail(): raise IPMError('Error parsing semantic version expression comparator: ' + expr)
    if c['Operator'] not in ('=', '<', '<=', '>', '>='): fail()
    c['Major'] = piece(expr, '.', 1); c['Minor'] = piece(expr, '.', 2)
    end = piece(expr, '.', 3, '*')
    c['Patch'] = piece(piece(end, '-'), '+'); c['Prerelease'] = piece(piece(end, '-', 2, '*'), '+'); c['Build'] = piece(end, '+', 2)
    if not props_valid(c): fail()
    return c
def cmp_resolved(c):
    op = '' if (var('cmp-resolved-no-eq') and c['Operator'] == '=') else c['Operator']
    return op + c['Major'] + '.' + c['Minor'] + ('' if c['Patch'] == '' else '.' + c['Patch']) + \
        ('' if c['Prerelease'] == '' else '-' + c['Prerelease']) + ('' if c['Build'] == '' else '+' + c['Build'])
def cmp_extract(c): return {k: c[k] for k in ('Major', 'Minor', 'Patch', 'Prerelease')} | {'Build': ''}
def cmp_evaluate(c, v):
    eq = '=' in c['Operator']
    if eq:
        for part in ('Major', 'Minor', 'Patch', 'Prerelease', 'Build'):
            a, b = v[part], c.get(part, '')
            if a == '*' or b == '*': break
            if b != '': eq = eq and a == b
    if (not V['post'] or var('cmp-postrelease-not-equal')) or c['Operator'][:1] == '=':
        if not var('cmp-exact-ignores-prerelease'): eq = eq and v['Prerelease'] == c['Prerelease']
    if eq or c['Operator'] == '=': return eq
    tv = dict(v)
    if not var('cmp-build-kept'): tv['Build'] = ''
    cv = cmp_extract(c)
    return follows(cv, tv) if '<' in c['Operator'] else follows(tv, cv)

# ---------------------------------------------------------------- Range
def range_from_string(rexpr):
    parts, exprs = rexpr.split(' '), []
    it = iter(range(len(parts))); i = 0
    while i < len(parts):
        p = parts[i]
        if p == '-':
            if i + 1 >= len(parts): raise IPMError('Invalid range expression: ' + rexpr)
            mx = parts[i + 1]; i += 1
            mn = exprs[-1]; n = len(mn.split('.'))
            if n < 3: mn += '.0' * (3 - n)
            exprs[-1] = '>=' + mn
            n = len(mx.split('.')); op = '<='
            if n < 3 and not var('range-hyphen-max-inclusive-partial'):
                op = '<'; ps = mx.split('.'); ps[n - 1] = str(1 + num(ps[n - 1])); mx = '.'.join(ps)
            if n < 3: mx += '.0' * (3 - n)
            exprs.append(op + mx)
        else:
            exprs.append(p)
        i += 1
    comps = []
    for e in exprs:
        mmp = zstrip_lead(piece(piece(e, '-'), '+'), '<>=^~').replace('X', 'x').replace('*', 'x')
        if re.fullmatch(r'20\d\d\.\d', mmp) and not var('range-year-as-x'):
            e += '.0'; mmp += '.0'
        ma, mi, pa = piece(mmp, '.', 1), piece(mmp, '.', 2), piece(mmp, '.', 3)
        isx = 'x' in mmp or e == '*' or mi == '' or pa == ''
        tilde, caret = e[:1] == '~', e[:1] == '^'
        if var('range-partial-keeps-operator') and isx and e[:1] != '' and e[:1] in '<>=' and not ('x' in mmp or e == '*'):
            op = re.match(r'[<>=]+', e).group(); base = [ma or '0', mi or '0', pa or '0']
            if op in ('>=', '='): comps.append('>=' + '.'.join(base)) if op == '>=' else comps.extend(['>=' + '.'.join(base)])
            if op == '<': comps.append('<' + '.'.join(base) + '-0')
            if op == '<=': comps.append('<' + (f'{num(ma) + 1}.0.0-0' if mi == '' else f'{ma}.{num(mi) + 1}.0-0'))
            if op == '>': comps.append('>=' + (f'{num(ma) + 1}.0.0' if mi == '' else f'{ma}.{num(mi) + 1}.0'))
            continue
        if not (isx or tilde or caret):
            comps.append(e)
        elif tilde:
            e = e[1:]
            if len(e.split('.')) < 3:
                e += '.x'; isx = True
            else:
                comps += ['>=' + e, '<' + piece(e, '.', 1) + '.' + str(num(piece(e, '.', 2)) + 1) + '.0-0']
        elif caret:
            e = e[1:]
            ma, mi, pa = piece(mmp, '.', 1), piece(mmp, '.', 2), piece(mmp, '.', 3)
            if len(e.split('.')) < 3:
                if var('range-caret-major') and ma != '0' and ma != 'x':
                    comps += ['>=' + ma + '.' + (mi if mi not in ('', 'x') else '0') + '.0', '<' + str(num(ma) + 1) + '.0.0-0']; isx = False
                elif ma != '0' and not isx:
                    comps += ['>=' + e.replace('.x', '.0'), '<' + str(num(ma) + 1) + '.0.0-0']; isx = False
            else:
                comps.append('>=' + e.replace('.x', '.0'))
                if num(ma) == 0 and mi != 'x':
                    mx = f'0.0.{num(pa) + 1}-0' if (num(mi) == 0 and pa != 'x') else f'0.{num(mi) + 1}.0-0'
                else:
                    mx = f'{num(ma) + 1}.0.0-0'
                comps.append('<' + mx); isx = False
        if isx:
            mmp = zstrip_lead(piece(piece(e, '-'), '+'), '<>=').replace('X', 'x').replace('*', 'x')
            ma, mi, pa = piece(mmp, '.', 1), piece(mmp, '.', 2), piece(mmp, '.', 3)
            if e in ('', '*') or ma == '':
                comps.append('>=0.0.0')
            elif mi in ('', 'x'):
                comps += ['>=' + ma + '.0.0', '<' + str(num(ma) + 1) + '.0.0-0']
            elif pa in ('', 'x'):
                comps += ['>=' + ma + '.' + mi + '.0', '<' + ma + '.' + str(num(mi) + 1) + '.0-0']
    return [cmp_from_string(c) for c in comps if c != '>=0.0.0']
def range_resolved(r): return (',' if var('range-resolved-comma') else ' ').join(cmp_resolved(c) for c in r)
def range_in(r, v):
    if var('range-any-comparator'): return any(cmp_evaluate(c, v) for c in r) if r else True
    return all(cmp_evaluate(c, v) for c in r)
def group_comparators(r, lo, up, ex):
    for c in r:
        if c['Operator'] in ('>=', '>') or (var('group-eq-as-lower') and c['Operator'] == '='): lo.append(c)
        elif c['Operator'] in ('<', '<='): up.append(c)
        elif c['Operator'] == '=': ex.append(c)
def compare_versions(c1, c2):
    if var('compare-prerelease-aware'):
        a, b = cmp_extract(c1), cmp_extract(c2)
        if follows(a, b): return 1
        if follows(b, a): return -1
    else:
        for k in ('Major', 'Minor', 'Patch'):
            if num(c1[k]) > num(c2[k]): return 1
            if num(c1[k]) < num(c2[k]): return -1
    o1, o2 = c1['Operator'], c2['Operator']
    return {('>', '>='): 1, ('>=', '>'): -1, ('<', '<='): -1, ('<=', '<'): 1}.get((o1, o2), 0)
def intersection_valid(lo, up):
    vl, vu = cmp_extract(lo), cmp_extract(up)
    if follows(vu, vl): return True
    if follows(vl, vu): return False
    if var('valid-strict-equal-ok'): return not (lo['Operator'] == '>' and up['Operator'] == '<')
    return not (lo['Operator'] == '>' or up['Operator'] == '<')
def range_intersect(r1, r2):
    lo, up, ex = [], [], []
    group_comparators(r1, lo, up, ex); group_comparators(r2, lo, up, ex)
    if ex:
        f = ex[0]
        for c in ex[1:]:
            if any(f[k] != c[k] for k in ('Major', 'Minor', 'Patch', 'Prerelease')):
                raise IPMError(f'Exact version matches do not agree: {cmp_resolved(f)} and {cmp_resolved(c)}')
        v = cmp_extract(f)
        if not var('intersect-exact-ignores-bounds'):
            if not all(cmp_evaluate(c, v) for c in lo + up): return None
        return range_from_string(f['Expression'])
    def pick(cs, better):
        best = None
        for c in cs:
            if best is None: best = c
            elif better(compare_versions(best, c)) or (var('intersect-last-wins-tie') and compare_versions(best, c) == 0): best = c
        return best
    mlo, mup = pick(lo, lambda r: r < 0), pick(up, lambda r: r > 0)
    if mlo and mup and not intersection_valid(mlo, mup): return None
    merged = ' '.join(cmp_resolved(c) for c in (mlo, mup) if c)
    if merged == '': raise IPMError('Cannot merge ranges')
    return range_from_string(merged)

# ---------------------------------------------------------------- Expression
def expression_set(value):
    if '&&' in value and '||' in value: raise IPMError("Cannot mix '&&' and '||' in a single expression")
    anyof = []
    if '&&' in value and not var('expr-and-as-or'):
        acc = None
        for req in value.split('&&'):
            req = req.strip(' \t').replace('(', '').replace(')', '')
            opt = range_from_string(req)
            if range_resolved(opt) == '': continue
            if acc is None: acc = opt
            else:
                m = range_intersect(acc, opt)
                if m is None:
                    if var('expr-conflict-message-filled'):
                        raise IPMError(f'No overlapping versions in && operation for {range_resolved(acc)} and {range_resolved(opt)}')
                    raise IPMError('No overlapping versions in && operation for %1 and %2')
                acc = m
        if acc is not None and range_resolved(acc) != '': anyof.append(acc)
    else:
        for req in value.split('&&' if '&&' in value else '||'):
            req = req.strip(' \t').replace('(', '').replace(')', '')
            opt = range_from_string(req)
            if range_resolved(opt) == '': continue
            anyof.append(opt)
    return {'AnyOf': anyof, 'Expression': value}
def expr_resolved(e):
    rs = [range_resolved(o) for o in e['AnyOf']]
    if len(rs) > 1 and not var('expr-resolved-no-parens'): rs = ['(' + r + ')' for r in rs]
    return ' || '.join(rs)
def expr_satisfied(e, v):
    if not e['AnyOf']: return not var('expr-empty-none')
    return any(range_in(o, v) for o in e['AnyOf'])
def expr_and(e, o):
    if o['Expression'] != '':
        if not e['AnyOf']:
            if var('and-empty-left-keeps-empty'): return e
            return {'AnyOf': o['AnyOf'], 'Expression': o['Expression']}
        if not o['AnyOf']: return e
        cur = expr_resolved(e); out = []
        for a in e['AnyOf']:
            for b in o['AnyOf']:
                try:
                    m = range_intersect(a, b)
                except IPMError:
                    if var('and-intersect-error-wrapped'): m = None
                    else: raise
                if m is not None: out.append(m)
        r = {'AnyOf': out, 'Expression': ''}
        res = expr_resolved(r)
        if res == '': raise IPMError(f'No overlapping versions in AND operation for {cur} and {expr_resolved(o)}')
        r['Expression'] = res
        return r
    return e

# ---------------------------------------------------------------- corpus and predictions
PL = ["1", "1.2", "1.2.3", "1.2.3.4", "1.2.3-beta", "1.2.3-beta.1+b.2", "1.2.3+snapshot", "1.2.3+SNAPSHOT", "1.2.3+build-x", "1.2.3-beta-2", "01.2.3", "1.2.3-01", "1.2.3-0a", "v1.2.3", "1.2.3-", "", "1..3"]
FL = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0", "1.0.0+snapshot", "1.0.1-alpha", "1.0.1", "2.0.0", "1.0.0-Beta", "1.0.0-10"]
VL = ["0.0.1", "0.1.0", "0.1.5", "1.0.0-alpha", "1.0.0-beta", "1.0.0", "1.0.0+snapshot", "1.1.0-beta", "1.2.0", "1.2.3", "1.3.0", "2.0.0-beta.1", "2.0.0", "2.5.0", "3.0.0", "2024.1.0", "2024.1.2", "2024.12.0", "1.2.3+build-x"]
XL = ["", "*", "1.2.3", "=1.2.3", ">1.2.3", ">=1.2.3", "<1.2.3", "<=1.2.3", "1", "1.2", "1.x", "1.2.x", "1.X", "~1.2.3", "~1.2", "~1", "^1.2.3", "^0.1.5", "^0.0.1", "^1.2", "^0.1", "^1.x", "1.0.0 - 2.0.0", "1 - 2", "1.2 - 2.5", ">=1.2", "<2", ">1.2", "<=1", ">=1.0.0 <2.0.0", ">=1.0.0 <2.0.0 || >=3.0.0", ">=1.0.0 && <2.0.0", ">=2.0.0 && <1.0.0", "1.0.0 && 2.0.0", "2024.1", "2024.12", ">=1.0.0-beta", "1.0.0-beta", "1.0.0 || 2.0.0", "(>=1.0.0) && (<1.3.0)", "a.b.c", ">=1.0.0 && <2.0.0 || 3.0.0", "01.2.3", "1.2.3-01", "1.2.3+build-x"]
AL = [("^1.0.0", "<2.0.0"), ("<2.0.0", "^1.0.0"), (">=1.0.0-beta", ">=1.0.0"), (">=1.0.0", ">=1.0.0-beta"), ("1.0.0", "2.0.0"), (">=2.0.0", "<1.0.0"), ("1.2.3", ">=1.0.0"), ("", ">=1.0.0"), (">=1.0.0", ""), ("1.x || 3.x", ">=1.5.0"), (">1.0.0", "<=1.0.0"), (">=1.0.0", "<=1.0.0")]
CL = [">=1.0.0", "<=1.0.0", "=1.0.0", ">1.0.0", "<1.0.1", "^1.0.0"]
def bits(e): return ''.join('1' if expr_satisfied(e, sv_from_string(v)) else '0' for v in VL)
def predict(variant):
    V['v'], V['post'] = variant, 0
    out = {}
    for i, s in enumerate(PL, 1):
        v = sv_from_string(s)
        out[(f'V{i}', 'parse')] = '|'.join(v[k] for k in ('Major', 'Minor', 'Patch', 'Prerelease', 'Build')) + \
            f" str={sv_to_string(v)} snap={int(is_snapshot(v))} valid={int(sv_validate(v))}"
    for pr in (0, 1):
        V['post'] = pr
        for i, a in enumerate(FL, 1):
            out[(f'F{pr}.{i}', 'row')] = ''.join('1' if follows(sv_from_string(a), sv_from_string(b)) else '0' for b in FL)
    V['post'] = 0
    for i, s in enumerate(XL, 1):
        try:
            e = expression_set(s); out[(f'X{i}', 'res')] = f'{expr_resolved(e)} sat={bits(e)}'
        except IPMError as ex: out[(f'X{i}', 'err')] = 'ERROR #5001: ' + str(ex)
    for i, (a, b) in enumerate(AL, 1):
        try:
            r = expr_and(expression_set(a), expression_set(b))
            out[(f'A{i}', 'res')] = f"{expr_resolved(r)} expr={r['Expression']} sat={bits(r)}"
        except IPMError as ex: out[(f'A{i}', 'err')] = 'ERROR #5001: ' + str(ex)
    V['post'] = 1
    for i, s in enumerate(CL, 1): out[(f'C{i}', 'sat')] = bits(expression_set(s))
    V['post'] = 0
    # command level: the highest offered version that the requirement accepts is installed
    reg = {'sv-lt-dep': ['1.5.0', '2.5.0'], 'sv-ge-dep': ['1.3.0'], 'sv-ge2-dep': ['1.5.0', '1.2.0']}
    for case, dep, req, main in (('S1', 'sv-lt-dep', '<2', 'sv-lt-main'), ('S2', 'sv-ge-dep', '>=1.2', 'sv-ge-main'), ('S3', 'sv-ge2-dep', '>=1.2', 'sv-ge2-main')):
        e = expression_set(req)
        ok = sorted([v for v in reg[dep] if expr_satisfied(e, sv_from_string(v))], key=lambda v: [num(x) for x in v.split('.')], reverse=True)
        out[(case, 'tlevel.before')] = '0'; out[(case, 'tlevel.after')] = '0'
        if ok:
            out[(case, 'status')] = 'OK'; out[(case, 'mod.' + dep)] = f'present version={ok[0]} repo=reg2 desc={ok[0]} dev=0'
        else:
            out[(case, 'status')] = 'ERR'; out[(case, 'mod.' + dep)] = 'absent'
            out[(case, 'error')] = f'ERROR #5001: Could not find satisfactory version of {dep} in any repositories. Required by: {main} 1.0.0: {req}'
            out[(case, 'mod.' + main)] = 'absent'
    # Diamond (S4ab, S4ba, S5): the resolver visits parents in name order (pWorkQueue keyed by name) and combines the
    # current parent's requirement with the ones already recorded (`searchExpr.And(...)`, Storage/Module.cls:1095-1106);
    # the highest accepted version is chosen; LoadModuleReference then checks the And of every dependent's requirement
    # (Utils/Module.cls:116-124). Abstraction of members already in the baseline (P: ProcessSingleDependencyIterative).
    offered = ['2.0.0-beta.1', '1.5.0']
    for case, shared, parents in (('S4ab', 'shared-ab', [('dep-a-ab', '^1.0.0'), ('dep-b-ab', '<2.0.0')]),
                                  ('S4ba', 'shared-ba', [('dep-a-ba', '^1.0.0'), ('dep-b-ba', '<2.0.0')]),
                                  ('S5', 'shared-zz', [('dep-0-zz', '<2.0.0'), ('dep-1-zz', '^1.0.0')])):
        rec, chosen = [], None
        for pname, req in sorted(parents):
            se = expression_set(req)
            for _, prev in sorted(rec): se = expr_and(se, expression_set(prev))
            cands = [v for v in offered if expr_satisfied(se, sv_from_string(v))]
            chosen = cands[0] if cands else None; rec.append((pname, req))
        allreq = expression_set(sorted(parents)[0][1])
        for _, r in sorted(parents)[1:]: allreq = expr_and(allreq, expression_set(r))
        out[(case, 'tlevel.before')] = '0'; out[(case, 'tlevel.after')] = '0'
        if expr_satisfied(allreq, sv_from_string(chosen)):
            out[(case, 'status')] = 'OK'; out[(case, 'mod.' + shared)] = f'present version={chosen} repo=reg2b desc={chosen} dev=0'
            out[(case, 'mod.root-zz')] = 'present version=1.0.0 repo= desc= dev=0'
        else:
            out[(case, 'status')] = 'ERR'; out[(case, 'mod.' + shared)] = 'absent'
            req_txt = '; '.join(f'{n}: {r}' for n, r in sorted(parents))
            out[(case, 'error')] = f'ERROR #5001: Requested version ({shared} {chosen}) does not satisfy the requirements of other modules installed in the current namespace ({req_txt}).'
    _heldback(out)
    # Addendum (round 5, closes two survivors): a +snapshot build against strict bounds; an exact version outside a range.
    for i, e in enumerate(['<1.0.0', '>1.0.0', '<=1.0.0'], 1):
        x = expression_set(e); out[(f'Y{i}', 'res')] = f'{expr_resolved(x)} sat={bits(x)}'
    for i, (a, b) in enumerate([('1.0.0', '>=2.0.0'), ('>=2.0.0', '1.0.0')], 2):
        try:
            r = expr_and(expression_set(a), expression_set(b)); out[(f'B{i}', 'res')] = expr_resolved(r)
        except IPMError as ex: out[(f'B{i}', 'err')] = 'ERROR #5001: ' + str(ex)
    return out

def _heldback(out):
    # Round 5 held-back case: LoadNewModule refuses a downgrade when the installed version Follows the new one
    # (Utils/Module.cls:1212-1217, a baseline member); ordering is this family's Follows.
    for pre, name, first, second in (('HB', 'hb-pre', '2.0.0-beta', '1.0.0-rc'), ('HR', 'hb-rel', '2.0.0', '1.0.0'), ('HM', 'hb-mix', '2.0.0-beta', '1.0.0')):
        out[(pre + '1', 'status')] = 'OK'; out[(pre + '1', 'tlevel.before')] = '0'; out[(pre + '1', 'tlevel.after')] = '0'
        out[(pre + '1', 'mod.' + name)] = f'present version={first} repo= desc= dev=0'
        refused = follows(sv_from_string(first), sv_from_string(second))
        out[(pre + '2', 'status')] = 'ERR' if refused else 'OK'
        out[(pre + '2', 'tlevel.before')] = '0'; out[(pre + '2', 'tlevel.after')] = '0'
        if refused:
            out[(pre + '2', 'error')] = (f'ERROR #5001: Cannot downgrade {name} from version {first} to {second}. To downgrade a module, you must first run '
                                         'the uninstall command. Then, run the install or load command with the desired module version.')
        out[(pre + '2', 'mod.' + name)] = f'present version={first if refused else second} repo= desc= dev=0'
def _downgrade_always_refused(p):
    return (p[('HB2', 'status')] == 'ERR', p[('HB2', 'mod.hb-pre')])
def _name_independent(p):
    return (p[('S4ab', 'status')] == p[('S5', 'status')], f"S4ab={p[('S4ab','status')]} S5={p[('S5','status')]}")
def _partial_operator(p):
    r = p.get(('X27', 'res'), '')
    return (r.startswith('<2.0.0-0'), r)
def _caret(p):
    r = p.get(('X20', 'res'), '')
    return (r.startswith('>=1.2.0 <2.0.0-0'), r)
def _and_commutes(p):
    pairs = [(1, 2), (3, 4)]
    bad = [(a, b) for a, b in pairs if p.get((f'A{a}', 'res'), '').split(' sat=')[1:] != p.get((f'A{b}', 'res'), '').split(' sat=')[1:]]
    return (not bad, bad)
def _build_hyphen(p):
    return ('|build-x' in p[('V9', 'parse')] and p[('V9', 'parse')].startswith('1|2|3||'), p[('V9', 'parse')])
def _lt2_installs_below_2(p):
    return ('version=1.5.0' in p[('S1', 'mod.sv-lt-dep')], p[('S1', 'mod.sv-lt-dep')])
CONTRACT = [
    ('<2 accepts versions below 2.0.0 (node-semver, which the range code cites)', _partial_operator, False, ['C:D6']),
    ('^1.2 accepts 1.x from 1.2.0 (node-semver caret text quoted in the code)', _caret, False, ['C:D7']),
    ('combining two requirements does not depend on their order', _and_commutes, False, ['C:D8']),
    ('build metadata containing a hyphen stays build metadata (SemVer 2.0.0 item 10)', _build_hyphen, False, ['C:D9']),
    ('a dependency declared <2 is installed below 2.0.0', _lt2_installs_below_2, False, ['C:D6']),
    ('whether a satisfiable diamond installs does not depend on dependency names', _name_independent, False, ['C:D8']),
    ('loading a lower version is refused (load.refusal.downgrade)', _downgrade_always_refused, False, ['C:D15']),
]
