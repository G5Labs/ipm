#!/usr/bin/env python3
"""Join scoped source review, exact endpoints and proof receipts; never edit iter1 counts."""
from pathlib import Path
import json, hashlib, re
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
FORMAL=ROOT/'.g5/Verus_verification/consolidated/iter2'
def read(p):return json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 review=read(HERE/'inventory/new-binding-review.json')
 baseline=read(HERE/'inventory/baseline-reach.json')
 old=read(ROOT/'.g5/uplift/consolidated/extension/member-dispositions.json')
 represented={x['member'] for x in old if x['disposition'] in ['owns','delegates']}
 assert len(represented)==174
 proofs=read(HERE/'proofs/models.json')
 assert all(x['proof']['exit']==0 and all(y['rejected_at_assertion'] for y in x['controls']) for x in proofs['models'])
 edges=[]
 for i,b in enumerate(review['bindings']):
  p=FORMAL/b['model'];src=p.read_text()
  assert re.search(r'\bfn '+re.escape(b['symbol'])+r'\b',src),b
  assert re.search(r'\bfn '+re.escape(b['proof'])+r'\b',src),b
  path,span=b['source_slice'].rsplit(':',1);start,end=map(int,span.split('-'))
  text='\n'.join((ROOT/path).read_text().splitlines()[start-1:end])+'\n'
  # Review hashes use exact slice without trailing newline.
  assert b['source_sha256'] in [hashlib.sha256(text.encode()).hexdigest(),hashlib.sha256(text.rstrip('\n').encode()).hexdigest()],b['member']
  if b['model']=='readers.rs':
   owner='uninstall.all' if b['symbol']=='uninstall_all_step' else 'build.test-output.counts' if b['member'].startswith(('Test/','Lifecycle/','ResourceProcessor/')) else 'inspect.dependents'
  else:
   owner='repo.publish-cache' if b['symbol'] in ['remote_cache','oras_cache','manager_status','capability'] else 'build.publish.partial-effects' if 'Publish' in b['member'] or b['symbol'] in ['write_prefix','delete_prefix','remote_delete','remote_delete_enabled'] else 'repo.oras.integrity' if b['symbol'] in ['last_tag','verify_layers','module_manifest','has_module','pull'] else 'repo.oras'
  edges.append({'id':f'iter2-reviewed-{i+1:02d}',**b,'formal_symbol':str(p.relative_to(ROOT))+'::'+b['symbol'],
    'ontology':owner,'role':'owns','formal_sha256':sha(p),'proof_receipt':'.g5/uplift/consolidated/iter2/proofs/models.json',
    'scope_status':'scoped-Verus-representation','product_body_proved':False,'premises':b['remaining_scope']})
 edges+=read(HERE/'boundaries/bindings.json')
 ids={x['member'] for x in edges}
 new=ids-represented
 after=represented|ids
 assert len(after)==len(represented)+len(new)
 reverse={}
 for x in edges:reverse.setdefault(x['member'],[]).append(x['id'])
 out={'pin':'571e04a19aba9f9b4bc0ea23fbe09182038f4e55','denominator':1216,'before':174,'after':len(after),
  'before_direct':165,'before_shared_only':9,'after_direct':165+len(new),'after_shared_only':9,
  'new_direct':sorted(new),'new_shared_only':[],'whole_member_withdrawals':[],
  'property_withdrawals':read(HERE/'boundaries/withdrawals.json'),
  'remaining_unrepresented':1216-len(after),'implementation_proofs':0,
  'caution':'Member union is narrow representation, not whole-body correctness, independent discovery count, or sample performance.',
  'source_reviewed_outside_baseline':75,'outside_unclassified_after_source_review':967}
 (HERE/'bindings.json').write_text(json.dumps(edges,indent=2)+'\n')
 (HERE/'reverse-bindings.json').write_text(json.dumps(reverse,indent=2)+'\n')
 (HERE/'coverage.json').write_text(json.dumps(out,indent=2)+'\n')
 print(json.dumps({k:out[k] for k in ['before','after','after_direct','after_shared_only','remaining_unrepresented']}))
if __name__=='__main__':main()
