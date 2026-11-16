from pathlib import Path
import json,hashlib
r=Path('.g5/Verus_verification/consolidated/iter26');o=r/'model-lane';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();p=json.loads((o/'positive-receipt.json').read_text());c=json.loads((o/'isolated-controls/receipt.json').read_text());q=json.loads((o/'replay-receipt.json').read_text())
assert c['complete'] and all(x['accepted'] and not x['resource_limit'] for x in c['runs']) and q['exit']==0
assert sha(r/'sync_summary.rs')==p['source_sha256']==c['model_sha256']==q['source_sha256'];assert sha(r/'sync-summary-cases.json')==q['config_sha256']
prior={}
for i in range(1,4):
 a=json.loads((o/f'baseline-{i}.json').read_text());prior[f'baseline-{i}']={'wall_seconds':a['wall_seconds'],'exit':a.get('exit'),'source_sha256':a['source_sha256'],'receipt':f'baseline-{i}.json','log':f'baseline-{i}.log','credited_as_final_gate':False}
prior['baseline-1']['reason']='13/1 staged map equality facts missing; exact source retained attempt-1.rs'
prior['baseline-2']['reason']='15/0 earlier pass before independent selected-owner witness'
prior['baseline-3']['reason']='16/0 earlier pass before used verbose callsite helper; final recorded candidate unchanged in verification-unit count'
d={
 'source_sha256':sha(r/'sync_summary.rs'),'final_config_sha256':sha(r/'sync-summary-cases.json'),
 'isolation_helper_sha256':sha(o/'isolate_witness.py'),'review_sha256':sha(o/'model-review.md'),'binding_proposal_sha256':sha(o/'bindings-proposal.json'),
 'baseline_verified':16,'inherited_verified':0,'new_verified':16,'new_unit_scope':'12 proof functions plus4 recursive checks; zero imports/unused algorithm verification units; not a member coverage count',
 'focused_good':8,'focused_controls':28,'full_positive_receipt':'positive-receipt.json','isolated_control_receipt':'isolated-controls/receipt.json','control_config_snapshot':'isolated-controls/config.json','control_inputs_unchanged':q['control_inputs_unchanged'],'control_inputs_sha256':q['control_inputs_sha256'],
 'suite_wall_seconds':round(sum(x['wall_seconds'] for x in p['runs'])+sum(x['wall_seconds'] for x in c['runs']),3),'uncredited_prior_solver_attempts':prior,
 'parent_initial_replay':'Parent-owned first grouped wrapper lacked two row-length facts,16/1. Exact configuration/wrapper/log preserved; public recursion fuel added, no rule/native/domain/limit change. Excluded from lane cost.',
 'parent_focused_gate':'Parent reported three same-source proved-closed-alias focused positives and semantic opposite rejections; owned by parent gate, not included in lane solver count.',
 'replay_grouped':True,'replay_fields':126,'native_qualifications':22,'replay_verified':17,'replay_errors':0,'replay_wall_seconds':q['wall_seconds'],'replay_support':q['support'],'replay_wrapper':q['wrapper'],
 'native_observations_path':q['native_observations_path'],'native_observations_sha256':q['native_observations_sha256'],'native_actual_image':q['native_actual_image'],
 'implementation_proofs':0,'native_jobs_by_lane':0,'guidance_sha256':'8442346f9c3e58a33a5a1aea90a8c40c45bf71683858b6999b3a144f509c581d',
 'exposure':'Explicitly disclosed closed22-to24 repair; only authorized REPORT.md read, no linked/private files or private IRIS sample. No independent/unlinked-gain/convergence claim.',
 'store_scope':'One process-private job store, explicit nonoverflow numeric per-module counters/keys, generated-prefix wf; no SQL/durability/opaqueID proof.',
 'provider_boundaries':['module_count direct skipped-only stores, not CLI total buckets','skipped-row report projection excludes other fields/header/format branches','one selected resource/class group; general assignment/fallback/IsA/callback/status mechanics unassessed','CLI compilemarkers/status/testcounter0/factory/header remain native qualifications','after_report frame source-inspected/native-confirmed, not mechanical body no-mutation proof'],
 'scope_miss':'Parent initial verify priming unexpectedly traversed an excluded package path; stopped, exact raw retained unassessed/uncredited, container removed. No follow-up source/probe/model and no claim that no such path executed overall.',
 'policy_disposition':'No new bug/Q/S/repair certification; existing old models remain unchanged valid interface scopes.',
 'frozen':True
}
(o/'final-summary.json').write_text(json.dumps(d,indent=2)+'\n');print(json.dumps({k:d[k] for k in ['source_sha256','final_config_sha256','review_sha256','isolation_helper_sha256','baseline_verified','focused_good','focused_controls','replay_verified','replay_fields','native_qualifications']},indent=2));print('summary_sha256',sha(o/'final-summary.json'))
