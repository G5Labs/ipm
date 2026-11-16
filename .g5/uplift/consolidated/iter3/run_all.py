#!/usr/bin/env python3
"""Complete iter3 gate; add --native and --inherited to rerun those environments."""
from pathlib import Path
import argparse,json,resource,subprocess,sys,time
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];FORMAL=ROOT/'.g5/Verus_verification/consolidated/iter3'
def main():
    p=argparse.ArgumentParser();p.add_argument('--native',action='store_true');p.add_argument('--inherited',action='store_true');a=p.parse_args()
    commands=[('new-contracts',[sys.executable,FORMAL/'run_models.py'])]
    if a.native:commands.append(('native',[sys.executable,FORMAL/'run_native.py']))
    commands += [('native-bridge',[sys.executable,FORMAL/'replay_native.py']),('index',[sys.executable,HERE/'build_index.py'])]
    if a.inherited:commands.append(('inherited',[sys.executable,HERE/'replay_inherited.py']))
    commands.append(('validate',[sys.executable,HERE/'validate.py']))
    out=HERE/'gate';out.mkdir(exist_ok=True);rows=[]
    for name,cmd in commands:
        before=resource.getrusage(resource.RUSAGE_CHILDREN);start=time.monotonic()
        r=subprocess.run(list(map(str,cmd)),cwd=ROOT,capture_output=True,text=True,timeout=1200)
        after=resource.getrusage(resource.RUSAGE_CHILDREN);(out/(name+'.log')).write_text(r.stdout+r.stderr)
        rows.append({'name':name,'command':list(map(str,cmd)),'exit':r.returncode,'wall_seconds':time.monotonic()-start,'child_user_seconds':after.ru_utime-before.ru_utime,'child_system_seconds':after.ru_stime-before.ru_stime,'peak_child_rss_bytes_cumulative_max':after.ru_maxrss})
        (out/'receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,r.returncode,flush=True)
        if r.returncode:print((r.stdout+r.stderr)[-3000:]);raise SystemExit(r.returncode)
if __name__=='__main__':main()
