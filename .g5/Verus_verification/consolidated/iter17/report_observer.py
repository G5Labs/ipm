"""Read actual CLI/file JSON and independently emitted stored rows; no predicted values."""
import json, re

def observe(raw):
    raw = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', raw).replace('\r\n', '\n')
    obs, files, trees, devices = {}, {}, {}, {}
    for line in raw.splitlines():
        if line.startswith('FILE|'):
            _, label, value = line.split('|', 2)
            assert label not in files, label
            files[label] = json.loads(value)
        elif line.startswith('TREE|'):
            _, label, id_, value = line.split('|', 3)
            key = (label, int(id_)); assert key not in trees, key
            trees[key] = json.loads(value)
    for label in ['pass', 'fail', 'nested']:
        begin, end = 'BEGIN|'+label+'\n', 'END|'+label+'\n'
        assert raw.count(begin) == raw.count(end) == 1
        segment = raw.split(begin, 1)[1].split(end, 1)[0]
        devices[label] = [json.loads(line) for line in segment.splitlines()
                          if line.startswith('{"summary":')]
        assert devices[label]
        before = int(re.search(r'^BEFORE\|'+label+r'\|(\d+)$', raw, re.M)[1])
        latest = int(re.search(r'^LATEST\|'+label+r'\|(\d+)$', raw, re.M)[1])
        file, device = files[label], devices[label][-1]
        obs[label+'-new-runs'] = sum(k[0] == label for k in trees)
        obs[label+'-file-offset'] = int(file['id'])-before
        obs[label+'-device-offset'] = int(device['summary']['id'])-before
        obs[label+'-file-is-latest'] = int(int(file['id']) == latest)
        obs[label+'-file-device-same'] = int(int(file['id']) == int(device['summary']['id']))
        obs[label+'-device-count'] = len(devices[label])
        obs.update(flatten(label+'-file', file))
        summary = device['summary']
        for kind in ['methods', 'assertions']:
            for field in ['total', 'passed', 'failed']:
                obs[label+'-device-'+kind+'-'+field] = summary[kind][field]
        obs[label+'-device-failure-rows'] = len(device['failures'])
        obs[label+'-file-matches-stored-tree'] = int(file == trees[(label, int(file['id']))])
        wanted = 'TestNestedFail' if label == 'nested' else 'Test'+label.title()
        actual_methods = [m for (l,_),t in trees.items() if l==label
                          for s in t['suites'] for c in s['cases'] if c['name']=='Iter17.Outer.Checks'
                          for m in c['methods'] if m['name']==wanted]
        assert len(actual_methods)==1
        outer = actual_methods[0]
        obs[label+'-stored-outer-method-failed'] = int(outer['status']=='failed')
        obs[label+'-stored-outer-error-empty'] = int(outer['error']=='')
        rows=[]; methods=[]
        for line in raw.splitlines():
            if line.startswith('ROW|'+label+'|'):
                row=line.split('|'); assert len(row)==10, row
                if row[3]=='Iter17.Outer.Checks' and row[4]==wanted: rows.append(row)
            elif line.startswith('METHOD|'+label+'|'):
                row=line.split('|'); assert len(row)==8, row
                if row[3]=='Iter17.Outer.Checks' and row[4]==wanted: methods.append(row)
        assert len(methods)==1
        obs[label+'-raw-method-pass'] = int(methods[0][5])
        obs[label+'-raw-error-category-empty'] = int(methods[0][6]=='')
        obs[label+'-raw-row-count'] = len(rows)
        obs[label+'-raw-failed-count'] = sum(row[6]=='0' for row in rows)
        obs[label+'-raw-duration-rows'] = sum(row[7]=='LogMessage' for row in rows)
        obs[label+'-raw-duration-passed'] = int(all(row[6]!='0' for row in rows if row[7]=='LogMessage'))
    for label in ['child', 'explicit-outer']:
        obs.update(flatten(label+'-file', files[label]))
    outer_id = int(devices['nested'][-1]['summary']['id'])
    inner_id = int(devices['nested'][0]['summary']['id'])
    obs['nested-child-later-than-outer'] = int(inner_id > outer_id)
    obs['nested-file-is-child'] = int(int(files['nested']['id']) == inner_id)
    obs['nested-file-equals-child-file'] = int(files['nested'] == files['child'])
    obs['explicit-file-is-outer'] = int(int(files['explicit-outer']['id']) == outer_id)
    obs['explicit-file-matches-outer-tree'] = int(files['explicit-outer'] == trees[('nested', outer_id)])
    obs['nested-device-failures-have-outer-labels'] = int(
        len(devices['nested'][-1]['failures'])==2 and
        {f['description'] for f in devices['nested'][-1]['failures']} ==
        {'outer deliberate false','outer deliberate mismatch'} and
        all(f['case']=='Iter17.Outer.Checks' and f['method']=='TestNestedFail'
            for f in devices['nested'][-1]['failures']))
    return obs

def flatten(prefix, tree):
    methods=[m for s in tree['suites'] for c in s['cases'] for m in c['methods']]
    asserts=[a for m in methods for a in m['asserts']]
    return {prefix+'-methods':len(methods),
            prefix+'-failed-methods':sum(m['status']=='failed' for m in methods),
            prefix+'-assertions':len(asserts),
            prefix+'-failed-assertions':sum(a['status']=='failed' for a in asserts),
            prefix+'-is-child-case':int(any(c['name']=='Iter17.Child.Checks' for s in tree['suites'] for c in s['cases'])),
            prefix+'-is-outer-case':int(any(c['name']=='Iter17.Outer.Checks' for s in tree['suites'] for c in s['cases']))}
