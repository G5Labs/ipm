#!/usr/bin/env python3
"""Verify thin contracts, execute their decisions against held-out native receipts, kill mutants."""
from pathlib import Path
import hashlib,json,re,subprocess,tempfile,datetime,os
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
MODEL=HERE/'thin_surfaces.rs'
VERUS=Path(os.environ.get('VERUS',str(Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))
EXT=ROOT/'.g5/uplift/consolidated/extension'
LOGS=[EXT/'independent-native-final/raw.log',EXT/'independent-native-lockfailure/raw.log',EXT/'independent-native-inspection-case/raw.log']

def run(cmd):
 p=subprocess.run(list(map(str,cmd)),text=True,capture_output=True,timeout=120)
 return {'command':list(map(str,cmd)),'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
def rows(text,prefix):
 out={}
 for line in text.splitlines():
  if not line.startswith(prefix+' '):continue
  parts=line.split();case=parts[1]
  for field in parts[2:]:
   if '=' not in field:continue
   k,v=field.split('=',1)
   if v in ['0','1']:out[(case,k)]=int(v)
 return out

def main():
 manifest=HERE/'thin_surfaces.manifest.json'
 bindings=json.loads(manifest.read_text())['bindings']
 inputs=[MODEL,Path(__file__),manifest,HERE/'thin_surfaces.TRUST.md',*LOGS]
 inputs.extend(ROOT/b['source']['path'] for b in bindings)
 for logfile in LOGS:
  inputs.append(logfile.parent/'receipt.json')
  inputs.extend(p for p in (logfile.parent/'inputs').rglob('*') if p.is_file())
 results={'product':'571e04a1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'trust':'Translated decisions and explicit filesystem/HTTP inputs; no ObjectScript-body proof','inputs_sha256':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs},'tool_sha256':hashlib.sha256(VERUS.read_bytes()).hexdigest(),'tool_inputs_sha256':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [VERUS,VERUS.parent/'vstd.vir',VERUS.parent/'z3',VERUS.parent/'version.json']}}
 with tempfile.TemporaryDirectory(prefix='g5-thin-') as temp:
  t=Path(temp); binary=t/'thin'
  proof=run([VERUS,MODEL,'--compile','--triggers-mode','silent','-o',binary]);results['proof']=proof
  assert proof['exit']==0,proof
  execution=run([binary]);results['execution']=execution
  assert execution['exit']==0,execution
  expected=rows(execution['stdout'],'FORMAL');observed={}
  for logfile in LOGS:
   text=logfile.read_text();assert 'NATIVE-END' in text
   observed.update(rows(text,'OBS'))
  comparisons=[{'case':c,'field':f,'formal':v,'native':observed.get((c,f)),'matches':observed.get((c,f))==v} for (c,f),v in expected.items()]
  results['native_comparisons']=comparisons
  assert all(r['matches'] for r in comparisons),comparisons
  source=MODEL.read_text()
  mutations={
   'ignore-file-admission-error':('if !mkdir_ok || !source_exists { (false,target_before) }','if !mkdir_ok || !source_exists { (true,target_before) }'),
   'erase-deferred-before':('Phase::Activate => !defer && target_set','Phase::Activate => target_set'),
   'clear-protected-directory':('{ !overlay && !protected }','{ !overlay }'),
   'erase-keep-on-clean':('Phase::Clean => !keep && (as_file || !protected)','Phase::Clean => as_file || !protected'),
   '401-without-challenge-available':('{ transport_ok && ((200 <= status && status < 300) || (status == 401 && www_authenticate)) }','{ transport_ok && ((200 <= status && status < 300) || status == 401) }'),
   'propagate-lock-write-failure':('{ dependencies_ok }','{ dependencies_ok && (!requested || lock_write_ok) }'),
   'erase-external-name-lookup':('{ name_found || external_found }','{ name_found }'),
   'erase-summary-failure':('{ (!requested || (extension_supported && export_ok)) && summary_class_exists && summary_ok && (!fatal || tests_ok) }','{ (!requested || (extension_supported && export_ok)) && summary_ok && (!fatal || tests_ok) }'),
  }
  results['controls']=[]
  for name,(before,after) in mutations.items():
   assert source.count(before)==1,(name,source.count(before))
   mutated=t/(name+'.rs');mutated.write_text(source.replace(before,after,1))
   r=run([VERUS,mutated,'--triggers-mode','silent'])
   rejected=r['exit']!=0 and 'postcondition not satisfied' in r['stderr']
   native_control=run([VERUS,mutated,'--no-verify','--compile','--triggers-mode','silent','-o',t/'mutant'])
   assert native_control['exit']==0,(name,native_control)
   mutant_execution=run([t/'mutant'])
   mutant_rows=rows(mutant_execution['stdout'],'FORMAL')
   native_differences=[{'case':c,'field':f,'wrong':v,'native':observed.get((c,f))} for (c,f),v in mutant_rows.items() if observed.get((c,f))!=v]
   results['controls'].append({'id':name,'rejected':rejected,'native_rejected':bool(native_differences),'native_differences':native_differences,'control_compile':native_control,'control_execution':mutant_execution,**r})
   assert rejected,(name,r)
 results['summary']={'verified':int(re.search(r'(\d+) verified, 0 errors',proof['stdout']).group(1)),'native_fields':len(comparisons),'native_agree':sum(r['matches'] for r in comparisons),'negative_controls':len(results['controls']),'controls_rejected':sum(r['rejected'] for r in results['controls']),'native_controls_rejected':sum(r['native_rejected'] for r in results['controls'])}
 out=HERE/'results'/'thin_surfaces.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results['summary']))
if __name__=='__main__':main()
