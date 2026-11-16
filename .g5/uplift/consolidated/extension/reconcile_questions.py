from pathlib import Path
import re,json
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];ONT=ROOT/'.g5/ontology'
def replace(file,id,text,kind=None):
 p=ONT/file;s=p.read_text();pat=r'(^\[[IDQ] · '+re.escape(id)+r'\]\n)(.*?)(?=\n<a id=|\n## |\Z)';m=re.search(pat,s,re.M|re.S);assert m
 header='['+kind+' · '+id+']\n' if kind else m[1];s=s[:m.start()]+header+'\n'+text.strip()+'\n'+s[m.end():];p.write_text(s)
replace('features/history.g5.md','history.dependencies.parent-rollback','''A dependency's `Success` records its own returned status, even if its parent's later rollback undoes its data. `Committed` describes kept database writes. The surviving-record requirement remains in [history.survives-rollback](#history.survives-rollback).

As built, the child can finish successfully and then lose its permanent row with the parent's rollback (G:D1 = P:F-HIST-1). These facts are supported by the property comments, native receipts, and the history design discussion recorded in P DEC-HIST-1. Q-06 remains an alias of this resolved meaning; audit retention under a caller's transaction stays [open](#history.committed.nested).''','D')
for file,id in [('features/history.g5.md','history.committed.side-effects'),('concepts/transactions-and-dev-mode.g5.md','transaction.rollback-scope')]:
 p=ONT/file;s=p.read_text().replace('[Q · '+id+']','[D · '+id+']');p.write_text(s)
p=ONT/'concepts/modules.g5.md';s=p.read_text();id='module.parameter.expressions.brace-policy'
if '['+'Q · '+id+']' not in s:
 s=s.replace('## Metadata',f'''<a id="{id}"></a>
[Q · {id}]

Should a `$$$macro` expand when the attribute contains no opening brace?

- Evidence: `%Evaluate` returns early on no `{{`; native E15/P4 leave the macro literal. The expression documentation lists macros without this gate.
- Working assumption: keep the observed brace gate until a maintainer chooses otherwise.
- Alternative: expand macros with or without a brace.
- Affected claims: parameter evaluation, after-install message, file-copy target and Invoke arguments. Alias C:Q2; parent [module.parameter.expressions](#module.parameter.expressions).

## Metadata''',1)
 s+=f'\n### {id}\n\n- agent-provided\n- user-facing-behavior\n- Expands: [expressions](#module.parameter.expressions).\n\nEvidence: C:Q2; Storage/Module.cls:1833-1841; E15/P4; new attr_evaluator.rs.\n'
 s=s.replace('C:Q2 asks whether brace-free macros should expand;','[C:Q2](#'+id+') asks whether brace-free macros should expand;');p.write_text(s)
# Registers link to canonical owners; inherited baseline registers remain unchanged.
p=HERE.parent/'QUESTIONS.md';s=p.read_text().replace('concepts/modules.g5.md#module.parameter.expressions)','concepts/modules.g5.md#module.parameter.expressions.brace-policy)')
if '## Extension audit' not in s:s+='\n## Extension audit\n\n[Question audit](extension/question-audit.json) retains all inherited aliases. The owning `.g5/ontology` blocks carry settled facts, working assumptions and alternatives. No maintainer decision was inferred from a passing check. LockManager constructor release mode stays open without a native real-boundary distinction.\n'
p.write_text(s)
rows=[]
for p in ONT.rglob('*.g5.md'):
 for m in re.finditer(r'^\[([IDQ]) · ([^\]]+)\]\n(.*?)(?=\n<a id=|\n## |\Z)',p.read_text(),re.M|re.S):
  if m[1]!='Q' and m[2] not in ['history.committed.side-effects','history.dependencies.parent-rollback','transaction.rollback-scope']:continue
  text=m[3].strip();rows.append({'id':m[2],'path':str(p.relative_to(ROOT)),'status':'open-policy-or-environment' if m[1]=='Q' else 'resolved-meaning-alias-preserved','question':text.splitlines()[0],'working_assumption_present':bool(re.search(r'Provisional|Working assumption|working assumption',text)),'alternatives_present':'Alternative' in text,'evidence_location':'owning block text plus same-ID Metadata in canonical file','audit_scope':'Canonical question form and linked evidence reviewed; no fresh maintainer assent claimed'})
(HERE/'question-audit.json').write_text(json.dumps(rows,indent=2));print(len(rows),'canonical question dispositions')
