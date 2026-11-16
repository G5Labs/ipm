from pathlib import Path
import json,re,hashlib,subprocess,os,time
r=Path('.g5/Verus_verification/consolidated/iter23');out=r/'model-lane';p=r/'status-reader-cases.json';cfg=json.loads(p.read_text())
# Preserve exact configuration underlying the completed control suite.
old=out/'controls/config.json'
if not old.exists():old.write_text(p.read_text())
cfg['dependency_controls']={};cfg['replay_model']='status_reader.rs'
# Scope metadata is already recorded by the isolation configuration.
p.write_text(json.dumps(cfg,indent=2)+'\n')
rec=json.loads((out/'controls/receipt.json').read_text());audit=[]
for x in rec['runs']:
 if not x.get('semantic_rejection'):continue
 log=(out/'controls'/(x['name']+'.log')).read_text();src=(out/'controls'/(x['name']+'.rs')).read_text().splitlines()
 sites=[]
 for line in re.findall(r'\.rs:(\d+):\d+',log):
  ln=int(line);fns=[re.search(r'pub proof fn (\w+)',t).group(1) for t in src[:ln] if re.search(r'pub proof fn (\w+)',t)]
  sites.append({'line':ln,'function':fns[-1] if fns else 'unknown'})
 audit.append({'name':x['name'],'selector':x['selector'],'first_error_sites':sites,'early_compute':'evaluates to false' in log or 'expression simplifies to' in log,'accepted_semantic':x['accepted']})
(out/'controls/scope-audit.json').write_text(json.dumps({'scope':cfg['control_scope_note'],'controls':audit},indent=2)+'\n')
n=Path(cfg['native_receipt']);native=json.loads(n.read_text());obs=native['observations']
assert set(obs)==set(cfg['replay'])|set(cfg['native_expected']);assert all(obs[k]==v for k,v in cfg['native_expected'].items())
s=out/'replay.rs';body='use vstd::prelude::*;\n#[path="'+os.path.relpath(r/'status_reader.rs',s.parent)+'"] pub mod model;\nuse model::*;\nverus! {pub proof fn observed_group() {\n'+cfg['replay_prelude']+'\n'
for k,v in cfg['replay'].items():body+='// '+k+'\n'+cfg.get('replay_preludes',{}).get(k,'')+'\nassert(('+v+')=='+str(obs[k])+');\n'
s.write_text(body+'}}\nfn main() {}\n')
cmd=['/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus',str(s),'--triggers-mode','silent'];t=time.monotonic()
with (out/'replay.log').open('w') as log:q=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,timeout=120)
subset=lambda j:{k:j.get(k,{}) for k in ['models','dependency_controls','control_functions','control_options']}
assert subset(cfg)==subset(json.loads(old.read_text()))
res={'command':cmd,'exit':q.returncode,'wall_seconds':time.monotonic()-t,'source_sha256':hashlib.sha256((r/'status_reader.rs').read_bytes()).hexdigest(),'config_sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'import_sha256':{'../verus/history.rs':hashlib.sha256((r/'../verus/history.rs').read_bytes()).hexdigest()},'native_observations_path':str(n),'native_observations_sha256':hashlib.sha256(n.read_bytes()).hexdigest(),'native_actual_image':native['actual_container_image_id'],'control_config_snapshot':str(old),'control_inputs_unchanged':True,'control_inputs_sha256':hashlib.sha256(json.dumps(subset(cfg),sort_keys=True).encode()).hexdigest(),'replay_grouped':True,'wrapper':'public model module plus use model::*','replay_fields':len(cfg['replay']),'native_qualifications':len(cfg['native_expected'])}
(out/'replay-receipt.json').write_text(json.dumps(res,indent=2)+'\n');print(json.dumps(res,indent=2));print((out/'replay.log').read_text());assert q.returncode==0
