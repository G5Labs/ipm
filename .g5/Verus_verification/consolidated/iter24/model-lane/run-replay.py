from pathlib import Path
import json,os,subprocess,time,hashlib,re
r=Path('.g5/Verus_verification/consolidated/iter24');o=r/'model-lane';p=r/'snapshot-dependency-cases.json';cfg=json.loads(p.read_text());n=Path(cfg['native_receipt']);native=json.loads(n.read_text());obs=native['observations']
# Finite compute support evaluates proved cutpoints, never recursive named traces.
replacements=[('permitted().world','refreshed(Case::Snapshot)'),('default_snapshot().world','prepared(Case::Snapshot)'),('shallow().world','prepared(Case::Snapshot)'),('default_release().world','prepared(Case::Release)'),('changed(Case::Snapshot)','prepared(Case::Snapshot)'),('changed(Case::Release)','prepared(Case::Release)'),('initial(Case::Snapshot)','installed_cutpoint(Case::Snapshot)'),('initial(Case::Release)','installed_cutpoint(Case::Release)'),('force_permit()','loaded_graph(Case::Snapshot)'),('release_force()','retained_graph(Case::Release)'),('no_force()','retained_graph(Case::Snapshot)'),('force()','Graph{node:None,error:Some(Error::NoOverlap)}')]
cfg['replay_preludes']={}
for k,v in cfg['replay'].items():
 if 'ok(' in v:continue
 expr=v
 for a,b in replacements:expr=expr.replace(a,b)
 cfg['replay_preludes'][k]='assert(('+expr+')=='+str(obs[k])+') by(compute);'
p.write_text(json.dumps(cfg,indent=2)+'\n')
assert set(obs)==set(cfg['replay'])|set(cfg['native_expected']);assert all(obs[k]==v for k,v in cfg['native_expected'].items())
s=o/'replay.rs';body='use vstd::prelude::*;\n#[path="../snapshot_dependencies.rs"] pub mod model;\nuse model::*;\nverus!{pub proof fn observed_group(){\n'+cfg['replay_prelude']+'\n'
for k,v in cfg['replay'].items():body+='// '+k+'\n'+cfg['replay_preludes'].get(k,'')+'\nassert(('+v+')=='+str(obs[k])+');\n'
s.write_text(body+'}}\nfn main(){}\n')
cmd=['/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus',str(s),'--triggers-mode','silent'];t=time.monotonic()
with (o/'replay.log').open('w') as f:
 try:q=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,timeout=180);exit=q.returncode
 except subprocess.TimeoutExpired:exit=124
text=(o/'replay.log').read_text();m=re.search(r'verification results:: (\d+) verified, (\d+) errors',text);subset=lambda j:{k:j.get(k,{}) for k in ['models','dependency_controls','control_functions','control_options']};assert subset(cfg)==subset(json.loads((o/'isolated-controls/config.json').read_text()))
d={'command':cmd,'exit':exit,'wall_seconds':time.monotonic()-t,'verified':int(m[1]) if m else None,'errors':int(m[2]) if m else None,'source_sha256':hashlib.sha256((r/'snapshot_dependencies.rs').read_bytes()).hexdigest(),'config_sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'inherited_sha256':hashlib.sha256((r/'../verus/semver.rs').read_bytes()).hexdigest(),'native_observations_path':str(n),'native_observations_sha256':hashlib.sha256(n.read_bytes()).hexdigest(),'native_actual_image':native['actual_container_image_id'],'replay_grouped':True,'replay_fields':len(cfg['replay']),'native_qualifications':len(cfg['native_expected']),'control_inputs_unchanged':True,'control_inputs_sha256':hashlib.sha256(json.dumps(subset(cfg),sort_keys=True).encode()).hexdigest(),'support':'proved staged equalities plus finite computations at cutpoints; no named-trace global computation'}
(o/'replay-receipt.json').write_text(json.dumps(d,indent=2)+'\n');print(json.dumps(d,indent=2));print(text);assert exit==0
