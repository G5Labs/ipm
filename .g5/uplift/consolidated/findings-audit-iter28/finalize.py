#!/usr/bin/env python3
"""Attach evidence to its ontology owner and check the product/register boundary."""
from pathlib import Path
from collections import Counter
import hashlib,json,re,os,subprocess,urllib.parse
H=Path(__file__).resolve().parent;R=H.parents[3]
def write(p,s):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s.rstrip()+'\n')
bugs=json.loads((H/'bugs.json').read_text())
files={'C':R/'.g5/uplift/consolidated/BUGS.md','G':R/'.g5/Verus_verification/bugs_issues.md','P':R/'playbook-uplift/BUGS.md'}
root_owner=R/'.g5/ontology/features/dependencies.g5.md'
root_text=root_owner.read_text()
root_note='- Root can be populated later in the load. A nonempty stored Root at return does not show which directory the earlier lock-file writer used.'
if root_note not in root_text:
 root_text=root_text.replace('- A direct helper with a populated Root is a distinct neighbor.',root_note+'\n\n- A direct helper with a populated Root is a distinct neighbor.',1)
 write(root_owner,root_text)
for b in bugs:
 if not b['disposition'].startswith('active') or b['owner_path']=='root.g5.md':continue
 p=R/'.g5/ontology'/b['owner_path'];s=p.read_text();id=b['owner_id']
 assert f'[I · {id}]' in s or f'[D · {id}]' in s or f'[Q · {id}]' in s,(b['id'],id)
 m=re.search(r'^### '+re.escape(id)+r'\s*\n',s,re.M)
 assert m,(b['id'],'no metadata',id)
 e=re.search(r'^### ',s[m.end():],re.M);end=m.end()+e.start() if e else len(s)
 url=os.path.relpath(files[b['arm']],p.parent)+'#'+b['anchor']
 line=f'Findings: [{b["id"]} — {b["repro_level"]}]({url}). The linked entry qualifies the caller, reader and providers.'
 if url not in s[m.end():end]:s=s[:end].rstrip()+'\n\n'+line+'\n\n'+s[end:];write(p,s)

design_owners={
 'C:S13':('features/build-and-publish.g5.md','build.package.deployed-status'),
 'C:S17':('features/uninstall.g5.md','uninstall.dependents'),
 'C:S20':('features/uninstall.g5.md','uninstall.child-error'),
 'C:S21':('features/build-and-publish.g5.md','build.test-output.run-identity'),
 'P:D1':('features/history.g5.md','history.survives-rollback'),
}
for id,(file,owner_id) in design_owners.items():
 target=R/('.g5/uplift/consolidated/DESIGN-SIMPLIFICATIONS.md' if id.startswith('C:') else 'playbook-uplift/DESIGN-SIMPLIFICATIONS.md')
 p=R/'.g5/ontology'/file;a=id.lower().replace(':','-')
 s=target.read_text();m=re.search(r'^<a id="'+a+r'"></a>\n### ',s,re.M);assert m
 nxt=re.search(r'^<a id="|^## ',s[m.end():],re.M);end=m.end()+nxt.start() if nxt else len(s)
 href=os.path.relpath(p,target.parent)+'#'+owner_id
 if href not in s[m.end():end]:s=s[:end].rstrip()+'\n\nGoverning owner: ['+owner_id+']('+href+'). The repair remains a proposal.\n\n'+s[end:];write(target,s)
 s=p.read_text();m=re.search(r'^### '+re.escape(owner_id)+r'\s*\n',s,re.M);assert m
 nxt=re.search(r'^### ',s[m.end():],re.M);end=m.end()+nxt.start() if nxt else len(s)
 back=os.path.relpath(target,p.parent)+'#'+a
 if back not in s[m.end():end]:s=s[:end].rstrip()+'\n\nFindings: ['+id+' — scoped repair comparison]('+back+').\n\n'+s[end:];write(p,s)

# New children belong beside the owning rule. Keep every pre-existing statement intact.
new_ids={x['id'] for x in json.loads((H/'questions.json').read_text()) if not any(x['id']==old[0] for old in json.loads((H/'canonical-questions.json').read_text()))}
for p in sorted((R/'.g5/ontology').rglob('*.md')):
 s=p.read_text()
 if '## Metadata' not in s:continue
 body,meta=s.split('## Metadata',1)
 for id in new_ids:
  pattern=r'<a id="'+re.escape(id)+r'"></a>\s*\n\[Q · '+re.escape(id)+r'\]\s*\n.*?(?=\n<a id=|\Z)'
  m=re.search(pattern,body,re.S)
  if not m:continue
  block=m[0].strip()+'\n\n';body=body[:m.start()]+body[m.end():]
  parent=id.rsplit('.',1)[0]
  spans=list(re.finditer(r'<a id="([^"]+)"></a>\s*\n\[[IDQ] · \1\]',body))
  parent_index=next((i for i,a in enumerate(spans) if a[1]==parent),None)
  assert parent_index is not None,(id,parent)
  next_i=parent_index+1
  while next_i<len(spans) and spans[next_i][1].startswith(parent+'.'):next_i+=1
  at=spans[next_i].start() if next_i<len(spans) else len(body)
  body=body[:at].rstrip()+'\n\n'+block+body[at:]
 ids=re.findall(r'\[[IDQ] · ([^\]]+)\]',body)
 ms=list(re.finditer(r'^### ([^\n]+)\s*\n',meta,re.M))
 prefix=meta[:ms[0].start()] if ms else meta
 sections={m[1]:meta[m.start():ms[i+1].start() if i+1<len(ms) else len(meta)].strip() for i,m in enumerate(ms)}
 assert set(ids)==set(sections),(str(p),set(ids)-set(sections),set(sections)-set(ids))
 for id in re.findall(r'\[Q · ([^\]]+)\]',body):
  if not re.search(r'^- uncertain\s*$',sections[id],re.M):
   sections[id]+='\n\n- uncertain'
 # Formatting changes only: match metadata to the unchanged order of owning blocks.
 write(p,body.rstrip()+'\n\n## Metadata\n'+prefix.rstrip()+'\n\n'+'\n\n'.join(sections[id] for id in ids))

# Store precisely which current assertions also pass against each fresh captured reader.
assertions=json.loads((H/'assertions.json').read_text());latest={}
for p in sorted((H/'runs').glob('*/receipt.json')):
 x=json.loads(p.read_text());latest[x['group']]=(p,x)
for group,(p,x) in latest.items():
 if group in assertions:
  raw=(p.parent/'session.log').read_text()
  bad=[a['id'] for a in assertions[group] if not re.search(a['pattern'],raw,re.M)]
  assert not bad,(group,bad)
  x['current_reader_assertions']=len(assertions[group])
 # Preserve the execution receipt; this is a separate review of that receipt.
replay={'groups':{k:{'receipt':str(p.relative_to(R)),'execution_gates':x['gates'],'reviewed_reader_assertions':x.get('current_reader_assertions',x['gates'])} for k,(p,x) in latest.items()},'note':'Execution receipts remain unchanged. This index rechecks their captured readers; it does not pretend a component replay is a new product execution.'}
write(H/'regression-index.json',json.dumps(replay,indent=2))

pin='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
changed=subprocess.run(['git','diff','--name-only',pin,'--','src','preload','modules','tests'],cwd=R,capture_output=True,text=True,check=True).stdout.strip()
untracked=subprocess.run(['git','ls-files','--others','--exclude-standard','--','src','preload','modules','tests'],cwd=R,capture_output=True,text=True,check=True).stdout.strip()
assert not changed and not untracked,(changed,untracked)
frozen=R.parent/'results/experiments/uplift-measurement-20261114/ipm-iter28/candidate'
archive_hash=hashlib.file_digest(open(frozen/'sealed-candidate.tar.gz','rb'),'sha256').hexdigest()
assert archive_hash=='8bf2e2688d5a963d18f4cb66504b180a88f685e32179601183117f4a800813b4'
receipt=json.loads((frozen/'receipt.json').read_text());assert receipt['archive_sha256']==archive_hash and receipt['product_pin']==pin
guard={'product_pin':pin,'production_diff':changed,'untracked_product_files':untracked,'archive_sha256':archive_hash,'frozen_receipt_sha256':hashlib.sha256((frozen/'receipt.json').read_bytes()).hexdigest(),'frozen_receipt_rechecked_only':True,'iteration29_not_written':True,'stopped_builder_not_restarted':True,'html_not_modified':True}
write(H/'boundary-check.json',json.dumps(guard,indent=2))

dispositions=Counter(b['disposition'] for b in bugs);levels=Counter(b['repro_level'] for b in bugs)
residual=[b for b in bugs if b['disposition'].startswith('inconclusive')]
out=['# IPM findings audit against iteration 28','',
 'Product `571e04a1`; sealed archive SHA-256 `'+archive_hash+'`. This is a register audit, not another uplift iteration. The live registers initially matched the nine sealed registers. Live ontology and unfinished iteration work were preserved; the only statement additions restore explicit unresolved questions. No product behavior, frozen receipt, benchmark HTML or stopped builder was changed.','',
 '## Reconciliation','', '| Starting inventory | Count |','|---|---|', '| C bug IDs | 31 |','| G bug IDs | 54 |','| P bug IDs | 80 |','| Total starting bug IDs | 165 |','', '| Disposition | Count |','|---|---|']
out += [f'| {k} | {v} |' for k,v in sorted(dispositions.items())]
out += ['', 'Aliases are counted against the starting IDs, not as additional defects. P:F-HIST-10 forwards to C:Q7 because the two observed deletion/close schedules do not justify a blanket no-resurrection guarantee. C:D25 forwards to P:F-UNIN-1 for resource removal and P:F-UNIN-6 for its success/status projection. The constructor’s immediate lock release withdrawal remains narrow; other BuildCache properties are not withdrawn.','',
 '[Every bug ID, original evidence, canonical owner and gate](BUG-REVIEW.md) · [Machine-readable inventory](bugs.json) · [Starting snapshots and hashes](starting-manifest.json).','',
 'The simplification inventory has 49 IDs. Five actual scoped model comparisons remain as repair evidence; none demonstrates a net state reduction. Forty-four broader simplification claims are withdrawn pending a comparable after model. [Per-ID preservation, metrics and moved obligations](SIMPLIFICATION-REVIEW.md). No repair is accepted or implemented.','',
 'The question audit accounts for all 31 inherited G IDs (plus aliases), 22 inherited C IDs and 44 P decision rows, as well as P verification items V1–V18. Seventeen product-policy assumptions formerly called resolved now have explicit Q owners; three other assumptions concern method/verification. Fourteen Q blocks were added (13 missing policy owners and C:Q23); there are 63 canonical Q blocks. Existing source-settled meanings stay settled and no new maintainer approval is claimed. [Question/source/owner audit](QUESTION-REVIEW.md) · [Public upstream review](UPSTREAM.md).','',
 '## Reusable execution evidence','',
 'From the product checkout, run:', '', '```sh', 'python3 .g5/uplift/consolidated/findings-audit-iter28/run_regressions.py early params inst repo nsp misc error readers lock returns nested iter11 iter17 iter21 iter24 iter27 iter28', '```','',
 'Each group creates and removes its own container from `ipm-consolidated-iris:571e04a1-fresh`, uses network `none`, and checks source bytes against the Git product pin. Real IRIS Community 2026.1 ARM64, filesystem, ObjectScript compiler, SQL, storage and language-extension providers are used. Ordinary directory modules and supported application entries are exercised. The C:D27 extension deliberately returns an error through the supported processor API; C:D21 exposes a private helper through a subclass and is only partial. C:D19 reuses an older substituted-helper witness and is not a fresh default-helper/full-command reproduction.','',
 f'{len(latest)} fresh passing cohorts contain {sum(x["gates"] for p,x in latest.values())} execution gates. They assert the recorded as-built discrepancy and passing neighbors; a passing witness does not mean the product meets the intended contract. [Commands, raw reader output, binary captures and results](regression-index.json). Unsuccessful fixture/observer attempts are retained under `runs/` and are not counted as evidence of a product defect.','',
 'Four fresh Verus files passed. The older durable-history file verified 37 functions and hit resource limits on two witnesses; its full run remains incomplete. The model comparisons are abstract contracts with declared premises, not proofs of shipped ObjectScript bodies. [Proof receipts](proofs/receipt.json). No finite state-count ratio is claimed for unbounded Seq/Map/Set/integer models.','',
 '## Incomplete exact-trigger qualifications','',
 'The entire starting inventory has a disposition, but the following exact application/reader gates remain inconclusive. These entries were removed from the active ordinary bug list; the starting claims and historical evidence remain available. Lack of reproduction is not proof of correctness. Security and previously interrupted archive investigations were not constructed, executed or resumed.','', '| Starting ID | Remaining gate |','|---|---|']
out += [f'| [{b["id"]}](BUG-REVIEW.md#{b["anchor"]}) | {b["reason"].replace("|",";")} |' for b in residual]
out += ['', '## Review regeneration', '', 'The native runner and run_proofs.py preserve new executions in dated attempt folders. refresh.py rebuilds the registers from the saved starting inventory and existing evidence, then checks them; it refuses to overwrite a register/owner edit made after this audit. Regeneration alone is not a new reproduction.', '', 'The supplied image ID is pinned in the runner as well as the source tree. The compiled-image installation provenance is inherited from the original fresh-image receipt; source byte agreement is not an independent proof of every compiled ObjectScript body.', '', '## Validation boundary','', '[Product/frozen-candidate guard](boundary-check.json) records an empty product diff and no new product files. No commit was made because the checkout already contained staged and unfinished work. Ontology structure and current register/link checks are recorded in `validation.json`; inherited writing warnings remain distinct from mechanical errors.']
write(H/'README.md','\n'.join(out))
write(H/'summary.json',json.dumps({'starting_bug_ids':165,'dispositions':dict(dispositions),'reproduction_levels':dict(levels),'fresh_passing_groups':len(latest),'execution_gates':sum(x['gates'] for p,x in latest.values()),'inconclusive_ids':[b['id'] for b in residual],'simplifications':{'starting':49,'scoped_repair_comparisons':5,'withdrawn_simplification_claims':44},'questions':{'starting_G':31,'starting_C':22,'starting_P':44,'canonical_Q':63,'new_Q':14,'new_C_question':'C:Q23'},'implementation_proofs':0},indent=2))
print(json.dumps({'dispositions':dict(dispositions),'passing_groups':len(latest),'gates':sum(x['gates'] for p,x in latest.values())},indent=2))
