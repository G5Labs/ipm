#!/usr/bin/env python3
"""Iteration 4 native cases in a fresh disposable IRIS container from the pinned image.

Creates ipm-iter4-native-<stamp> (or $IPM_ITER4_CONTAINER), checks on-disk product sources against
the pin, copies fixtures, starts the two-origin registry provider, runs every probe session, parses
OBS lines and records raw logs, provider requests, exit codes, timings and container counters under
.g5/uplift/consolidated/iter4/native/<stamp>/. The container is removed afterwards unless --keep.
No shared container is touched and no product file is changed.
"""
from pathlib import Path
import datetime, hashlib, json, os, subprocess, sys, time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
IMAGE = 'ipm-consolidated-iris:571e04a1-fresh'
PIN = '571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
STAMP = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
OUT = ROOT / '.g5/uplift/consolidated/iter4/native' / STAMP
C = os.environ.get('IPM_ITER4_CONTAINER', 'ipm-iter4-native-' + STAMP.lower())
COMMANDS = []


def run(args, check=True, timeout=600, **kw):
    start = time.monotonic()
    p = subprocess.run(args, capture_output=True, text=True, timeout=timeout, **kw)
    row = {'command': args, 'exit': p.returncode, 'seconds': round(time.monotonic() - start, 3)}
    if 'input' in kw:
        path = OUT / 'stdin' / ('command-%02d.txt' % (len(COMMANDS) + 1))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(kw['input'])
        row['stdin'] = str(path.relative_to(ROOT))
    COMMANDS.append(row)
    if check and p.returncode:
        raise RuntimeError((args, p.stdout[-2000:], p.stderr[-2000:]))
    return p


def session(name, lines):
    script = '\n'.join(lines + ['halt']) + '\n'
    p = run(['docker', 'exec', '-i', C, 'iris', 'session', 'IRIS', '-U', 'USER'], input=script)
    raw = p.stdout + p.stderr
    (OUT / ('session-' + name + '.log')).write_text(raw)
    return raw


def parse(raw, into):
    for line in raw.splitlines():
        if line.startswith('OBS|'):
            _, case, field, value = (line.split('|', 3) + [''])[:4]
            into.setdefault(case, {})[field] = value.strip()


def main():
    keep = '--keep' in sys.argv
    OUT.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    run(['docker', 'run', '-d', '--name', C, IMAGE])
    try:
        for _ in range(120):
            p = subprocess.run(['docker', 'exec', '-i', C, 'iris', 'session', 'IRIS', '-U', 'USER'],
                               input='write "I4-READY",!\nhalt\n', capture_output=True, text=True)
            if p.returncode == 0 and 'I4-READY' in p.stdout:
                break
            time.sleep(1)
        else:
            raise RuntimeError('fresh container did not become ready')
        ready_seconds = time.monotonic() - started
        # Product source identity on disk (not a proof of compiled-code identity).
        # pin-sources.json was generated from `git show <pin>:<path>`; it lets an extracted seal without .git run this check.
        expected = json.loads((HERE / 'native/pin-sources.json').read_text())['source_sha256']
        if subprocess.run(['git', 'rev-parse', '--git-dir'], cwd=ROOT, capture_output=True).returncode == 0:
            paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', PIN, '--', 'src', 'preload', 'modules'], cwd=ROOT, text=True).splitlines()
            assert expected == {p: hashlib.sha256(subprocess.check_output(['git', 'show', PIN + ':' + p], cwd=ROOT)).hexdigest() for p in paths}
        script = ('import hashlib,json; from pathlib import Path; root=Path("/home/irisowner/zpm"); wanted=' + repr(list(expected)) +
                  '; print(json.dumps({p:hashlib.sha256((root/p).read_bytes()).hexdigest() if (root/p).is_file() else None for p in wanted}))')
        actual = json.loads(run(['docker', 'exec', C, 'python3', '-c', script]).stdout)
        mismatches = [p for p, v in expected.items() if actual[p] != v]
        (OUT / 'source-pin.json').write_text(json.dumps({'product_pin': PIN, 'container': C, 'paths': len(expected), 'mismatches': mismatches,
                                                         'scope': 'On-disk product source identity, not compiled-code identity.'}, indent=2) + '\n')
        assert not mismatches, mismatches
        run(['docker', 'exec', C, 'mkdir', '-p', '/tmp/g5-iter4/out'])
        for item in ['fixtures/tmod', 'fixtures/synca', 'fixtures/syncb', 'Probe.cls', 'FailResolve.cls', 'SyncProbe.cls', 'RelPath.cls', 'registry.py']:
            run(['docker', 'cp', str(HERE / 'native' / item), C + ':/tmp/g5-iter4/' + Path(item).name])
        run(['docker', 'exec', '-d', C, 'python3', '/tmp/g5-iter4/registry.py'])
        for _ in range(30):
            if subprocess.run(['docker', 'exec', C, 'test', '-f', '/tmp/g5-iter4/registry-sha1.json']).returncode == 0:
                break
            time.sleep(0.5)
        obs = {}
        load = ['write !,"OBS|L-%s|ok|",$system.OBJ.Load("/tmp/g5-iter4/%s.cls","ck-d"),!' % (n, n) for n in ['Probe', 'FailResolve', 'SyncProbe', 'RelPath']]
        parse(session('1-test-outcome', load + ['do ##class(Iter4.Probe).TestOutcome()', 'do ##class(Iter4.Probe).Producers()']), obs)
        parse(session('2-accumulator', ['do ##class(Iter4.Probe).Accumulator()']), obs)
        parse(session('3-registry', ['do ##class(Iter4.Probe).Registry()', 'do ##class(Iter4.Probe).RegistryInstall()']), obs)
        parse(session('4-sync', ['do ##class(Iter4.SyncProbe).Drive()', 'do ##class(Iter4.RelPath).Drive()']), obs)
        exits = {}
        for case, command, terminate in [('E1-fail-terminate', 'test g5i4-tests -only', 1),
                                         ('E2-pass-terminate', 'test g5i4-tests -only -DUnitTest.Suite=G5I4.Pass', 1),
                                         ('E3-fail-no-terminate', 'test g5i4-tests -only', 0),
                                         ('E4-not-fatal-terminate', 'test g5i4-tests -only -DUnitTest.FailuresAreFatal=0', 1)]:
            arg = '##class(%%IPM.Main).Shell("%s",%d)' % (command, terminate) if terminate else '##class(%%IPM.Main).Shell("%s")' % command
            p = run(['docker', 'exec', C, 'iris', 'session', 'IRIS', '-U', 'USER', arg.replace('%%', '%')], check=False)
            (OUT / ('exit-' + case + '.log')).write_text(p.stdout + p.stderr)
            exits[case] = p.returncode
        requests = run(['docker', 'exec', C, 'cat', '/tmp/g5-iter4/requests.jsonl']).stdout
        (OUT / 'requests.jsonl').write_text(requests)
        cpu = run(['docker', 'exec', C, 'sh', '-c', 'cat /sys/fs/cgroup/cpu.stat; echo memory.peak $(cat /sys/fs/cgroup/memory.peak 2>/dev/null)'], check=False).stdout
        (OUT / 'container-resource.txt').write_text(cpu)
        warnings = {}
        for name in ['4-sync']:
            text = (OUT / ('session-' + name + '.log')).read_text()
            warnings[name] = text.count('WARNING: could not resolve contents of resource')
        result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'container': C, 'image': IMAGE, 'product_pin': PIN,
                  'ready_seconds': round(ready_seconds, 3), 'wall_seconds': round(time.monotonic() - started, 3),
                  'observations': obs, 'exit_codes': exits, 'printed_resolve_warnings': warnings,
                  'requests': [json.loads(x) for x in requests.splitlines() if x.strip()],
                  'fixture_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                                     for p in sorted((HERE / 'native').rglob('*')) if p.is_file()},
                  'commands': COMMANDS}
        (OUT / 'observations.json').write_text(json.dumps(result, indent=2) + '\n')
        (OUT.parent / 'LATEST').write_text(STAMP + '\n')
        print(json.dumps({'out': str(OUT.relative_to(ROOT)), 'cases': len(obs), 'exit_codes': exits, 'wall_seconds': result['wall_seconds']}))
    finally:
        if not keep:
            subprocess.run(['docker', 'rm', '-f', C], capture_output=True)


if __name__ == '__main__':
    main()
