from pathlib import Path
import tempfile,subprocess,shutil,json,sys,hashlib
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];OUT=HERE/'P-explorer-rerun';rows=[];readsets={}
with tempfile.TemporaryDirectory(prefix='ipm-P-audit-') as td:
 base=Path(td);p=base/'results/experiments/ipm-playbook-uplift';p.parent.mkdir(parents=True);shutil.copytree(ROOT/'playbook-uplift',p,ignore=shutil.ignore_patterns('__pycache__'));(base/'ipm').symlink_to(ROOT)
 for name,script in [('load','models/explorer/explore.py'),('install-resolve','models/explorer/inst/check.py'),('namespace-sync','models/explorer/nsp/check.py'),('repository','models/explorer/repo/check.py')]:
  audit=base/(name+'-read-inputs.json');wrapper=base/(name+'-audit.py')
  wrapper.write_text('''import sys,os,runpy,json
reads=set()
def audit(event,args):
 if event=='open':
  path,mode,flags=args
  if isinstance(path,(str,bytes)) and (mode is not None and 'r' in mode or mode is None and flags & (os.O_WRONLY|os.O_RDWR)==0):reads.add(os.fsdecode(path))
sys.addaudithook(audit)
script=sys.argv[1];destination=sys.argv[2];sys.argv=[script]
try:runpy.run_path(script,run_name='__main__')
finally:
 with open(destination,'w') as f:json.dump(sorted(reads),f)
''')
  cmd=['/opt/homebrew/bin/python3.11','-I',str(wrapper),str(p/script),str(audit)];r=subprocess.run(cmd,cwd=p,capture_output=True,text=True,timeout=600);(OUT/(name+'-audited.log')).write_text(r.stdout+r.stderr)
  assert r.returncode==0,(name,(r.stdout+r.stderr)[-1000:])
  reads={}
  for raw in json.loads(audit.read_text()):
   f=Path(raw);f=f if f.is_absolute() else p/f
   if not f.is_file():continue
   if p in f.parents:canonical=ROOT/'playbook-uplift'/f.relative_to(p)
   else:canonical=f.resolve()
   if not canonical.is_file():continue
   # pyc imports are represented by their source where a source file exists, plus interpreter version.
   if '__pycache__' in str(canonical):
    stem=canonical.name.split('.cpython-')[0];source=canonical.parent.parent/(stem+'.py')
    if source.exists():canonical=source
   reads[str(canonical)]=hashlib.sha256(canonical.read_bytes()).hexdigest()
  readsets[name]=reads;rows.append({'name':name,'command':cmd,'returncode':r.returncode,'actual_read_inputs':len(reads),'runtime':'Python3.11.10','input_audit':'CPython open audit events; source mapped back from untouched disposable copy'})
(OUT/'actual-inputs.json').write_text(json.dumps({'checks':readsets,'receipts':rows},indent=2));print([(r['name'],r['actual_read_inputs']) for r in rows])
