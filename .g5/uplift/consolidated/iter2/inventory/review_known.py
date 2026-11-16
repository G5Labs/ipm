#!/usr/bin/env python3
"""Apply scoped reviewer deltas to immutable known-panel I grades; never edit graders."""
import collections
import copy
import hashlib
import json
import re
from pathlib import Path
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[4]
BASE=REPO.parent/'results/experiments/uplift-measurement-20261114/score-audit-v2.json'
LEDGER=REPO.parent/'results/experiments/uplift-measurement-20261114/expanded-case-ledger.json'
root=json.loads(BASE.read_text())
original=root['systems']['ipm']
PIN='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def block(file,ident):
    p=REPO/'.g5/ontology'/file
    s=p.read_text(); match=re.search(r'<a id="'+re.escape(ident)+r'"></a>\n',s); assert match
    end=s.find('\n<a id=',match.end()); meta=s.find('\n## Metadata',match.end())
    endpoints=[e for e in [end,meta] if e>=0]
    text=s[match.end():min(endpoints) if endpoints else len(s)].strip()
    return {'path':str(p.relative_to(REPO)),'id':ident,'text':text,'sha256':hashlib.sha256(text.encode()).hexdigest()}

owners={
 'L8':block('features/history.g5.md','history.survives-rollback.temp-copy'),
 'L12':block('features/history.g5.md','history.delete.running-records'),
 'HELD-FILE-ROLLBACK':block('concepts/modules.g5.md','module.processor.file-copy'),
 'HELD-CPF-ROLLBACK':block('concepts/modules.g5.md','module.processor.cpf-effects'),
 'HELD-INSPECTION-NAME':block('features/inspection.g5.md','inspect'),
 'X-ATTR-CYCLE':block('concepts/modules.g5.md','module.parameter.cycles'),
 'OW-CONSTRUCTOR':block('concepts/repositories.g5.md','repo.filesystem'),
 'B9':block('features/build-and-publish.g5.md','build.test-output.counts'),
}
boundary='.g5/uplift/consolidated/iter2/boundaries/'
native=boundary+'native-replay-20261114T173805Z/receipt.json'
reasons={
 ('ontology','coverage','L8'):(2,'Canonical owner now distinguishes abrupt SIGKILL from whole-instance restart: temporary progress disappears only at restart and the durable initial permanent row remains unfinished. IRISTEMP mapping and durable DB conditions are in the rule. One provider/configuration bounds the observation.',[native,boundary+'native-replay-20261114T173805Z/processloss.log',boundary+'native-replay-20261114T173805Z/restart.log']),
 ('ontology','coverage','HELD-FILE-ROLLBACK'):(2,'Canonical FileCopy owner now states both successful-copy survival through DB rollback and missing-source error/no copied target. Directory creation and arbitrary partial OS failure remain separate; intended rule not weakened. Retained native pair supports this text.', ['.g5/uplift/consolidated/extension/independent-native-final/raw.log','.g5/Verus_verification/consolidated/verus/thin_surfaces.rs']),
 ('ontology','precision','HELD-FILE-ROLLBACK'):('S','Complete scheduled answer now given in owning text: successful copied file survives journaled sentinel rollback; missing-source call fails and creates no copied target. No new universal filesystem claim.', ['.g5/uplift/consolidated/extension/independent-native-final/raw.log']),
 ('ontology','coverage','HELD-CPF-ROLLBACK'):(2,'Canonical CPF effects owner separates child exit from applied configuration, records valid role survival after DB rollback and invalid-section refusal/no role, and keeps other actions/partial/restart effects explicit.',[native,boundary+'native-replay-20261114T173805Z/cpf.log']),
 ('ontology','precision','HELD-CPF-ROLLBACK'):('S','Scheduled provider-limited answer is supported by actual final role reader, not exit status alone. Valid role merge survives DB rollback; invalid section exits7 and creates no role. Owner compensation policy remains unratified.',[native,boundary+'native-replay-20261114T173805Z/cpf.log']),
 ('model','coverage','HELD-CPF-ROLLBACK'):(2,'CPF state records DB sentinel, child status and independently supplied configuration role. rollback_cpf changes the DB sentinel only. Both real provider outcomes instantiate the model, with success/refusal/rollback assertions and rejected compensation control. This completes the scheduled contrast under stated external premise, not arbitrary CPF semantics.',[boundary+'proofs/receipt.json','.g5/Verus_verification/consolidated/iter2/boundaries/boundaries.rs',native]),
 ('model','precision','HELD-CPF-ROLLBACK'):('S','cpf_status_does_not_establish_actual_configuration and cpf_native_valid_invalid_witnesses answer child status versus actual configuration separately; real role readback validates the two supplied outcomes. Arbitrary CPF outcome is still an input, and no implementation proof is claimed.',[boundary+'proofs/receipt.json',native]),
 ('ontology','coverage','HELD-INSPECTION-NAME'):(2,'inspect now states stored Name first, ExternalName second, Name precedence and installed-module/namespace error when neither matches; case-normalization premise remains explicit. Canonical text supplies formerly missing conditions.', ['.g5/uplift/consolidated/extension/independent-native-inspection/raw.log','.g5/Verus_verification/consolidated/verus/thin_surfaces.rs']),
 ('ontology','precision','HELD-INSPECTION-NAME'):('S','Owner text predicts existing name and valid alias select the same installed record/version, while missing name throws. Retained native name/alias/missing neighbors and source Show support the answer.', ['.g5/uplift/consolidated/extension/independent-native-inspection/raw.log']),
 ('ontology','coverage','X-ATTR-CYCLE'):(2,'module.parameter.cycles now places the intended named-cycle error beside its as-built unreachable guard and unresolved cyclic output, plus resolving-chain neighbor. It identifies source-recovered authority rather than an owner decision.', ['.g5/uplift/consolidated/iter2/native/receipt.json','.g5/Verus_verification/consolidated/verus/attr_evaluator.rs']),
}
checks={
 ('model','coverage','L12'):'Retain partial=1. The new model has real row/twin state and two fixed native-validated schedule orders, but lock blocking/timeouts and scheduler composition are not modeled. save_after_delete/replay_on_close are outcome-table ports. More source or native evidence alone does not earn complete concurrent formal representation.',
 ('ontology','coverage','L12'):'Retain full=2. Canonical owner now gives the accessible-versus-locked schedule distinction and limits; corrects the broad no-resurrection interpretation without losing provisional policy alternatives.',
 ('ontology','precision','L12'):'Retain S. The fixed question asks whether permitted delete always wins; owner explicitly says it does not, and provides the final same-ID resurrection neighbor. This is an improved supported answer, not an extra scheduled question.',
 ('model','coverage','L8'):'Retain full=2 from imported foundation. New process/restart model and disposable observations strengthen correspondence but do not create an additional case or duplicate member credit.',
 ('model','precision','L8'):'Retain S: dedicated instance-restart semantics remain represented under durable DB/IRISTEMP premises. Arbitrary crash durability and replication stay open.',
 ('ontology','precision','X-ATTR-CYCLE'):'Retain S: existing narrower as-built prediction was supported; the gain is canonical ownership coverage, not a second precision gain.',
 ('model','coverage','X-ATTR-CYCLE'):'Retain2: inherited resolve_bounded refutation and resolving neighbor preserved; native rerun adds correspondence without new model coverage.',
 ('model','precision','X-ATTR-CYCLE'):'Retain S: unresolved cyclic output rather than promised named error remains the supported as-built answer.',
 ('model','coverage','B9'):'Retain partial=1. New JSON tree/device modes and method/case accounting are consequential gains, but all summary formats, file-extension selection/serialization and verify namespace/error lifecycle are not represented. Do not infer full original reporting case from JSON subset.',
 ('model','coverage','B11'):'Retain partial=1. New show dependents and requirements readers do not model ListInstalled tree/Python/sort options that own B11.',
 ('model','coverage','L13'):'Retain partial=1. One actual CPF action supplements file witnesses, but the broad outside-IRIS effect universe (pip/services/other settings) and composition remain open.',
}
rows=[]
for key,r in original.items():
    before=r['grades']['I']; tuplekey=(r['layer'],r['measure'],r['id']); change=reasons.get(tuplekey)
    after,reason,ev=change if change else (before,checks.get(tuplekey,'Frozen I grade preserved. This narrow delta review did not reopen this case; no new validation is implied.'),[])
    entry={'key':key,'id':r['id'],'layer':r['layer'],'measure':r['measure'],'stratum':r['stratum'],'before':before,'after':after,'changed':before!=after,'reason':reason,'evidence':ev,'review_scope':'changed-case-adjudication' if change else 'challenged-credit-recheck' if tuplekey in checks else 'unchanged-inherited-grade'}
    if r['id'] in owners and (change or tuplekey in checks): entry['canonical_owner']=owners[r['id']]
    rows.append(entry)

summary={}
for layer in ['ontology','model']:
    coverage=[r for r in rows if r['layer']==layer and r['measure']=='coverage']
    precision=[r for r in rows if r['layer']==layer and r['measure']=='precision']
    assert len(coverage)==41 and len(precision)==26
    def precision_counts(field):
        counts=collections.Counter(r[field] for r in precision)
        given=sum(counts.get(x,0) for x in ['S','P','W','E'])
        return {'statuses':{s:counts.get(s,0) for s in ['S','P','W','E','U','Q']},'supported':counts['S'],'all_scheduled':26,'answers_given':given,'supported_per_answer_given':counts['S']/given if given else None,'supported_per_all_scheduled':counts['S']/26,'human_readability':'unmeasured'}
    summary[layer]={'coverage':{'scheduled':41,'max_points':82,'before_points':sum(r['before'] for r in coverage),'after_points':sum(r['after'] for r in coverage),'before_distribution':{str(s):sum(r['before']==s for r in coverage) for s in [0,1,2]},'after_distribution':{str(s):sum(r['after']==s for r in coverage) for s in [0,1,2]},'unclassified':0,'gains':[r['id'] for r in coverage if r['after']>r['before']],'withdrawals':[r['id'] for r in coverage if r['after']<r['before']]},'precision':{'before':precision_counts('before'),'after':precision_counts('after'),'gains':[r['id'] for r in precision if r['before']!='S' and r['after']=='S'],'withdrawals':[r['id'] for r in precision if r['before']=='S' and r['after']!='S']}}

repair_queue=[{'id':'OW-CONSTRUCTOR','in_fixed_panel':False,'disposition':'property-correspondence-withdrawal-and-corrected-contract','reason':'Old owners.rs successful_refresh_holds_outer_lock remains true only of incorrect source abstraction. Actual default BuildCache releases explicit lock at method return even with outer transaction; explicit New(0) retains until commit, New(1) releases at close. No complete-member loss because other BuildCache properties remain supported.','canonical_owner':owners['OW-CONSTRUCTOR'],'evidence':[boundary+'withdrawals.json',boundary+'proofs/receipt.json',native]}, {'id':'L12','in_fixed_panel':True,'disposition':'generalized-no-resurrection-claim-withdrawn','reason':'Locked schedule re-creates original same ID finalized with successful result after deleting then closing. Accessible schedule still leaves absence. Frozen known-panel grades do not encode the withdrawn universal claim; do not invent a duplicate score unit.','evidence':[boundary+'withdrawals.json',boundary+'native/delete-final-reader.log'] }]
inputs={str(BASE):sha(BASE),str(LEDGER):sha(LEDGER)}
for p in sorted((REPO/'.g5/ontology').rglob('*.md')): inputs[str(p.relative_to(REPO))]=sha(p)
for rel in [native,boundary+'native-replay-20261114T173805Z/cpf.log',boundary+'proofs/receipt.json',boundary+'withdrawals.json','.g5/uplift/consolidated/iter2/native/receipt.json','.g5/Verus_verification/consolidated/iter2/boundaries/boundaries.rs','.g5/Verus_verification/consolidated/iter2/readers.rs']:
    inputs[rel]=sha(REPO/rel)
for entry in rows:
    for rel in entry['evidence']:
        inputs[rel]=sha(REPO/rel)
output={'panel':'known IPM frozen41coverage+26precision per layer','pin':PIN,'method':'Independent scoped reviewer delta adjudication, unblinded known repair closure. Same source evaluator previously supplied source-first repair leads; no fresh evaluator panel access. Preserves original scoring/rules and all unchallenged I grades. This is not fresh-case generalization or blind performance.','grade_rules':{'coverage':{'2':'full','1':'partial','0':'absent'},'precision':root['rules']},'score_baseline':'actual iter1 I grades from immutable score-audit-v2.json; artifact snapshot preserved by parent','summary':summary,'cases':rows,'repair_queue_outside_or_aliased_panel':repair_queue,'duplicate_policy':'Restart counted only as L8, delete/finalize only as L12, OW-CONSTRUCTOR outside denominator. X-ATTR-CYCLE ontology ownership gain does not duplicate already-supported precision. CPF native readback applies once per existing CPF case.','remaining_gaps':['Model L12 remains partial: no complete lock/scheduler/composition semantics.','Model L13 remains partial across outside-DB effects.','Model B9 remains partial for all formats/extensions and verify lifecycle.','Model B11 remains partial for ListInstalled options.','Ontology L5,L6,B2,B3,B12 partial commitments remain unchanged.','No human priorities, policy ratification or human readability measurements.'],'inputs_sha256':inputs,'implementation_proof':False,'cost':'unknown','reviewer_tokens':'unknown'}
(HERE.parent/'known-panel-review.json').write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps(summary,indent=2))
