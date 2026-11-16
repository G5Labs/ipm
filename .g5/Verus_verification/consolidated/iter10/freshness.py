"""Conservative proof/correspondence invalidation, including newly added writers.

python3 .../freshness.py seal | check | selftest
An unchanged theorem may be reused after a product-only change, but its product
correspondence is reopened. Any production change reopens this iteration's scope.
"""
from pathlib import Path
import hashlib,json,sys,subprocess
HERE=Path(__file__).resolve().parent; ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated'/HERE.name
TOOL=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def inventory(root):
 rows={str(p.relative_to(root)):sha(p) for d in ['src','preload','modules','tests'] for p in sorted((root/d).rglob('*')) if p.is_file()}
 rows['@native-image']=subprocess.check_output(['docker','image','inspect','ipm-consolidated-iris:571e04a1-fresh','--format','{{.Id}}'],text=True).strip()
 return rows
def formal():
 rows={str(p.relative_to(ROOT)):sha(p) for p in sorted(HERE.rglob('*')) if p.is_file() and '__pycache__' not in str(p)}
 rows.update({str(p):sha(p) for p in [TOOL/'verus',TOOL/'rust_verify',TOOL/'z3',TOOL/'vstd.vir',TOOL/'libvstd.rlib',TOOL/'libverus_builtin.rlib',TOOL/'libverus_builtin_macros.dylib']})
 return rows
def delta(before,after):return sorted(k for k in before.keys()|after.keys() if before.get(k)!=after.get(k))
def verdict(old,product,proof):
 p=delta(old['production'],product); f=delta(old['formal'],proof)
 return {'theorem':'stale' if f else 'reusable','correspondence':'stale' if p or f else 'fresh','production_changes':p,'formal_changes':f,'scope':'All current/inherited native correspondence using these production, fixture and image inputs reopens conservatively; theorem reuse verdict covers only '+HERE.name+' formal inputs.'}
def main():
 OUT.mkdir(exist_ok=True); receipt=OUT/'freshness-receipt.json';cmd=sys.argv[1]
 if cmd=='seal':
  receipt.write_text(json.dumps({'production':inventory(ROOT),'formal':formal(),'direct_implementation_proofs':0},indent=2)+'\n');print('sealed',receipt);return
 old=json.loads(receipt.read_text());prod=inventory(ROOT);proof=formal()
 if cmd=='check':
  r=verdict(old,prod,proof);print(json.dumps(r));assert r['correspondence']=='fresh';return
 assert cmd=='selftest'; assert verdict(old,prod,proof)['correspondence']=='fresh'
 # Preserve actual source/formal edits in durable fixtures, without changing live.
 worlds=OUT/'freshness-worlds';worlds.mkdir(exist_ok=True);records=[]
 source='src/cls/IPM/Lifecycle/Base.cls'
 file=worlds/'source-only.cls';file.write_text((ROOT/source).read_text().replace('do tResource.ResolveChildren(.tResourceChildren)','set tSC = tResource.ResolveChildren(.tResourceChildren)\n                $$$ThrowOnError(tSC)',1))
 changed=dict(prod);changed[source]=sha(file);records.append({'world':'product-only Clean checks returned child status','fixture':str(file.relative_to(ROOT)),'verdict':verdict(old,changed,proof)})
 model=str((HERE/'errors.rs').relative_to(ROOT));file=worlds/'formal-only.rs';file.write_text((ROOT/model).read_text().replace('Outcome{ok:true,state:State{module:false','Outcome{ok:r.ok,state:State{module:false',1))
 changed=dict(proof);changed[model]=sha(file);records.append({'world':'formal-only Clean result rule changed','fixture':str(file.relative_to(ROOT)),'verdict':verdict(old,prod,changed)})
 file=worlds/'new-writer.cls';file.write_text('Class IPM.FreshnessWriter {\nClassMethod Register(m,s) {\n set m.IncludeTests=1\n do s.%Save()\n}\n}\n')
 changed=dict(prod);changed['src/cls/IPM/FreshnessWriter.cls']=sha(file);records.append({'world':'new production writer registered','fixture':str(file.relative_to(ROOT)),'verdict':verdict(old,changed,proof)})
 assert records[0]['verdict']['theorem']=='reusable' and records[0]['verdict']['correspondence']=='stale'
 assert records[1]['verdict']['theorem']=='stale' and records[1]['verdict']['correspondence']=='stale'
 assert records[2]['verdict']['theorem']=='reusable' and records[2]['verdict']['correspondence']=='stale'
 (OUT/'freshness-selftest.json').write_text(json.dumps({'old_receipt_sha256':sha(receipt),'records':records,'passed':True},indent=2)+'\n');print('three staleness worlds passed')
if __name__=='__main__':main()
