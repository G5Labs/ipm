from pathlib import Path
import tempfile,subprocess,json,shutil,re,time,hashlib
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
# The first batch exceeded its20-minute cap on the last file. Seven completed file receipts were retained.
# Re-run the remaining original checker in a new copy; no inherited source/result is modified.
with tempfile.TemporaryDirectory(prefix='ipm-inherited-last-') as td:
 d=Path(td);(d/'models').mkdir();shutil.copytree(ROOT/'playbook-uplift/models/verus',d/'models/verus')
 cmd=['python3','-I','models/verus/controls.py','update_steps'];start=time.monotonic()
 p=subprocess.run(cmd,cwd=d,capture_output=True,text=True,timeout=1200)
 (HERE/'before-P-update_steps.log').write_text(p.stdout+p.stderr)
 assert p.returncode==0,(p.stdout+p.stderr)[-2000:]
 out=HERE/'before-P-results';out.mkdir(exist_ok=True)
 for f in (HERE/'before-P-partial').glob('*.json'):shutil.copyfile(f,out/f.name)
 shutil.copyfile(d/'models/verus/results/update_steps.json',out/'update_steps.json')
 rows=[json.loads(f.read_text()) for f in out.glob('*.json')];assert len(rows)==8
 assert all(r['all_killed_in_expected_place'] for r in rows)
 counts=sum(r['baseline'][0] for r in rows);controls=sum(len(r['controls']) for r in rows)
 assert counts==227 and controls==195,(counts,controls)
 summary={'G':{'verified':263,'errors':0,'receipt':'before-G-verus.log'},'P':{'verified':counts,'errors':0,'controls':controls,'controls_killed':sum(c['status']=='killed' for r in rows for c in r['controls']),'receipts':'before-P-results/*.json','recovery_command':cmd,'last_file_seconds':time.monotonic()-start,'batch_note':'Initial all-files run reached20-minute timeout in final update_steps file; seven decisive fresh file receipts retained; last file independently rerun unchanged under original checker. Timeout was not counted as pass.'}}
 (HERE/'before-verus.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary),flush=True)
