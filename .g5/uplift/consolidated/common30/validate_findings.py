#!/usr/bin/env python3
"""Mechanical register/ontology checks at the real register paths (adapted from findings-audit-iter28/validate.py).

Usage: validate_findings.py PREVIOUS_DIR OUT_JSON
PREVIOUS_DIR holds pre-edit copies named by the repo path with '/' replaced by '_'.
Checks: entry format, evidence fields, ontology back links, protected previous decisions, ontology checker,
model link resolution, and existence of current authored local links. No intent or proof is inferred."""
from pathlib import Path
import importlib.util,json,re,subprocess,sys,os,urllib.parse
R=Path(__file__).resolve().parents[4]
GUIDE=Path(os.environ.get('G5_HARNESS',str(Path.home()/'.agents')))/'guidance/scripts'
prev=Path(sys.argv[1]).resolve();out=Path(sys.argv[2]).resolve()
spec=importlib.util.spec_from_file_location('g5_findings_check',GUIDE/'check_findings.py')
mod=importlib.util.module_from_spec(spec);sys.modules[spec.name]=mod;spec.loader.exec_module(mod)
regfiles=['.g5/uplift/consolidated/'+n+'.md' for n in ['BUGS','DESIGN-SIMPLIFICATIONS']]+['.g5/Verus_verification/'+n+'.md' for n in ['bugs_issues','design_simplification']]+['playbook-uplift/'+n+'.md' for n in ['BUGS','DESIGN-SIMPLIFICATIONS']]
issues=[];counts={}
for name in regfiles:
    p=R/name;entries,bad=mod.check_text(str(p),p.read_text());issues+=map(str,bad)
    kind='bugs_issues.md' if p.name in {'BUGS.md','bugs_issues.md'} else 'design_simplification.md'
    issues+=map(str,mod.check_evidence(str(p),kind,entries,p.parent))
    issues+=map(str,mod.check_back_links(p.parent,R/'.g5/ontology',{p.name:entries}))
    before=prev/name.replace('/','_')
    if before.exists():issues+=map(str,mod.check_previous(str(p),entries,before.read_text()))
    counts[name]={'entries':len(entries),'active':sum(e.actionable for e in entries),'previous_checked':before.exists()}
onto=subprocess.run([sys.executable,str(GUIDE/'check_ontology.py'),str(R/'.g5/ontology')],capture_output=True,text=True)
summary=re.search(r'Checked (\d+) ontology Markdown file\(s\), (\d+) block\(s\): (\d+) issue\(s\), (\d+) writing warning\(s\)',onto.stdout)
oc=dict(zip(['files','blocks','issues','writing_warnings'],map(int,summary.groups()))) if summary else {'raw':onto.stdout[-2000:]}
if onto.returncode:issues.append('ontology check failed')
links=subprocess.run([sys.executable,str(GUIDE/'link_verification.py'),str(R/'.g5/ontology')],capture_output=True,text=True)
targets=regfiles+['.g5/uplift/consolidated/QUESTIONS.md','.g5/Verus_verification/open_questions.md','playbook-uplift/QUESTIONS.md','.g5/uplift/consolidated/HANDOFF.md']
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
res={'register_issues':issues,'entries':counts,'ontology':oc,'ontology_output_tail':onto.stdout[-1500:],'link_verification_rc':links.returncode,'link_verification_tail':(links.stdout+links.stderr)[-1500:],'missing_local_link_targets':link_errors}
out.write_text(json.dumps(res,indent=2)+'\n')
print(json.dumps({'register_problems':len(issues),'ontology':oc,'link_verification_rc':links.returncode,'missing_links':len(link_errors)},indent=1))
if issues:print('\n'.join(issues[:30]));sys.exit(1)
