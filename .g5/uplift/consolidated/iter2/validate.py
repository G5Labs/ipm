#!/usr/bin/env python3
"""Validate actual baseline preservation and sealed-answer integrity without resetting Git."""
from pathlib import Path
import hashlib,json,subprocess,re,datetime
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
EXP=ROOT.parent/'results/experiments/uplift-measurement-20261114/ipm-iter2'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 baseline=json.loads((EXP/'baseline/files.json').read_text())
 changed=[];missing=[]
 for rel,info in baseline.items():
  p=ROOT/rel
  if not p.is_file():missing.append(rel)
  elif sha(p)!=info['sha256']:changed.append(rel)
 unexpected=[x for x in changed if not x.startswith('.g5/ontology/')]
 manifest=json.loads((EXP/'candidate/sealed-files.json').read_text())
 sealed_changes=[x for x,h in manifest.items() if not (ROOT/x).is_file() or sha(ROOT/x)!=h]
 initial=json.loads((EXP/'baseline/receipt.json').read_text())
 pin=initial['product_pin']
 diff=subprocess.check_output(['git','diff',pin,'--','src','preload','modules','wheels','module.xml','Dockerfile','docker-compose.yml'],cwd=ROOT)
 docs={};duplicates=[]
 for p in (ROOT/'.g5/ontology').rglob('*.md'):
  for match in re.finditer(r'<a id="([^"]+)"></a>\s*\n\[([IDQ]) · ([^\]]+)\]',p.read_text()):
   ident,kind,sid=match.groups()
   if ident in docs:duplicates.append(ident)
   assert ident==sid,(ident,sid)
   docs[ident]={'path':str(p.relative_to(ROOT)),'kind':kind}
 edges=json.loads((HERE/'bindings.json').read_text())
 unresolved_owners=[x['ontology'] for x in edges if x['ontology'] not in docs]
 # Detect added broken Markdown file links; historical unresolved links are reported separately.
 broken=[]
 for rel in changed:
  p=ROOT/rel
  for link in re.findall(r'\]\(([^)]+)\)',p.read_text()):
   if '://' in link or link.startswith('#'):continue
   target=link.split('#')[0]
   if target and not (p.parent/target).exists():broken.append({'file':rel,'target':link})
 out={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'branch':subprocess.check_output(['git','branch','--show-current'],cwd=ROOT,text=True).strip(),
  'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'baseline_files_checked':len(baseline),
  'modified_existing_files':changed,'unexpected_modified_existing':unexpected,'missing_baseline_files':missing,
  'git_index_unchanged':sha(ROOT/'.git/index')==sha(EXP/'baseline/index'),'product_diff_empty':not diff,
  'sealed_answer_files_checked':len(manifest),'sealed_answer_changes':sealed_changes,'canonical_blocks':len(docs),
  'duplicate_ids':duplicates,'unresolved_binding_owners':unresolved_owners,'broken_relative_links_in_changed_pages':broken,
  'benchmark_html_sha256':sha(ROOT.parent/'docs/g5-formal-verification.html'),
  'grading_inputs_sha256':{n:sha(ROOT.parent/'results/experiments/uplift-measurement-20261114'/n) for n in ['score-audit-v2.json','expanded-case-ledger.json']}}
 (HERE/'validation.json').write_text(json.dumps(out,indent=2)+'\n')
 (HERE/'INDEX.json').write_text(json.dumps({'canonical_blocks':docs,'scope':'Generated navigation; individual evidence is in bindings.json. Presence is not coverage.'},indent=2)+'\n')
 assert not unexpected and not missing and out['git_index_unchanged'] and not diff and not sealed_changes
 assert not duplicates and not unresolved_owners
 print(json.dumps({k:out[k] for k in ['baseline_files_checked','git_index_unchanged','product_diff_empty','sealed_answer_files_checked','sealed_answer_changes','canonical_blocks','broken_relative_links_in_changed_pages']}))
if __name__=='__main__':main()
