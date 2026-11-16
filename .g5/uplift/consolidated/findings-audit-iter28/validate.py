#!/usr/bin/env python3
"""Validate actual register locations without relocating their relative links."""
from pathlib import Path
import importlib.util,json,re,subprocess,sys,os,urllib.parse,hashlib
H=Path(__file__).resolve().parent;R=H.parents[3]
GUIDE=Path('/Users/ziniuw/Desktop/research/G5-Lean-V2/.agents/guidance/scripts')
spec=importlib.util.spec_from_file_location('g5_findings_check',GUIDE/'check_findings.py')
mod=importlib.util.module_from_spec(spec);sys.modules[spec.name]=mod;spec.loader.exec_module(mod)
regfiles=['.g5/uplift/consolidated/'+n+'.md' for n in ['BUGS','DESIGN-SIMPLIFICATIONS']]+['.g5/Verus_verification/'+n+'.md' for n in ['bugs_issues','design_simplification']]+['playbook-uplift/'+n+'.md' for n in ['BUGS','DESIGN-SIMPLIFICATIONS']]
issues=[];entry_counts={}
for name in regfiles:
 p=R/name;entries,bad=mod.check_text(str(p),p.read_text());issues.extend(map(str,bad))
 kind='bugs_issues.md' if p.name in {'BUGS.md','bugs_issues.md'} else 'design_simplification.md'
 issues.extend(map(str,mod.check_evidence(str(p),kind,entries,p.parent)))
 issues.extend(map(str,mod.check_back_links(p.parent,R/'.g5/ontology',{p.name:entries})))
 issues.extend(map(str,mod.check_previous(str(p),entries,(H/'before'/name).read_text())))
 entry_counts[name]={'entries':len(entries),'active':sum(e.actionable for e in entries)}
source_checks={}
for model in json.loads((H/'simplifications.json').read_text()):
 path=R/model['model']
 source_checks[model['id']]={'exists':path.exists(),'hash_matches_review':path.exists() and hashlib.sha256(path.read_bytes()).hexdigest()==model['model_sha256']}
 assert source_checks[model['id']]['hash_matches_review'],model['id']
ontology=subprocess.run([sys.executable,str(GUIDE/'check_ontology.py'),str(R/'.g5/ontology')],capture_output=True,text=True)
(H/'ontology-after-check.log').write_text(ontology.stdout+ontology.stderr)
summary=re.search(r'Checked (\d+) ontology Markdown file\(s\), (\d+) block\(s\): (\d+) issue\(s\), (\d+) writing warning\(s\)',ontology.stdout)
assert summary,ontology.stdout[-1000:]
ontology_counts=dict(zip(['files','blocks','issues','writing_warnings'],map(int,summary.groups())))
if ontology.returncode:issues.append('Ontology mechanical check failed; see ontology-after-check.log')
# Check authored current links. Historical raw snapshots intentionally retain their old relative context.
targets=regfiles+['.g5/uplift/consolidated/QUESTIONS.md','.g5/Verus_verification/open_questions.md','playbook-uplift/QUESTIONS.md']+[str(p.relative_to(R)) for p in H.glob('*.md')]
link_errors=[]
for name in targets:
 p=R/name;lines=p.read_text().splitlines();usable=mod._structure_lines(lines)
 for i,line in enumerate(lines):
  if not usable[i]:continue
  for url in mod._link_targets(line):
   if re.match(r'^[a-z][a-z0-9+.-]*:',url,re.I):continue
   path,_,frag=urllib.parse.unquote(url).partition('#')
   q=(p.parent/path).resolve() if path else p
   if not q.exists():link_errors.append(f'{name}:{i+1}: {url}')
issues+=link_errors
result={'register_issues':issues,'entries':entry_counts,'ontology':ontology_counts,'simplification_source_checks':source_checks,'link_files_checked':len(targets),'missing_local_link_targets':link_errors,'scope':'Mechanical register/status/evidence/backlink/protected-decision checks at all six real paths; current authored local-link existence; no intent or implementation proof is inferred.'}
(H/'validation.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'register_problems':len(issues),'ontology':ontology_counts,'missing_link_targets':len(link_errors),'entries':entry_counts},indent=2))
if issues:
 print('\n'.join(issues[:20]));sys.exit(1)
