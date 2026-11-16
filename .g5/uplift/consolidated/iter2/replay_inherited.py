#!/usr/bin/env python3
"""Rerun inherited consolidated checks in a disposable extraction, preserving old receipts."""
from pathlib import Path
import argparse, tarfile, tempfile, subprocess, time, json, sys, shutil
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--only',nargs='*');args=parser.parse_args()
 names=args.only or ['check_env_attr.py','semver_check.py','owners_check.py','check_parent.py','check_thin_surfaces.py','check_flag_overrides.py']
 out=HERE/'inherited-replay';out.mkdir(exist_ok=True)
 archive=ROOT.parent/'results/experiments/uplift-measurement-20261114/ipm-iter2/baseline/actual-iter1.tar.gz'
 with tempfile.TemporaryDirectory(prefix='ipm-iter2-inherited-') as td:
  temp=Path(td).resolve();skipped=[]
  def filt(m,p):
   if m.issym() or m.islnk():skipped.append(m.name);return None
   return tarfile.data_filter(m,p)
  with tarfile.open(archive) as t:t.extractall(temp,filter=filt)
  results=[]
  for name in names:
   cmd=[sys.executable,str(temp/'.g5/Verus_verification/consolidated/verus'/name)];start=time.monotonic()
   p=subprocess.run(cmd,capture_output=True,text=True,timeout=300,cwd=temp)
   (out/(name+'.retry.log')).write_text(p.stdout+p.stderr)
   results.append({'name':name,'command':cmd,'exit':p.returncode,'seconds':time.monotonic()-start})
   print(name,p.returncode,flush=True)
  for p in (temp/'.g5/Verus_verification/consolidated/verus').rglob('*.json'):
   dest=out/'generated-retry'/p.relative_to(temp/'.g5/Verus_verification/consolidated/verus');dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,dest)
  (out/'retry-receipt.json').write_text(json.dumps({'checks':results,'skipped_scratch_links':skipped,'path_policy':'Canonical resolved tempfile path avoids /var versus /private/var alias in inherited relative_to checks.'},indent=2)+'\n')
  assert all(x['exit']==0 for x in results)
if __name__=='__main__':main()
