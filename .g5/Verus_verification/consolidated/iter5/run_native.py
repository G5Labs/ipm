#!/usr/bin/env python3
"""Iteration 5 native cases in a fresh disposable IRIS container from the pinned image.

Web application lifecycle, the registry redirect challenge and the shell entry forms. Creates
ipm-iter5-native-<stamp> (or $IPM_ITER5_CONTAINER), checks on-disk product sources against the pin,
runs the probes and records raw logs, provider requests, exit codes and container counters under
.g5/uplift/consolidated/iter5/native/<stamp>/. The container is removed afterwards unless --keep.
"""
from pathlib import Path
import datetime, hashlib, json, os, re, subprocess, sys, time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
IMAGE = 'ipm-consolidated-iris:571e04a1-fresh'
PIN = '571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
STAMP = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
OUT = ROOT / '.g5/uplift/consolidated/iter5/native' / STAMP
C = os.environ.get('IPM_ITER5_CONTAINER', 'ipm-iter5-native-' + STAMP.lower())
COMMANDS = []
PINS = HERE.parent / 'iter4/native/pin-sources.json'


def run(args, check=True, **kw):
    start = time.monotonic()
    p = subprocess.run(args, capture_output=True, text=True, timeout=600, **kw)
    COMMANDS.append({'command': args, 'exit': p.returncode, 'seconds': round(time.monotonic() - start, 3),
                     **({'stdin': kw['input']} if 'input' in kw else {})})
    if check and p.returncode:
        raise RuntimeError((args, p.stdout[-2000:], p.stderr[-2000:]))
    return p


def session(name, lines, check=True):
    p = run(['docker', 'exec', '-i', C, 'iris', 'session', 'IRIS', '-U', 'USER'], check=check, input='\n'.join(lines + ['halt']) + '\n')
    raw = p.stdout + p.stderr
    (OUT / ('session-' + name + '.log')).write_text(raw)
    return p, raw


def parse(raw, into):
    for line in raw.splitlines():
        if line.startswith('OBS|'):
            parts = line.split('|')
            into.setdefault(parts[1], {})['|'.join(parts[2:-1])] = parts[-1].strip()


def main():
    OUT.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    run(['docker', 'run', '-d', '--name', C, IMAGE])
    try:
        for _ in range(120):
            p = subprocess.run(['docker', 'exec', '-i', C, 'iris', 'session', 'IRIS', '-U', 'USER'], input='write "I5-READY",!\nhalt\n', capture_output=True, text=True)
            if p.returncode == 0 and 'I5-READY' in p.stdout:
                break
            time.sleep(1)
        else:
            raise RuntimeError('fresh container did not become ready')
        expected = json.loads(PINS.read_text())['source_sha256']
        script = ('import hashlib,json; from pathlib import Path; root=Path("/home/irisowner/zpm"); wanted=' + repr(list(expected)) +
                  '; print(json.dumps({p:hashlib.sha256((root/p).read_bytes()).hexdigest() if (root/p).is_file() else None for p in wanted}))')
        actual = json.loads(run(['docker', 'exec', C, 'python3', '-c', script]).stdout)
        mismatches = [p for p, v in expected.items() if actual[p] != v]
        (OUT / 'source-pin.json').write_text(json.dumps({'product_pin': PIN, 'paths': len(expected), 'mismatches': mismatches}, indent=2) + '\n')
        assert not mismatches, mismatches
        run(['docker', 'exec', C, 'mkdir', '-p', '/tmp/g5-iter5'])
        for item in sorted((HERE / 'native/fixtures').iterdir()):
            run(['docker', 'cp', str(item), C + ':/tmp/g5-iter5/' + item.name])
        for name in ['Probe.cls', 'Fail.cls', 'registry.py']:
            run(['docker', 'cp', str(HERE / 'native' / name), C + ':/tmp/g5-iter5/' + name])
        run(['docker', 'exec', '-d', C, 'python3', '/tmp/g5-iter5/registry.py'])
        for _ in range(30):
            if subprocess.run(['docker', 'exec', C, 'test', '-f', '/tmp/g5-iter5/registry-sha1.json']).returncode == 0:
                break
            time.sleep(0.5)
        obs = {}
        load = ['write !,"OBS|L-%s|ok|",$system.OBJ.Load("/tmp/g5-iter5/%s.cls","ck-d"),!' % (n, n) for n in ['Probe', 'Fail']]
        parse(session('1-webapps', load + ['do ##class(Iter5.Probe).WebApps()'])[1], obs)
        parse(session('2-redirect', ['do ##class(Iter5.Probe).Redirect()'])[1], obs)
        session('3-load-tests', ['do ##class(Iter5.Probe).Cmd("Z0-load","load /tmp/g5-iter5/tmod")'])
        forms = {
            'Z1-zpm-command-flag': ['zpm "test g5i4-tests -only",1', 'write !,"AFTER-MARK",!'],
            'Z2-zpm-command': ['zpm "test g5i4-tests -only"', 'write !,"AFTER-MARK",!'],
            'Z3-shell-flag-stdin': ['do ##class(%IPM.Main).Shell("test g5i4-tests -only",1)', 'write !,"AFTER-MARK",!'],
            'Z4-zpm-function-fail': ['set x=$zpm("test g5i4-tests -only") write !,"FN-RESULT|",x,!', 'write !,"AFTER-MARK",!'],
            'Z5-zpm-function-pass': ['set x=$zpm("test g5i4-tests -only -DUnitTest.Suite=G5I4.Pass") write !,"FN-RESULT|",x,!', 'write !,"AFTER-MARK",!'],
        }
        exits = {}
        for case, lines in forms.items():
            p, raw = session(case, lines, check=False)
            exits[case] = {'exit': p.returncode, 'continued': re.search(r'^AFTER-MARK', raw, re.M) is not None,
                           'command_error': 'Function must return a value' in raw, 'function_result_printed': re.search(r'^FN-RESULT\|', raw, re.M) is not None,
                           'failure_reported': 'failure(s)' in raw}
        requests = run(['docker', 'exec', C, 'cat', '/tmp/g5-iter5/requests.jsonl']).stdout
        (OUT / 'requests.jsonl').write_text(requests)
        (OUT / 'container-resource.txt').write_text(run(['docker', 'exec', C, 'sh', '-c', 'cat /sys/fs/cgroup/cpu.stat; echo memory.peak $(cat /sys/fs/cgroup/memory.peak 2>/dev/null)'], check=False).stdout)
        result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'container': C, 'image': IMAGE, 'product_pin': PIN,
                  'wall_seconds': round(time.monotonic() - started, 3), 'observations': obs, 'entry_forms': exits,
                  'requests': [json.loads(x) for x in requests.splitlines() if x.strip()],
                  'fixture_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((HERE / 'native').rglob('*')) if p.is_file()},
                  'commands': COMMANDS}
        (OUT / 'observations.json').write_text(json.dumps(result, indent=2) + '\n')
        (OUT.parent / 'LATEST').write_text(STAMP + '\n')
        print(json.dumps({'out': str(OUT.relative_to(ROOT)), 'cases': len(obs), 'entry_forms': exits, 'wall_seconds': result['wall_seconds']}))
    finally:
        if '--keep' not in sys.argv:
            subprocess.run(['docker', 'rm', '-f', C], capture_output=True)


if __name__ == '__main__':
    main()
