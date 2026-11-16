from pathlib import Path
import json,csv,hashlib
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];VV=ROOT/'.g5/Verus_verification/consolidated';V=VV/'verus'
r=json.loads((V/'results/thin_surfaces.json').read_text());assert all(c['matches'] for c in r['native_comparisons'])
for path,digest in r['inputs_sha256'].items():assert hashlib.sha256((ROOT/path).read_bytes()).hexdigest()==digest,('stale input',path)
manifest=json.loads((V/'thin_surfaces.manifest.json').read_text());bindings={b['member']:b for b in manifest['bindings']}
controls={c['id']:c for c in r['controls']}
pairs=[('ResourceProcessor/FileCopy.cls::OnBeforePhase','erase-deferred-before'),('ResourceProcessor/FileCopy.cls::DoCopy','ignore-file-admission-error'),('ResourceProcessor/FileCopy.cls::OnPhase','erase-keep-on-clean'),('Repo/Oras/PackageService.cls::IsAvailable','401-without-challenge-available'),('Test/Manager.cls::ReportTestResults','erase-summary-failure'),('General/ModuleSummary.cls::Show','erase-external-name-lookup')]
rows=[]
for member,control in pairs:
 # Resolve owning member names from native manifest; never credit a caller for a callee's mutation.
 if member not in bindings:
  candidates=[b for b in bindings if b.split('::')[1]==member.split('::')[1]];assert len(candidates)==1,(member,candidates);member=candidates[0]
 b=bindings[member];c=controls[control];assert c['rejected'] and c['native_rejected']
 rows.append({'member':member,'family':'thin_surfaces','round':'E5','kind':'Verus-native','slice':b['slice'],'model_ref':b['artifact']+'::'+b['symbol'],'behavior':b['property'],'check_receipt':'verus/results/thin_surfaces.json','verdict':'pass','control':control})
f=HERE.parent/'coverage/register-extra.tsv'
with f.open('w') as out:
 w=csv.DictWriter(out,fieldnames=list(rows[0]),delimiter='\t',lineterminator='\n');w.writeheader();w.writerows(rows)
(HERE/'old-rule-additions.json').write_text(json.dumps(rows,indent=2));print(len(rows),'qualified thin native-controlled rows; union deduplicated by count.py')
