#!/usr/bin/env python3
"""Pinned census plus separately recorded manual source review. No coverage credit."""
import csv
import hashlib
import json
import re
import subprocess
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
PIN = '571e04a19aba9f9b4bc0ea23fbe09182038f4e55'

def git(*args):
    return subprocess.check_output(['git', '-C', str(REPO), *args], text=True)

def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()

def dump(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + '\n')

rows = {}
files = [p for p in git('ls-tree','-r','--name-only',PIN,'--','src/cls/IPM').splitlines() if p.endswith('.cls')]
source_text = {}
for path in files:
    text = git('show', f'{PIN}:{path}')
    source_text[path] = text
    member = None
    for i, line in enumerate(text.splitlines(), 1):
        match = re.match(r'^(ClassMethod|Method|Query|Trigger)\s+("[^"]+"|[\w%]+)', line, re.I)
        if match:
            member = path.removeprefix('src/cls/IPM/') + '::' + match[2]
            rows.setdefault(member, {'kind':match[1], 'start':i, 'end':i, 'no_body':line.rstrip().endswith(';'), 'path':path})
        elif re.match(r'^(?:Class|ClassMethod|Method|Query|Trigger|Index|Parameter|Property|Relationship|Projection|ForeignKey|XData|Storage)\s',line,re.I):
            member = None
        if member:
            rows[member]['end'] = i
        if line.startswith('}') or (member and line.rstrip().endswith(';') and match):
            member = None
assert len(rows) == 1216
baseline_path = REPO/'.g5/uplift/consolidated/extension/member-dispositions.json'
baseline = {x['member']: x for x in json.loads(baseline_path.read_text())}
assert set(rows) == set(baseline)
represented = sorted(m for m,b in baseline.items() if b['disposition'] in {'owns','delegates'})
assert len(represented) == 174
assert sum(baseline[m]['disposition']=='owns' for m in represented)==165
assert sum(baseline[m]['disposition']=='delegates' for m in represented)==9
notes_path=HERE/'review-notes.tsv'
notes={n['member']: n for n in csv.DictReader(notes_path.open(),delimiter='\t')}
assert len(notes)==76
assert set(notes)<=set(rows)
source_slices=HERE/'reviewed-source.txt'
chunks=[]
for m,r in sorted(rows.items()):
    lines=source_text[r['path']].splitlines()
    raw='\n'.join(lines[r['start']-1:r['end']])+'\n'
    r.update(member=m, pin=PIN, source_sha256=sha(raw), file_sha256=sha(source_text[r['path']]), source_slice=f"{r['path']}:{r['start']}-{r['end']}")
    b=baseline[m]
    r.update(baseline_disposition=b['disposition'], baseline_represented=m in represented)
    n=notes.get(m)
    if n:
        r['source_review']=n
        r['review_status']='source-reviewed'
        chunks.append('\n'+m+' '+r['source_slice']+'\n'+raw)
    else:
        r['review_status']='not-resampled-in-iter2' if m in represented else 'still-unclassified'
source_slices.write_text(''.join(chunks))
dump(HERE/'inventory.json', {'pin':PIN,'counting_rule':'frozen coverage/members.py ClassMethod|Method|Query|Trigger declaration map; all original 1216 retained','members':list(rows.values())})
dump(HERE/'baseline-reach.json', {'pin':PIN,'baseline_dispositions_sha256':sha(baseline_path.read_text()),'direct_members':sorted(m for m in represented if baseline[m]['disposition']=='owns'),'shared_only_members':sorted(m for m in represented if baseline[m]['disposition']=='delegates'),'represented_members':represented,'outside_members':sorted(set(rows)-set(represented))})
uncovered=[r for m,r in rows.items() if m not in represented]
reviewed=[r for r in uncovered if r['review_status']=='source-reviewed']
strata=defaultdict(list)
for r in reviewed: strata[r['source_review']['stratum']].append(r['member'])
summary={'pin':PIN,'denominator':1216,'baseline_represented':174,'baseline_direct':165,'baseline_shared_only':9,'outside_accepted_reach':1042,'outside_source_reviewed':len(reviewed),'outside_still_unclassified':sum(r['review_status']=='still-unclassified' for r in uncovered),'outside_dispositions':dict(Counter(r['source_review']['disposition'] for r in reviewed)),'source_reviewed_total':76,'old_credit_challenged':['Lifecycle/Base.cls::%Publish'],'strata':dict(strata),'coverage_credit_from_this_triage':0,'new_representation_requires':'explicit scoped Verus symbol, reachable contrast, discriminating bad variant, solver verdict and source/final-reader correspondence; parent integrates only earned union','selection':{'method':'Independent reviewer purposive source-first strata, not random probability sample. No fresh evaluator case access and no model source inspection. Iter1 disposition ids used only to find gaps.','reason':'Prioritize destructive or externally visible operations, registry ordering/integrity/auth, publication partial effects, dependency removal, final reporting readers, test ownership and configuration/FS providers. Include small helpers to distinguish delegation from owned decisions.','seed':'none: purposive reasons recorded per stratum and member','stratum_frame':['registry-integrity','publication','verify','inspection','test-sync','filesystem-safety','settings','filesystem-repository','uninstall'],'exclusions':['Generated/nonmember declarations excluded by unchanged original census rule, not removed from product existence.','Other 967 outside members remain unclassified, not declared low risk or irrelevant.','Base constants explicitly marked justified-exclusion for independent deep behavior model only; remain in 1216 denominator.','Previously represented 173 other members were not independently resampled.']},'authority':{'product':'Pinned implementation as-built only; comments and local README/CHANGELOG/history are evidence of recovered intent, never owner ratification.','reviewed_docs':['README.md:65-105 operator install/uninstall/repository mapping','CHANGELOG.md:13-43 #1135/#300/#1115/#1281/#1269','CHANGELOG.md:210-243 #058/#908/#688','git log -15 --oneline PIN -- Repo/Oras ModuleSummary ResourceProcessor/Test'],'human':'Owner priorities, disputed desired policy and human readability unmeasured.'},'artifacts':{'notes_sha256':sha(notes_path.read_text()),'reviewed_source_sha256':sha(source_slices.read_text())}}
assert len(reviewed)==75
assert summary['outside_still_unclassified']==967
dump(HERE/'summary.json',summary)
print(json.dumps({k:v for k,v in summary.items() if k not in {'strata','selection','authority','artifacts'}},indent=2))
