#!/usr/bin/env python3
"""Reproduce the new assurance gates in-place; inherited rerun is optional and always disposable.
No product build, git-index write, shared-instance reset, or historical result rewrite.
"""
from pathlib import Path
import subprocess,json,sys,time,re,hashlib
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];V=ROOT/'.g5/Verus_verification/consolidated/verus';TOOL=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
OUT=HERE/'final-run';OUT.mkdir(exist_ok=True);results=[]
commands=[('original-two-counts',[sys.executable,str(HERE/'reproduce_counts.py')]),('env-attr',[sys.executable,str(V/'check_env_attr.py')]),('semver',[sys.executable,str(V/'semver_check.py')]),('owners',[sys.executable,str(V/'owners_check.py')]),('parent-models',[sys.executable,str(V/'check_parent.py')]),('thin-surfaces',[sys.executable,str(V/'check_thin_surfaces.py')]),('inherited-theorem-bridge',[str(TOOL),str(V/'inherited_bridge.rs'),'--rlimit','30','--triggers-mode','silent','-o',str(OUT/'inherited-bridge')]),('flag-overrides',[sys.executable,str(V/'check_flag_overrides.py')]),('old-rule-extension',[sys.executable,str(HERE/'extend_old_rule.py')]),('old-rule-count',[sys.executable,str(HERE.parent/'coverage/count.py')]),('old-rule-controls',[sys.executable,str(HERE.parent/'coverage/count.py'),'--selftest']),('binding-endpoints',[sys.executable,str(HERE/'build_coverage.py')]),('five-passes',[sys.executable,str(HERE/'five_passes.py')]),('drift',[sys.executable,str(HERE/'check_drift.py')]),('precision',[sys.executable,str(HERE/'precision_answers.py')]),('preservation',[sys.executable,str(HERE/'check_preservation.py')])]
if '--inherited' in sys.argv:commands.insert(0,('inherited-baselines',[sys.executable,str(HERE/'reproduce_baselines.py')]))
for name,cmd in commands:
 start=time.monotonic();r=subprocess.run(cmd,cwd=ROOT,text=True,capture_output=True,timeout=None if name=='inherited-baselines' else 1200)
 (OUT/(name+'.log')).write_text(r.stdout+r.stderr);row={'name':name,'command':cmd,'returncode':r.returncode,'seconds':round(time.monotonic()-start,3),'log':str((OUT/(name+'.log')).relative_to(ROOT))};results.append(row)
 (OUT/'results.json').write_text(json.dumps(results,indent=2));print(name,r.returncode,flush=True)
 if r.returncode:print((r.stdout+r.stderr)[-2500:]);sys.exit(r.returncode)
print('All new model, native comparison, control, count and drift checks passed.')
