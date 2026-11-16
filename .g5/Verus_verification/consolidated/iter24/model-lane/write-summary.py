from pathlib import Path
import json,hashlib
r=Path('.g5/Verus_verification/consolidated/iter24');o=r/'model-lane'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
p=json.loads((o/'positive-receipt.json').read_text());c=json.loads((o/'isolated-controls/receipt.json').read_text());q=json.loads((o/'replay-receipt.json').read_text());old=json.loads((o/'attempt-control-weaker-exact/receipt.json').read_text())
assert c['complete'] and all(x['accepted'] and not x['resource_limit'] for x in c['runs']) and q['exit']==0
assert sha(r/'snapshot_dependencies.rs')==p['source_sha256']==c['model_sha256']==q['source_sha256']
assert sha(r/'snapshot-dependency-cases.json')==q['config_sha256']
d={
 'source_sha256':sha(r/'snapshot_dependencies.rs'),
 'final_config_sha256':sha(r/'snapshot-dependency-cases.json'),
 'isolation_helper_sha256':sha(o/'isolate_witness.py'),
 'review_sha256':sha(o/'model-review.md'),
 'binding_proposal_sha256':sha(o/'bindings-proposal.json'),
 'baseline_verified':50,'inherited_verified':37,'new_verified':13,
 'new_unit_scope':'12 proof functions plus1 recursive consume check; no unused verification checks; spec helpers not member coverage',
 'focused_good':8,'focused_controls':30,
 'full_positive_receipt':'positive-receipt.json',
 'isolated_control_receipt':'isolated-controls/receipt.json',
 'control_config_snapshot':'isolated-controls/config.json',
 'control_inputs_unchanged':q['control_inputs_unchanged'],
 'control_inputs_sha256':q['control_inputs_sha256'],
 'suite_wall_seconds':round(sum(x['wall_seconds'] for x in p['runs'])+sum(x['wall_seconds'] for x in c['runs']),3),
 'uncredited_prior_solver_attempts':{
  'draft1':{'exact_duration_recorded':False,'source':'attempt-1.rs','receipt':'attempt-1.json','reason':'Wrong request producer and weak release-exact witness; computation rejected'},
  'draft2':{'exact_duration_recorded':False,'source':'attempt-2.rs','receipt':'attempt-2.json','exit':130,'reason':'Manual stop of named-World computation, no result; observed1:50 before later stop, unknown precise duration not zero'},
  'earlier_staged_passes':{'wall_seconds':sum(json.loads((o/f'baseline-{i}.json').read_text())['wall_seconds'] for i in [3,4]),'credited_as_final_gate':False},
  'weak_exact_control':{'wall_seconds':sum(x['wall_seconds'] for x in old['runs']),'receipt':'attempt-control-weaker-exact/receipt.json','credited_as_final_gate':False,'reason':'Mutant passed1/0; relevant release-vs-snapshot exact membership assertion added before final gate'}
 },
 'replay_grouped':True,'replay_fields':120,'native_qualifications':63,
 'replay_verified':51,'replay_errors':0,'replay_wall_seconds':q['wall_seconds'],
 'replay_support':q['support'],
 'native_observations_path':q['native_observations_path'],
 'native_observations_sha256':q['native_observations_sha256'],
 'native_actual_image':q['native_actual_image'],'inherited_sha256':q['inherited_sha256'],
 'implementation_proofs':0,
 'guidance_sha256':'8442346f9c3e58a33a5a1aea90a8c40c45bf71683858b6999b3a144f509c581d',
 'prediction_miss':'Frozen default-refresh prediction refuted; actual default NoOverlap derives from inherited intersection; permit reaches2',
 'provider_boundary':'Stable manifest/version only; directory equality applicability not production guard; C:D32 unchanged',
 'evaluation_independence':'Builder-selected public endpoint; no prospective/private held-out discovery claimed',
 'excluded_boundary':'UNASSESSED/UNPURSUED; incidental locator line discarded, no investigation/probe/model/reproduction',
 'metadata_attempt_note':'Initial inline summary writer had unmatched-brace SyntaxError before file mutation; no model/config/proof change.',
 'frozen':True
}
(o/'final-summary.json').write_text(json.dumps(d,indent=2)+'\n')
print(json.dumps({k:d[k] for k in ['source_sha256','final_config_sha256','review_sha256','baseline_verified','inherited_verified','new_verified','focused_good','focused_controls','replay_verified','replay_fields','native_qualifications']},indent=2))
print('summary_sha256',sha(o/'final-summary.json'))
