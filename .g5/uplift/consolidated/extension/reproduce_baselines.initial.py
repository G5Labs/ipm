from pathlib import Path
import tempfile,shutil,subprocess,json,hashlib,os,time
ROOT=Path(__file__).resolve().parents[4]; OUT=Path(__file__).resolve().parent
VERUS=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
rows=[]
with tempfile.TemporaryDirectory(prefix='ipm-inherited-') as td:
 d=Path(td); g=d/'g'; shutil.copytree(ROOT/'.g5/Verus_verification/crates/ipm-load-history/src',g)
 f=g/'lib.rs'; f.write_text(f.read_text().replace('pub mod explore;','// plain Rust harness excluded, as original receipt\n// pub mod explore;'))
 commands=[('G',[str(VERUS),'lib.rs','--crate-type=lib','--num-threads','4'],g)]
 p=d/'playbook-uplift'; shutil.copytree(ROOT/'playbook-uplift',p,ignore=shutil.ignore_patterns('__pycache__'))
 commands.append(('P',['python3','-I','models/verus/controls.py'],p))
 for arm,cmd,cwd in commands:
  start=time.monotonic(); r=subprocess.run(cmd,cwd=cwd,capture_output=True,text=True,timeout=1200)
  (OUT/f'before-{arm}-verus.log').write_text(r.stdout+r.stderr)
  row={'arm':arm,'command':cmd,'returncode':r.returncode,'seconds':time.monotonic()-start,'disposable_copy':True};rows.append(row)
  if arm=='P': shutil.copytree(p/'models/verus/results',OUT/'before-P-results',dirs_exist_ok=True)
  print(row,flush=True)
(OUT/'before-verus.json').write_text(json.dumps(rows,indent=2))
assert all(r['returncode']==0 for r in rows)
