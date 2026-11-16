from pathlib import Path
import csv,re,json,hashlib
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];ONT=ROOT/'.g5/ontology'
# Answers below are reconstructed only from named canonical block bodies; model results are not used.
mapping={
'L2':(['history.dependencies.parent-rollback','history.survives-rollback'],'The child keeps its own Success meaning. Its data may roll back and the permanent row may disappear as built, violating the surviving-record requirement.'),
'L3':(['history.committed.nested'],'Loss of caller-held transaction data/history after a later caller rollback is a product fact; independent audit retention is still an unapproved policy choice with explicit alternatives.'),
'L7':(['load.outcome','load.status','load.status.after-commit'],'The normal rollback table applies before final commit. Message or cleanup failure afterwards returns error with module kept and committed history; success and brace-free macro are distinct neighbors.'),
'L12':(['history.delete.running-records'],'Delete does not have a universal win guarantee: top-level and parent-transaction-locked child rows differ. Held-out stale writer deletion stayed absent; other concurrent schedules and retention policy remain open.'),
'L13':(['history.committed','transaction.rollback-scope'],'Committed describes database writes, not filesystem rollback. Copies and lock files can remain; compensation of external effects is a separate open policy.'),
'B7':(['devtools.sync','devtools.sync.retry'],'A failed sync with an existing baseline sets SyncFailed and is retried even after exact-byte revert. A failed compile/load/reload outside sync can change server code without that mark; reverting to baseline can then hide the change (OUT premise/F-SYNC-1). Tests fail after successful sync hash commit and do not undo it.'),
'B6':(['module.resource.scope','build.test'],'Ordinary activation omits test-scoped resources; Test phase includes its resources/dependencies.'),
'L4':(['transaction.flag-sources','load.outcome'],'Each activation merges flags by key; defined parent value including zero wins. Absent parent allows child NoJournal to escape rollback; child NoTransaction leaves an open parent transaction intact. Desired policy remains a child Q.'),
'L9':(['history.read.filter','history.delete'],'Keys are walked in IRIS collation order. The first invalid/absent key stops parsing; earlier predicates remain and all later keys are ignored. An underscore-only filter can delete every seeded row.'),
'L11':(['history.read.global','history.read.api'],'CLI fetches in each namespace with namespace labels and per-namespace limit. GlobalRecordsFetch repeats caller rows and lacks namespace/ID in row schema; their correctness is distinct.')}
blocks={}
for p in ONT.rglob('*.g5.md'):
 for m in re.finditer(r'^\[[IDQ] · ([^\]]+)\]\n(.*?)(?=\n<a id=|\n## |\Z)',p.read_text(),re.M|re.S):blocks[m[1]]={'path':str(p.relative_to(ROOT)),'text':m[2].strip(),'sha256':hashlib.sha256(m[2].encode()).hexdigest()}
rows=[]
for r in csv.DictReader(open(ROOT.parent/'results/experiments/ipm-uplift-comparison-20261113/precision.tsv'),delimiter='\t'):
 if r['layer']!='Ontology':continue
 ids,answer=mapping[r['id']];rows.append({'id':r['id'],'question':r['test'],'answer':answer,'owning_blocks':{id:blocks[id] for id in ids},'status':'answerable with stated uncertainty; no model inference'})
(HERE/'precision-after.json').write_text(json.dumps(rows,indent=2));print(len(rows),'ontology precision answers from owning blocks only')
# Question register audit after all canonical child additions, keeping original registers untouched.
questions=[]
for p in ONT.rglob('*.g5.md'):
 for m in re.finditer(r'^\[([IDQ]) · ([^\]]+)\]\n(.*?)(?=\n<a id=|\n## |\Z)',p.read_text(),re.M|re.S):
  if m[1]!='Q' and m[2] not in ['history.committed.side-effects','history.dependencies.parent-rollback','transaction.rollback-scope']:continue
  body=m[3].strip();questions.append({'ontology':m[2],'path':str(p.relative_to(ROOT)),'status':'open' if m[1]=='Q' else 'settled-meaning-alias-preserved','question_or_resolution':body.splitlines()[0],'assumption':next((x for x in body.splitlines() if 'Working assumption:' in x or 'Provisional answer:' in x),None),'alternatives': [x for x in body.splitlines() if 'Alternative:' in x],'evidence':'owning block and same-ID metadata','audit_limit':'No fresh maintainer decision; missing historical intent is carried open.'})
(HERE/'question-audit.json').write_text(json.dumps(questions,indent=2))
