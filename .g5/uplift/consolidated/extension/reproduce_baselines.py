#!/usr/bin/env python3
"""Original G/P proof rules on disposable copies; each file gate capped at20 minutes.
The initial all-P batch cap was too short; per-file receipts prevent discarding completed checks.
"""
from pathlib import Path
import tempfile,shutil,subprocess,json,hashlib,time,re
ROOT=Path(__file__).resolve().parents[4];OUT=Path(__file__).resolve().parent
VERUS=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
summary={};out=OUT/'before-P-results';out.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='ipm-inherited-') as td:
 d=Path(td);g=d/'g';shutil.copytree(ROOT/'.g5/Verus_verification/crates/ipm-load-history/src',g)
 f=g/'lib.rs';f.write_text(f.read_text().replace('pub mod explore;','// plain Rust explorer harness, excluded exactly as original receipt\n// pub mod explore;'))
 cmd=[str(VERUS),'lib.rs','--crate-type=lib','--num-threads','4'];r=subprocess.run(cmd,cwd=g,capture_output=True,text=True,timeout=1200)
 (OUT/'before-G-verus.log').write_text(r.stdout+r.stderr);assert r.returncode==0 and '263 verified, 0 errors' in r.stdout
 summary['G']={'verified':263,'errors':0,'command':cmd};print('G263 verified,0 errors',flush=True)
 p=d/'playbook-uplift';shutil.copytree(ROOT/'playbook-uplift',p,ignore=shutil.ignore_patterns('__pycache__'))
 commands=[]
 for f in sorted((p/'models/verus/controls').glob('*.json')):
  stem=f.stem;cmd=['python3','-I','models/verus/controls.py',stem];start=time.monotonic()
  r=subprocess.run(cmd,cwd=p,capture_output=True,text=True,timeout=1200)
  (OUT/('before-P-'+stem+'.log')).write_text(r.stdout+r.stderr);assert r.returncode==0,(stem,(r.stdout+r.stderr)[-1000:])
  shutil.copyfile(p/'models/verus/results'/f.name,out/f.name);commands.append({'command':cmd,'seconds':time.monotonic()-start});print(stem,'passed',flush=True)
 rows=[json.loads(f.read_text()) for f in out.glob('*.json')];assert len(rows)==8 and all(r['all_killed_in_expected_place'] for r in rows)
 verified=sum(r['baseline'][0] for r in rows);controls=sum(len(r['controls']) for r in rows);assert (verified,controls)==(227,195)
 summary['P']={'verified':verified,'errors':0,'controls':controls,'controls_killed':sum(c['status']=='killed' for r in rows for c in r['controls']),'commands':commands}
(OUT/'before-verus.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary))
