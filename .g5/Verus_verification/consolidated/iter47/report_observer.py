"""Host-side observer for iteration47: decodes the base64 dumps in the session log and parses each
report with the format's ordinary parser (json, PyYAML safe_load, RFC 4180 csv). Returns OBS ints."""
import base64, csv, io, json, re
import yaml

EXPECT = {'TestBPunct': 'has: colon, comma, "quote" and #hash', 'TestCLineFeed': 'first\nsecond',
          'TestDCrLf': 'one\r\ntwo', 'TestELeading': '  leading spaces'}
METHODS = ['TestAPass', 'TestBPunct', 'TestCLineFeed', 'TestDCrLf', 'TestELeading', 'TestFThrow']

def blocks(raw):
    raw = raw.replace('\r\n', '\n')
    out = {}
    for m in re.finditer(r'B64BEGIN\|(\S+)\n(.*?)B64END\|\1', raw, re.S):
        out[m.group(1)] = base64.b64decode(''.join(m.group(2).split())).decode('utf-8')
    return out

def first_desc(method_asserts):
    for a in method_asserts:
        if a.get('action') not in ('LogMessage',) and a.get('status') == 'failed':
            return a.get('description')
    return None

def facts_from_tree(prefix, methods):
    """methods: name -> {'status':..., 'error':..., 'asserts':[...]} """
    o = {}
    o[prefix + '.methods'] = len(methods)
    o[prefix + '.failed'] = sum(1 for m in methods.values() if m.get('status') == 'failed')
    for name, exp in EXPECT.items():
        d = first_desc(methods.get(name, {}).get('asserts', []))
        o[f'{prefix}.exact.{name}'] = int(d == exp)
        o[f'{prefix}.stripped.{name}'] = int(d is not None and d.rstrip('\n') == exp)
    err = methods.get('TestFThrow', {}).get('error') or ''
    o[prefix + '.throwerror'] = int('<DIVIDE>' in err)
    return o

def from_json(text):
    tree = json.loads(text)
    ms = {}
    for s in tree['suites']:
        for c in s['cases']:
            for m in c['methods']:
                ms[m['name']] = m
    return ms

def from_yaml(text):
    doc = yaml.safe_load(text)
    ms = {}
    for s in doc['unitTest']['results']:
        for c in s['testcases']:
            for m in c.get('methods') or []:
                ms[m['methodName']] = {'status': m['status'], 'error': m.get('error') or '', 'asserts': m.get('asserts') or []}
    return ms

def from_toon(text):
    lines = text.split('\n')
    i = next(k for k, l in enumerate(lines) if l.startswith('results['))
    declared = int(re.match(r'results\[(\d+)\]', lines[i]).group(1))
    body = '\n'.join(l[2:] if l.startswith('  ') else l for l in lines[i + 1:])
    rows = [r for r in csv.reader(io.StringIO(body, newline='')) if r]
    ms = {}
    rowsper = {}
    for r in rows:
        suite, case, meth, status, action, counter, desc, loc = r
        rowsper[meth] = rowsper.get(meth, 0) + 1
        m = ms.setdefault(meth, {'status': 'passed', 'error': '', 'asserts': []})
        if status == 'failed':
            m['status'] = 'failed'
        if action == 'error':
            m['error'] = desc
        elif action:
            m['asserts'].append({'action': action, 'status': status, 'description': desc})
    return ms, declared, len(rows), rowsper

def report_block(text, start):
    # The device capture continues with IPM's own phase lines ("[module]\tTest FAILURE", "ERROR!"),
    # which are command output, not part of the report.
    body = text[text.index(start):]
    out = []
    for line in body.split('\n'):
        if line.startswith('[') or '\x1b[' in line:
            break
        out.append(line)
    return '\n'.join(out)

def observe(raw):
    b = blocks(raw)
    o = {}
    for fmt, parser in [('json', from_json), ('yaml', from_yaml)]:
        text = b.get(fmt + '.filetext', '')
        try:
            ms = parser(text)
            o[f'{fmt}f.parse'] = 1
            o.update(facts_from_tree(fmt + 'f', ms))
        except Exception:
            o[f'{fmt}f.parse'] = 0
        o[f'{fmt}f.crbytes'] = text.count('\r')
    text = b.get('toon.filetext', '')
    try:
        ms, declared, actual, rowsper = from_toon(text)
        o['toonf.parse'] = 1
        o['toonf.declaredrows'] = declared
        o['toonf.actualrows'] = actual
        o['toonf.rows.TestFThrow'] = rowsper.get('TestFThrow', 0)
        o.update(facts_from_tree('toonf', ms))
    except Exception:
        o['toonf.parse'] = 0
    o['toonf.crbytes'] = text.count('\r')
    # Device summaries: counts as printed.
    dj = b.get('json.device', '')
    try:
        js, _ = json.JSONDecoder().raw_decode(dj[dj.index('{'):])
        o['jsond.methods'] = js['summary']['methods']['total']; o['jsond.failed'] = js['summary']['methods']['failed']
    except Exception:
        o['jsond.methods'] = -1; o['jsond.failed'] = -1
    dy = b.get('yaml.device', '')
    try:
        ydoc = yaml.safe_load(report_block(dy, 'summary:'))
        o['yamld.parse'] = 1
        o['yamld.methods'] = ydoc['summary']['methods']['total']; o['yamld.failed'] = ydoc['summary']['methods']['failed']
        dms = {}
        for s in ydoc['results']:
            for c in s['testcases']:
                for m in c.get('methods') or []:
                    dms[m['methodName']] = {'status': m['status'], 'error': m.get('error') or '', 'asserts': m.get('asserts') or []}
        o.update(facts_from_tree('yamld', dms))
    except Exception:
        o['yamld.parse'] = 0
    dt = b.get('toon.device', '')
    m = re.search(r'methods\[(\d+)\]: (\d+) passed, (\d+) failed', dt)
    o['toond.methods'] = int(m.group(1)) if m else -1
    o['toond.failed'] = int(m.group(3)) if m else -1
    dq = b.get('yamlq.device', '')
    try:
        q = yaml.safe_load(report_block(dq, 'summary:'))
        o['yamlqd.parse'] = 1; o['yamlqd.failures'] = sum(len(c.get('methods') or []) for s in q['failures'] for c in s['testcases'])
    except Exception:
        o['yamlqd.parse'] = 0
    o['xmlf.present'] = int('<testsuite' in b.get('xml.filetext', ''))
    return o
