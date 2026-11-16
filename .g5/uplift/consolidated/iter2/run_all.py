#!/usr/bin/env python3
"""Reproduce iter2 checks; --native adds real isolated-provider probes, --inherited replays snapshots."""
from pathlib import Path
import argparse,subprocess,sys,json,time
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
def main():
 p=argparse.ArgumentParser();p.add_argument('--native',action='store_true');p.add_argument('--inherited',action='store_true');a=p.parse_args()
 formal=ROOT/'.g5/Verus_verification/consolidated/iter2'
 commands=[('new-contracts',[sys.executable,str(formal/'run_models.py')]),
  ('boundaries',[sys.executable,str(HERE/'boundaries/check.py')]),
  ('member-census',[sys.executable,str(HERE/'inventory/triage.py')]),
  ('bindings',[sys.executable,str(HERE/'build_index.py')]),
  ('known-panel',[sys.executable,str(HERE/'inventory/review_known.py')]),
  ('preservation',[sys.executable,str(HERE/'validate.py')])]
 if a.native:
  commands.insert(2,('native-readers',[sys.executable,str(formal/'run_native.py')]))
  commands.insert(3,('native-boundaries',[sys.executable,str(HERE/'boundaries/native_driver.py')]))
 if a.inherited:commands.append(('inherited-disposable',[sys.executable,str(HERE/'replay_inherited.py')]))
 out=HERE/'gate';out.mkdir(exist_ok=True);rows=[]
 for name,cmd in commands:
  start=time.monotonic();r=subprocess.run(cmd,cwd=ROOT,text=True,capture_output=True,timeout=600)
  (out/(name+'.log')).write_text(r.stdout+r.stderr)
  rows.append({'name':name,'command':cmd,'exit':r.returncode,'seconds':round(time.monotonic()-start,3),'log':str((out/(name+'.log')).relative_to(ROOT))})
  (out/'receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,r.returncode,flush=True)
  if r.returncode:print((r.stdout+r.stderr)[-2500:]);sys.exit(r.returncode)
if __name__=='__main__':main()
