from pathlib import Path
import tempfile,shutil,subprocess,json,time,hashlib,sys
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];OUT=HERE/'P-explorer-rerun';OUT.mkdir(exist_ok=True)
rows=json.loads((OUT/'results.json').read_text()) if '--only-install' in sys.argv and (OUT/'results.json').exists() else []
with tempfile.TemporaryDirectory(prefix='ipm-P-explorers-') as td:
 base=Path(td);p=base/'results/experiments/ipm-playbook-uplift';p.parent.mkdir(parents=True);shutil.copytree(ROOT/'playbook-uplift',p,ignore=shutil.ignore_patterns('__pycache__'));(base/'ipm').symlink_to(ROOT)
 for name,path,outfile in [('load','models/explorer/explore.py','model-checks.json'),('install-resolve','models/explorer/inst/check.py','model-checks-inst.json'),('namespace-sync','models/explorer/nsp/check.py','model-checks-nsp.json'),('repository','models/explorer/repo/check.py','model-checks-repo.json')]:
  if '--only-install' in sys.argv and name!='install-resolve':continue
  produced=p/'evidence'/outfile
  if produced.exists():produced.rename(produced.with_suffix('.initial.json'))
  runtime='/opt/homebrew/bin/python3.11' if name=='install-resolve' else 'python3'
  start=time.monotonic();cmd=[runtime,'-I',path];r=subprocess.run(cmd,cwd=p,capture_output=True,text=True,timeout=600);(OUT/(name+'.log')).write_text(r.stdout+r.stderr)
  produced=p/'evidence'/outfile;before=json.loads((ROOT/'playbook-uplift/evidence'/outfile).read_text());after=json.loads(produced.read_text()) if produced.exists() else {};shutil.copyfile(produced,OUT/outfile) if produced.exists() else None
  b={z['name']:z.get('status') for z in before.get('records',[])};a={z['name']:z.get('status') for z in after.get('records',[])}
  changed=[{'name':k,'before':b.get(k),'after':a.get(k)} for k in sorted(set(b)|set(a)) if b.get(k)!=a.get(k)]
  rows=[z for z in rows if z['name']!=name]
  row={'name':name,'command':cmd,'returncode':r.returncode,'seconds':time.monotonic()-start,'status_changes':changed,'record_count':len(a),'failing_records':[k for k,v in a.items() if v!='pass'],'disposable_copy':True};rows.append(row);print(row,flush=True)
(OUT/'results.json').write_text(json.dumps(rows,indent=2));assert all(r['returncode']==0 and not r['status_changes'] for r in rows),rows
