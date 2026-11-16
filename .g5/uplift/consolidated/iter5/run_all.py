#!/usr/bin/env python3
"""Clean-scratch proof/control command for iteration 5.

python3 .g5/uplift/consolidated/iter5/run_all.py [--no-native] [baseline-members.json]

Runs the contracts and wrong variants, a fresh native suite (unless --no-native, in which case the
recorded observations are replayed), the native-to-Verus replay with flipped controls, and the
validator. Writes run_all.json with per-step wall and child CPU time.
"""
from pathlib import Path
import json, resource, subprocess, sys, time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
V = ROOT / '.g5/Verus_verification/consolidated/iter5'


def step(name, args):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.monotonic()
    p = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    return {'step': name, 'command': args, 'exit': p.returncode, 'wall_seconds': round(time.monotonic() - start, 3),
            'child_cpu_seconds': round(after.ru_utime - before.ru_utime + after.ru_stime - before.ru_stime, 3),
            'peak_child_rss_bytes': after.ru_maxrss, 'tail': (p.stdout + p.stderr)[-1500:]}


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    steps = [step('models', [sys.executable, str(V / 'run_models.py')])]
    if '--no-native' not in sys.argv:
        steps.append(step('native', [sys.executable, str(V / 'run_native.py')]))
    steps.append(step('replay', [sys.executable, str(V / 'replay_native.py')]))
    steps.append(step('validate', [sys.executable, str(HERE / 'validate.py')] + args))
    ok = all(s['exit'] == 0 for s in steps)
    (HERE / 'run_all.json').write_text(json.dumps({'ok': ok, 'steps': steps}, indent=2) + '\n')
    for s in steps:
        print(s['step'], s['exit'], s['wall_seconds'], 's')
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
