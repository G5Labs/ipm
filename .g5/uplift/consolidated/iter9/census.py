"""Same-rule bridge for resolved trace candidates; never calls them verified bodies."""
from pathlib import Path
import collections,csv,json,sys
HERE=Path(__file__).resolve().parent;CONS=HERE.parent
def main(n):
 census={r['member']:r for r in csv.DictReader((CONS/'coverage/members.tsv').open(),delimiter='\t')}
 old=json.loads((CONS/'iter2/inventory/baseline-reach.json').read_text());members=set(old['represented_members']);history=[]
 for i in range(2,n+1):
  if i==2:
   members.update(b['member'] for b in json.loads((CONS/'iter2/bindings.json').read_text()) if b.get('credit')!='challenge-only-no-member-gain')
  elif i==3:members.update(b['member'] for b in json.loads((CONS/'iter3/INDEX.json').read_text())['bindings'])
  else:
   f=CONS/f'iter{i}/bindings.json';f=f if f.exists() else CONS/f'iter{i}/INDEX.json'
   for row in json.loads(f.read_text())['rows']:
    for role in ['direct','reader']:
     for path,name in row[role]:members.add(path.removeprefix('src/cls/IPM/')+'::'+name)
  history.append({'iteration':i,'resolved_trace_candidates':len(members & census.keys())})
 valid=members & census.keys();unlinked=sorted(census.keys()-members)
 by=collections.defaultdict(list)
 for mid in unlinked:by[mid.split('/')[0] if '/' in mid else '(root)'].append(mid)
 out=CONS/f'iter{n}';out.mkdir(exist_ok=True)
 result={'pin':old['pin'],'denominator':len(census),'unit':'Pinned production ClassMethod/Method/Query/Trigger identities under src/cls/IPM; unchanged census script and source roots','historical_bridge':history,'resolved_trace_candidates':len(valid),'members':sorted(valid),'unresolved_historical_declarations':sorted(members-census.keys()),'unlinked_by_subsystem':dict(by),'unlinked_count':len(unlinked),'caution':'Resolved scoped locators, including callers/readers. This is NOT meaningful-binding or audited-workflow coverage; no direct ObjectScript executable-body proofs. Inherited semantic credits are carried, not independently rea udited here.'}
 (out/'census.json').write_text(json.dumps(result,indent=2)+'\n')
 (out/'unlinked-members.tsv').write_text('subsystem\tmember\n'+''.join(k+'\t'+m+'\n' for k,rows in sorted(by.items()) for m in rows))
 print(json.dumps({k:result[k] for k in ['denominator','historical_bridge','resolved_trace_candidates','unlinked_count']}))
if __name__=='__main__':main(int(sys.argv[1]) if len(sys.argv)>1 else 9)
