#!/usr/bin/env python3
from pathlib import Path
import re,json,os,hashlib
H=Path(__file__).resolve().parent;R=H.parents[3]
paths={'C':'.g5/uplift/consolidated/DESIGN-SIMPLIFICATIONS.md','G':'.g5/Verus_verification/design_simplification.md','P':'playbook-uplift/DESIGN-SIMPLIFICATIONS.md'}
def move_links(text,source,dest):
 def sub(m):
  label,url=m.group(1),m.group(2)
  if re.match(r'[a-z]+:',url) or url.startswith('/') or ' ' in url:return m[0]
  part,mark,frag=url.partition('#')
  path=source if not part else (source.parent/part).resolve()
  return '['+label+']('+os.path.relpath(path,dest.parent)+('#'+frag if mark else '')+')'
 return re.sub(r'\[([^\]\n]+)\]\(([^)]+)\)',sub,text)
items={}
for arm,path in paths.items():
 s=(H/'sealed'/path).read_text()
 if arm=='C':
  for m in re.finditer(r'^\| (C:S\d+b?)\b[^\n]*',s,re.M):
   id=m[1];items.setdefault(id,{'id':id,'arm':arm,'original':m[0],'title':m[0].split(' | ')[1]})
  m=re.search(r'^C:S10 .*?(?=\n\n)',s,re.M|re.S);items['C:S10']={'id':'C:S10','arm':arm,'original':m[0],'title':'Persist the intended root before writing a lock file'}
 else:
  pattern=r'^## '+('S' if arm=='G' else 'D')+r'(\d+)[.:] (.*?)(?=^## |\Z)'
  for m in re.finditer(pattern,s,re.M|re.S):
   id=arm+':'+('S' if arm=='G' else 'D')+m[1]
   items[id]={'id':id,'arm':arm,'title':m[2].splitlines()[0],'original':m[0]}
assert len(items)==49,len(items)

default_model={'C':'.g5/Verus_verification/consolidated/verus/thin_surfaces.rs','G':'.g5/Verus_verification/crates/ipm-load-history/src/model.rs','P':'playbook-uplift/models/verus/load_general.rs'}
models={
 **{f'C:S{i}':'.g5/Verus_verification/consolidated/verus/attr_evaluator.rs' for i in [2,3,8]},
 **{f'C:S{i}':'.g5/Verus_verification/consolidated/verus/semver.rs' for i in [4,5,9]},
 'C:S1':'.g5/Verus_verification/consolidated/verus/temp_repo.rs','C:S7':'.g5/Verus_verification/consolidated/verus/temp_repo.rs',
 'C:S6':'.g5/Verus_verification/consolidated/verus/owners.rs','C:S10':'.g5/Verus_verification/consolidated/verus/owners.rs',
 'C:S11':'.g5/Verus_verification/consolidated/iter3/artifacts.rs','C:S12':'.g5/Verus_verification/consolidated/iter3/artifacts.rs',
 'C:S13':'.g5/Verus_verification/consolidated/iter3/resources.rs','C:S14':'.g5/Verus_verification/consolidated/iter4/registry.rs',
 'C:S14b':'.g5/Verus_verification/consolidated/iter5/redirect.rs','C:S15':'.g5/Verus_verification/consolidated/iter4/repairs4.rs',
 'C:S16':'.g5/Verus_verification/consolidated/iter4/registry.rs','C:S17':'.g5/Verus_verification/consolidated/iter5/webapp.rs',
 'C:S18':'.g5/Verus_verification/consolidated/iter5/entry_forms.rs','C:S19':'.g5/Verus_verification/consolidated/iter5/webapp.rs',
 'C:S20':'.g5/Verus_verification/consolidated/iter10/errors.rs','C:S21':'.g5/Verus_verification/consolidated/iter17/report_identity.rs',
 'G:S2':'.g5/Verus_verification/crates/ipm-load-history/src/model.rs','G:S3':'.g5/Verus_verification/crates/ipm-load-history/src/model.rs',
 'G:S4':'.g5/Verus_verification/followup/models/history_readers.py','G:S8':'.g5/Verus_verification/followup/models/loadtx.py',
 'P:D3':'playbook-uplift/models/verus/uninstall.rs','P:D4':'playbook-uplift/models/verus/uninstall.rs',
 'P:D5':'playbook-uplift/models/verus/settings.rs','P:D6':'playbook-uplift/models/verus/update_steps.rs',
 'P:D7':'playbook-uplift/models/verus/update_steps.rs','P:D9':'playbook-uplift/models/verus/ns_mapping.rs',
 'P:D10':'playbook-uplift/models/verus/sync_baseline.rs','P:D16':'playbook-uplift/models/verus/dep_resolve.rs',
}
paired={
 'C:S13':{'before':'export_item','after':'repaired_export_item','record':'ExportResult','metric':'0 mutable model fields on both sides; 4 input parameters and 3 result fields on both sides. The proposal adds one failed-helper decision before fallback; no state field or interface disappears.','preserve':'Successful deployed handling, legitimate source fallback and source-export errors.','cost':'The caller must retain deployed_ok and prove that the new guard precedes fallback. export_repair_preserves_success and export_repair_stops_failed_helper are two distinct obligations. Decoder, provider and whole-package behavior stay outside this comparison.'},
 'C:S17':{'before':'uninstall','after':'repaired_uninstall','record':'CleanResult','metric':'3 CleanResult fields on each side, with the same Apps map. The dependent check moves before app deletion; no persistent field is eliminated.','preserve':'Unblocked and forced uninstall and the app state of valid neighbors. A blocked command intentionally changes to an error with app retained.','cost':'The authorization/ownership check must precede all cleanup hooks, not just the modeled web-app deletion. The abstract pair does not prove full hook ordering, general compensation or recursive cleanup.'},
 'C:S20':{'before':'uninstall','after':'repaired_uninstall','record':'State','metric':'3 State fields on both sides (module, manifest, code) and 2 Outcome fields on both sides. One returned-status guard is added; no state field or transition obligation is eliminated.','preserve':'Successful child enumeration and ordinary class cleanup; the error path must retain metadata and report failure.','cost':'The dispatcher now owns the returned-status check. Earlier resource deletions and extension side effects are still not compensated. returned_error_neighbors proves the successful neighbor equality and the changed failure outcome.'},
 'C:S21':{'before':'report(w,start,None)','after':'report(w,start,device_index(w))','record':'State','metric':'2 State fields on both sides (db, tracked_ids), and 5 Report fields on both sides. The selected ID is forwarded to an existing API; the optional default selector and generic API still exist. Zero stored fields are removed.','preserve':'Single-run results, outer command status, nested child rows and file-before-fatal-check ordering.','cost':'Every report caller must forward the correct ID; selection responsibility moves to that caller. explicit_forwarding_preserves_reader adds an equality obligation. Stream/serialization failure and full nested lifetimes remain provider/caller premises.'},
 'P:D1':{'before':'Cfg.fix=false','after':'Cfg.fix=true','record':'L2','metric':'The same 10 declared L2 fields occur on both sides (including configuration and four ghost fields, so this is not an independent semantic-state count). The proposed configuration activates the pending Seq<Row> and enables the Reconcile action: one additional active history collection and one additional recovery transition. No field is eliminated.','preserve':'Per-action success versus kept database writes, rollback-surviving history and the named caller/crash boundary.','cost':'Durability is paid for by extra durable writes, a pending dependency collection and reconciliation/version lookup. The instance-loss-of-pending theorem remains; a durable pending marker is not modeled. Caller transactions, terminal commit and commit-window assumptions are not removed. The fresh full-file run has 37 verified and 2 resource-limit failures, so this audit does not claim a complete green re-verification.'},
}
aliases={'C:S14':'C:S14b','G:S7':'C:S1','G:S10':'P:D8','G:S6':'P:D13','G:S5':'P:D2','C:S2':'C:S8','P:D15':'G:S1','P:D14':'P:F-LOAD-4'}
costs={
 'G:S1':'Validation moves to a preflight reader. It still must check parsed documents, mutable input consistency, imported requirements and developer-mode refusal policy; no comparable full preflight Verus variant exists.',
 'G:S2':'Variant 103 covers transactional dependency rows, not the whole top-row/caller/NoJournal/unfinished-row proposal. Guarded/global Python variants have different policy scope. Their state totals cannot be promoted to a comparable Verus state reduction.',
 'G:S3':'Removing the success-based marker requires a complete account of every durable writer and escaped dependency. It is a correctness condition for the history repair, not elimination of an obligation.',
 'G:S4':'The comparison is an executable Python reader model, not an actual paired Verus filter design. Parsing, column validation, typed values and SQL binding remain obligations in a structured representation.',
 'G:S8':'A single mode moves the decision to the load boundary and changes dependency-default policy. No same-policy paired Verus comparison covers that change; the Python end-state totals are not Verus state counts.',
 'P:D2':'A slogan covering unrelated error sites does not remove their independent status obligations. Each producer/caller still needs checked propagation and a supported failing input.',
 'P:D4':'Resolved identity must be available before self-removal and recursion; normalization does not settle externally named aliases or remove privileged cleanup checks.',
 'P:D5':'Shared selection still needs different search, publication and unpublication policies and provider order. Those obligations move to the shared selector; no complete proposed Verus selector was supplied.',
 'P:D6':'Exact-name parsing repairs class discovery but adds the obligation to define/refuse legacy helper names. The as-built witness is not a proved after parser.',
 'P:D7':'Moving flags to the command boundary does not preserve stored mode, dependency defaults and step-seeding policy without owner decisions. Those obligations remain at the boundary.',
 'P:D8':'Staging files adds a durable stage, commit/rename/failure cleanup and crash recovery. Database and filesystem effects remain non-atomic; no after Verus model includes those obligations.',
 'P:D9':'A shared mapping signature does not remove pending-versus-active state, activation failures, collation and source ambiguity. A whole-configuration after model is absent.',
 'P:D10':'A common baseline writer still needs failed-import retry semantics, exact-byte revert and unrelated-server edits. No paired after Verus model covers all of these callers.',
 'P:D11':'Removing an accepted option changes the public interface; implementing it adds its advertised obligation. Neither alternative is a measured behavior-preserving simplification.',
 'P:D12':'Helpers retain differences in namespace ownership, authentication and manifest cleanup. Reuse alone gives no model reduction; the proposal has no complete paired after model.',
 'P:D13':'This groups unrelated local repairs. It is not one comparable design; fields/obligations cannot be summed across different domains as one simplification.',
 'P:D16':'The upstream repair changes repository result cardinality so resolution can consider alternatives. Search-space/caching/provider obligations grow or move; the witness in dep_resolve is not the complete upstream after design.',
 'P:D17':'Migration adds a writer wherever enable reaches legacy records and must preserve mappings, schema compatibility and partial-failure behavior. No paired after Verus migration model exists.',
 'C:S4':'Removing the year exception would decide open C:Q3 and alter the native year-version neighbor. The suggested unification is not behavior preserving until that policy is settled.',
 'C:S8':'A shared literal replacement routine still has different module/system/environment callers and failure/merge policies. The source has no complete paired Verus helper with all caller obligations.',
 'C:S10':'Persisting Root adds a write/ordering/rollback obligation before file generation. The tested as-built lockfile witness is not an after design.',
 'C:S18':'The proposed function is an intended-outcome stub, not a generator model. It cannot substantiate removing a generator branch or preserving user language extensions.',
}
review=H/'SIMPLIFICATION-REVIEW.md';lines=['# Simplification audit','',
 'Starting inventory: 49 IDs (C 22 including S14b, G 10, P 17). No product design is accepted or implemented. Five scoped before/after model comparisons are retained as repair evidence, with zero substantiated state-field reduction. The other 44 complexity claims are withdrawn pending a comparable after model; an as-built witness is not an after design. Related repair ideas remain in this review with stable IDs.','',
 'Metrics below count named source structures, decisions or obligations in the same model scope. They are measured source counts, not reachable-state counts, performance estimates or total implementation costs. Seq, Map, Set and integer domains are unbounded unless a separate finite experiment fixes them. No finite state ratio is claimed. Historical Python/explorer totals are retained in the sealed files with their original bounds and policies; they do not become Verus state counts.','']
for id,x in items.items():
 model=models.get(id,default_model[x['arm']]);path=R/model
 x.update(model=model,model_exists=path.exists(),model_sha256=hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None,disposition='repair-only; no simplification demonstrated' if id in paired else 'simplification claim withdrawn',comparison=paired.get(id))
 lines += [f'<a id="{id.lower().replace(":","-")}"></a>',f'### {id}: {x["title"][:150]}','',f'Disposition: {x["disposition"]}.','', 'Comparison: none. Verus-only evidence; no TLC pair or state count is claimed.','']
 if id in paired:
  c=paired[id];x['cost_review']=c['cost'];lines += [f'Actual model pair: `{model}::{c["before"]}` → `{c["after"]}`.','', 'Behavior to preserve: '+c['preserve'],'', 'Measured source comparison: '+c['metric'],'', 'Decision/obligation destination and limits: '+c['cost'],'', '[Fresh proof receipts](proofs/receipt.json). These prove only the named abstract model under its written premises; they are not ObjectScript-body proofs.','']
 else:
  cost=costs.get(id,'The as-built model or narrower repair boundary does not contain a comparable after design for the whole proposal. The same callers, provider failures, public interface and policy exceptions still require obligations; moving the check to a shared helper does not eliminate them. Preserve every successful/allowed neighbor named in the original proposal. No complete preservation theorem or source-supported reduction count was found, so the simplification claim is withdrawn.')
  x['cost_review']=cost
  lines += [f'As-built/model evidence inspected: `{model}`'+(' (available).' if path.exists() else ' (not present at this named location; no evidence credited).'),'', 'Before/after result: no complete comparable proposed Verus design was found for this ID. Counts and reduction estimates are therefore unavailable, not zero.','',cost,'']
  if id in aliases:lines += ['Related owning proposal/repair: '+aliases[id]+'. This is a cross-reference, not additional reduction credit.','']
 lines += ['Preserved original behavior, evidence and stated alternatives:', '',move_links(x['original'],R/paths[x['arm']],review),'']
save=lambda p,s:p.write_text(s.rstrip()+'\n')
save(review,'\n'.join(lines));save(H/'simplifications.json',json.dumps(list(items.values()),indent=2))
for arm,path in paths.items():
 target=R/path;out=[f'# IPM simplification register: {arm}','',f'All starting {arm} IDs are reconciled below. No product redesign is accepted or implemented. [Before/after evidence and cost review]({os.path.relpath(review,target.parent)}) records preserved behavior, assumptions, public interfaces and maintenance obligations.','', '## Scoped repair comparisons','']
 for id,x in items.items():
  if x['arm']!=arm or id not in paired:continue
  c=paired[id];a=id.lower().replace(':','-');out += [f'<a id="{a}"></a>',f'### {id}: {x["title"][:140]}','', 'Severity: Medium · Decision: Open','', 'Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.', '',c['preserve'],'',c['metric'],'',c['cost'],'',f'[Actual model pair, proof result and source]({os.path.relpath(review,target.parent)}#{a}). Retained as a correctness repair; no net simplification is established.','']
 out += ['## Withdrawn simplification claims','', '| Starting ID | Disposition and preserved proposal |','|---|---|']
 for id,x in items.items():
  if x['arm']==arm and id not in paired:
   a=id.lower().replace(':','-');out.append(f'| {id} | [Withdrawn complexity claim; repair idea and remaining obligations preserved]({os.path.relpath(review,target.parent)}#{a}) |')
 old=(H/'sealed'/path).read_text()
 for head in re.findall(r'^## (.+)$',old,re.M):
  a=re.sub(r'[^\w\- ]','',head.lower()).replace(' ','-');out += [f'<a id="{a}"></a>']
 save(target,'\n'.join(out))
print('simplifications',len(items),'repair comparisons',len(paired),'withdrawn',len(items)-len(paired))
