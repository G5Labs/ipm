#!/usr/bin/env python3
"""Keep backward anchors while rendering canonical/retired entries distinctly."""
from pathlib import Path

if (Path(__file__).resolve().parents[4] / ".g5/Verus_verification/results/viewer-index.jsonl").exists():
    raise SystemExit("Historical register generator. Use the sealed iteration 28 revision to reproduce the audit.")

import json,re,os
H=Path(__file__).resolve().parent;R=H.parents[3]
bugs=json.loads((H/'bugs.json').read_text())
bugfiles={'C':'.g5/uplift/consolidated/BUGS.md','G':'.g5/Verus_verification/bugs_issues.md','P':'playbook-uplift/BUGS.md'}
designfiles={'C':'.g5/uplift/consolidated/DESIGN-SIMPLIFICATIONS.md','G':'.g5/Verus_verification/design_simplification.md','P':'playbook-uplift/DESIGN-SIMPLIFICATIONS.md'}
def slug(s):return re.sub(r'[^\w\- ]','',s.lower()).replace(' ','-')
def aliases(s,old,valid,arm,design=False):
 # Inline comments let historical HTML fragment aliases coexist with one parsed
 # entry anchor. They are never additional editable findings.
 s=re.sub(r'^<a id="([^"]+)"></a>\s*$',lambda m:m[0] if m[1] in valid else '',s,flags=re.M)
 for head in re.findall(r'^#{2,3} (.+)$',old,re.M):
  a=slug(head)
  if not a or f'id="{a}"' in s:continue
  m=re.match(r'((?:D|S|I)\d+|OBS-[LH]\d+|OOS-\d+)',head)
  target=(arm.lower()+'-'+m[1].lower()) if m else None
  if target and target in valid:
   marker='<a id="'+target+'"></a>'
   s=s.replace(marker,'<a id="'+a+'"></a><!-- historical fragment alias -->\n'+marker,1)
  else:s='<a id="'+a+'"></a><!-- historical section alias -->\n'+s
 return s
for arm,path in bugfiles.items():
 p=R/path;s=p.read_text();s=re.sub(r'^<a id="[^"]+"></a><!-- historical [^\n]* -->\n','',s,flags=re.M);valid={b['anchor'] for b in bugs if b['arm']==arm}
 # Redirect/scope entries carry no active decision and no discarded claim text.
 def convert(m):
  a,line=m[1],m[2]
  if a not in valid or line.startswith('#'):return m[0]
  id=next(b['id'] for b in bugs if b['anchor']==a)
  return f'<a id="{a}"></a>\n### {id} review record\n\nSeverity: Unranked\n\n'+line
 s=re.sub(r'^<a id="([^"]+)"></a>\n([^\n]+)',convert,s,flags=re.M)
 s=s.replace('Only the heading entries below are active functional findings.','Entries with Decision: Open are active; review and scope records are not active functional findings.')
 # Sort only active entries, leaving review/scope sections after them.
 at=s.index('## Outside active reproduction scope')
 active=s[:at];tail=s[at:]
 matches=list(re.finditer(r'^<a id="([^"]+)"></a>\n### ',active,re.M))
 if matches:
  prefix=active[:matches[0].start()];entries=[active[m.start():matches[i+1].start() if i+1<len(matches) else len(active)].strip() for i,m in enumerate(matches)]
  order={'Critical':0,'High':1,'Medium':2,'Low':3,'Unranked':4}
  entries.sort(key=lambda x:order.get(re.search(r'^Severity: (\w+)',x,re.M)[1],4))
  s=prefix+'\n\n'.join(entries)+'\n\n'+tail
 s=aliases(s,(H/'sealed'/path).read_text(),valid,arm)
 p.write_text(s.rstrip()+'\n')

items=json.loads((H/'simplifications.json').read_text())
for arm,path in designfiles.items():
 p=R/path;s=p.read_text();s=re.sub(r'^<a id="[^"]+"></a><!-- historical [^\n]* -->\n','',s,flags=re.M);split=s.index('## Withdrawn simplification claims');s=s[:split]+'## Withdrawn simplification claims\n\n'
 valid={x['id'].lower().replace(':','-') for x in items if x['arm']==arm}
 for x in items:
  if x['arm']!=arm or x['disposition'].startswith('repair-only'):continue
  a=x['id'].lower().replace(':','-')
  s+=f'<a id="{a}"></a>\n### {x["id"]} comparison review\n\nSeverity: Unranked\n\nThe simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations]({os.path.relpath(H/"SIMPLIFICATION-REVIEW.md",p.parent)}#{a}).\n\n'
 s=aliases(s,(H/'sealed'/path).read_text(),valid,arm,True)
 p.write_text(s.rstrip()+'\n')
print('Formatted six canonical finding/proposal registers and preserved historical fragments.')
