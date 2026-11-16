from pathlib import Path
import json,hashlib
r=Path('.g5/Verus_verification/consolidated/iter25');o=r/'model-lane';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();p=json.loads((o/'positive-receipt.json').read_text());c=json.loads((o/'isolated-controls/receipt.json').read_text());q=json.loads((o/'replay-receipt.json').read_text());cfg=json.loads((r/'developer-source-cases.json').read_text())
assert c['complete'] and all(x['accepted'] and not x['resource_limit'] for x in c['runs']) and q['exit']==0
assert sha(r/'developer_sources.rs')==p['source_sha256']==c['model_sha256']==q['source_sha256'];assert sha(r/'developer-source-cases.json')==q['config_sha256']
prior={}
for i in range(1,6):
 a=json.loads((o/f'baseline-{i}.json').read_text());prior[f'baseline-{i}']={'wall_seconds':a['wall_seconds'],'exit':a.get('exit'),'source_sha256':a['source_sha256'],'receipt':f'baseline-{i}.json','log':f'baseline-{i}.log','credited_as_final_gate':False}
prior['baseline-1']['reason']='Struct-literal parentheses compile error'
prior['baseline-2']['reason']='Incomplete explicit constructor proof facts,8/1 postconditions'
prior['baseline-3']['reason']='Passing9 before stored-resource argument reader correction'
prior['baseline-4']['reason']='Passing10 before independent marker/guard/accessor witnesses'
prior['baseline-5']['reason']='Unneeded last-dot unfolding in staged proof hit rlimit13/1; uncredited, removed unfolding without rule changes'
d={
 'source_sha256':sha(r/'developer_sources.rs'),'final_config_sha256':sha(r/'developer-source-cases.json'),
 'isolation_helper_sha256':sha(o/'isolate_witness.py'),'review_sha256':sha(o/'model-review.md'),'binding_proposal_sha256':sha(o/'bindings-proposal.json'),
 'baseline_verified':14,'inherited_verified':0,'new_verified':14,
 'new_unit_scope':'11 proof functions plus3 recursive string-helper checks; no imports/unused verification units; not a product-member coverage count',
 'focused_good':10,'focused_controls':35,'full_positive_receipt':'positive-receipt.json','isolated_control_receipt':'isolated-controls/receipt.json',
 'control_config_snapshot':'isolated-controls/config.json','control_inputs_unchanged':q['control_inputs_unchanged'],'control_inputs_sha256':q['control_inputs_sha256'],
 'suite_wall_seconds':round(sum(x['wall_seconds'] for x in p['runs'])+sum(x['wall_seconds'] for x in c['runs']),3),
 'uncredited_prior_solver_attempts':prior,
 'non_solver_attempt':'Draft build-source-attempt-1.py had f-string braces error before source creation; subsequent FileNotFound did not launch Verus. Preserved, no proof credit.',
 'parent_initial_replay':'Parent-owned initial wrapper omitted later per-field support and hit rlimit14/1; retained under uplift/consolidated/iter25/replay, excluded from lane cost to avoid double count.',
 'replay_grouped':True,'replay_fields':132,'native_qualifications':70,'replay_verified':15,'replay_errors':0,'replay_wall_seconds':q['wall_seconds'],'replay_support':q['support'],'replay_wrapper':q['wrapper'],
 'native_observations_path':q['native_observations_path'],'native_observations_sha256':q['native_observations_sha256'],'native_actual_image':q['native_actual_image'],
 'implementation_proofs':0,'native_jobs_by_lane':0,'guidance_sha256':'8442346f9c3e58a33a5a1aea90a8c40c45bf71683858b6999b3a144f509c581d',
 'important_corrections':['%Reload writer attribution, not CheckBeforeClean','actual stored ref.Name reader input, not fixture sidecar','literal8 prefix and map-target oracle independent of writer helper','shared global NoFolders carried A to B','cached/new processor metadata equivalence qualified; no allocation claim'],
 'provider_boundaries':['only A missing-cls/B existing-code directory applicability','file_exists static fixture membership only','Class default filename extension and literal normalization only','actual module/resource/factory availability, supplier/code/payload/content remain qualification','external modern Sources consumer, trigger queue, installer/Invoke flags and generic metadata/cache/filesystem mechanics unassessed'],
 'policy_disposition':'No new bug/Q/S/repair; SDK legacy reader hypotheses refuted, no modern consumer or general path-resolution claim.',
 'evaluation_independence':'Builder-selected public25 flow; no independent private-case discovery, convergence or unlinked-complement gain claimed; private IRIS sample not accessed.',
 'excluded_boundary':'UNASSESSED/UNPURSUED; incidental unrelated header display discarded without follow-up/model/probe/reproduction.',
 'frozen':True
}
(o/'final-summary.json').write_text(json.dumps(d,indent=2)+'\n');print(json.dumps({k:d[k] for k in ['source_sha256','final_config_sha256','review_sha256','isolation_helper_sha256','baseline_verified','focused_good','focused_controls','replay_verified','replay_fields','native_qualifications']},indent=2));print('summary_sha256',sha(o/'final-summary.json'))
