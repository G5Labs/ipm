#!/usr/bin/env python3
from pathlib import Path
import re,json,subprocess,os,hashlib
H=Path(__file__).resolve().parent;R=H.parents[3]
def move_links(text,source,dest):
 def sub(m):
  label,url=m.group(1),m.group(2)
  if re.match(r'[a-z]+:',url) or url.startswith('/') or ' ' in url:return m[0]
  part,mark,frag=url.partition('#')
  path=source if not part else (source.parent/part).resolve()
  return '['+label+']('+os.path.relpath(path,dest.parent)+('#'+frag if mark else '')+')'
 return re.sub(r'\[([^\]\n]+)\]\(([^)]+)\)',sub,text)

def save(p,s):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s.rstrip()+'\n')
def relative(p,f):return os.path.relpath(p,f.parent)
def snap(p):
 q=H/'before'/p.relative_to(R)
 if not q.exists():q.parent.mkdir(parents=True,exist_ok=True);q.write_bytes(p.read_bytes())
def meta_append(file,id,text):
 p=R/'.g5/ontology'/file;s=p.read_text();snap(p)
 m=re.search(r'^### '+re.escape(id)+r'\s*\n',s,re.M)
 if not m:return
 e=re.search(r'^### ',s[m.end():],re.M);end=m.end()+e.start() if e else len(s)
 if 'findings-audit-iter28/QUESTION-REVIEW.md' in s[m.end():end]:return
 s=s[:end].rstrip()+'\n\n'+text+'\n\n'+s[end:];save(p,s)

P={}
for path in sorted((R/'playbook-uplift/decisions').glob('*.md')):
 snap(path)
 for line in (H/'before'/path.relative_to(R)).read_text().splitlines():
  m=re.match(r'\| (DEC-[A-Z]+-\d+) \|',line)
  if m:
   cells=line.strip('|').split(' | ')
   P[m[1]]={'id':m[1],'source':str(path.relative_to(R)),'row':line,'question':cells[2].strip(),'options':cells[3].strip(),'old_status':cells[-1].strip()}
assert len(P)==44,len(P)
assumptions={}
for line in (H/'sealed/playbook-uplift/QUESTIONS.md').read_text().splitlines():
 m=re.match(r'\| (DEC-[A-Z]+-\d+) \|',line)
 if m:
  cells=line.strip('|').split(' | ');assumptions[m[1]]={'assumption':cells[1].strip(),'reason':cells[2].strip(),'alternative':cells[3].strip()}

specs={
 'DEC-INST-2':('features/update.g5.md','update.steps.dev-seeding-policy','Should a developer-mode install seed update-step records?'),
 'DEC-INST-6':('features/update.g5.md','update.steps.busy-policy','Should an update whose step lock is busy fail or report success?'),
 'DEC-INST-8':('features/update.g5.md','update.steps.fresh-seeding-policy','Should a dependency first installed by update run its old steps or seed them?'),
 'DEC-REPO-3':('concepts/repositories.g5.md','repo.oras.credentials-change-policy','Should changing an ORAS registry address clear its stored credentials?'),
 'DEC-REPO-5':('concepts/repositories.g5.md','repo.oras.unpublish-platform-policy','Which platform tags should unpublish remove when only a version is named?'),
 'DEC-SYNC-1':('features/developer-tools.g5.md','devtools.sync.failed-import-policy','Should sync retry a failed compile or reload after its source is reverted?'),
 'DEC-NSP-6':('concepts/namespaces.g5.md','namespace.enable.source-ambiguity-policy','Which source owns mapped IPM when several namespace records name one code database?'),
 'DEC-NSP-4':('concepts/namespaces.g5.md','namespace.enable.community-failure-policy','What status should enable -community return when a component step fails?'),
 'DEC-SYNC-3':('features/developer-tools.g5.md','devtools.sync.aggregate-status-policy','Should sync without a module name return an error when a module fails?'),
 'DEC-SYNC-4':('concepts/lifecycle.g5.md','lifecycle.empty-reload-policy','Should standalone reload warn or fail when a module has no source root?'),
 'DEC-DEP-3':('features/dependencies.g5.md','deps.lockfile.local-supplier-policy','Should a lock file accept a directory-loaded dependency with no registry?'),
 'DEC-UNIN-3':('features/uninstall.g5.md','uninstall.dependents.scoped-policy','Should a phase-scoped dependency block uninstall?'),
 'DEC-LOAD-1':('features/history.g5.md','history.committed.terminal-choice-policy','What history outcome is required when an operator commits a stopped load at the terminal?'),
}
existing={
 'DEC-INST-3':('concepts/transactions-and-dev-mode.g5.md','dev-mode.overwrite-guard.dependency-policy'),
 'DEC-NSP-5':('features/load.g5.md','load.refusal.requirements.timing'),
 'DEC-HIST-4':('features/history.g5.md','history.committed.nested'),
 'DEC-HIST-6':('features/history.g5.md','history.delete.running-records'),
 'DEC-INST-1':('features/load.g5.md','load.refusal.update-required.dev-bypass'),
 'DEC-INST-4':('features/install.g5.md','reinstall.version'),
 'DEC-LOAD-2':('concepts/transactions-and-dev-mode.g5.md','dev-mode.sticky'),
}
for id,(file,qid,title) in specs.items():
 p=R/'.g5/ontology'/file;s=p.read_text();snap(p)
 if f'[Q · {qid}]' not in s:
  a=dict(assumptions[id])
  a['alternative']=a['alternative'].replace(', changelog #1128','').replace(', issue #962','')
  block=f'<a id="{qid}"></a>\n[Q · {qid}]\n\n{title}\n\n- Working assumption: {a["assumption"]}\n- Alternative and implications: {a["alternative"]}\n- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.\n\n'
  s=s.replace('## Metadata',block+'## Metadata',1)
  parent=qid.rsplit('.',1)[0]
  s+=f'\n### {qid}\n\n- agent\n- uncertain\n- user-facing-behavior\n- Expands: [{parent}](#{parent})\n\nSources: [{id}]({relative(R/P[id]["source"],p)}); {a["reason"]}\n'
  save(p,s)
 existing[id]=(file,qid)

# A reproduced leftover declaration does not settle upgrade ownership policy.
p=R/'.g5/ontology/concepts/modules.g5.md';snap(p);s=p.read_text();qid='module.webapp.upgrade.policy'
if f'[Q · {qid}]' not in s:
 s=s.replace('## Metadata',f'<a id="{qid}"></a>\n[Q · {qid}]\n\nShould a higher-version load remove a web application omitted by the new manifest?\n\n- Working assumption: disclose that the application remains; do not promise automatic removal without an ownership rule.\n- Alternatives: remove resources formerly owned by the module, or require an explicit migration. Automatic removal needs ownership and administrator-edit checks; explicit migration leaves cleanup to the author.\n- An owner decision covering dropped resources, administrator changes and later uninstall would settle the policy. C:D26 observed a load, not update.\n\n## Metadata',1)
 s+=f'\n### {qid}\n\n- agent\n- uncertain\n- user-facing-behavior\n- Expands: [Upgrade behavior](#module.webapp.upgrade)\n\nSources: C:D26; [starting evidence]({relative(H/"sealed/.g5/uplift/consolidated/BUGS.md",p)}).\n';save(p,s)

# Collect each canonical question and the exact metadata already governing it.
records=[]
production={
 'history':['src/cls/IPM/General/History.cls','src/cls/IPM/General/HistoryTemp.cls','src/cls/IPM/General/AbstractHistory.cls'],
 'version':['src/cls/IPM/General/SemanticVersionExpression/Range.cls','tests/unit_tests/Test/PM/Unit/SemanticVersion/Expressions.cls'],
 'repo':['src/cls/IPM/Repo/Manager.cls','src/cls/IPM/Repo/Filesystem/Definition.cls','src/cls/IPM/Repo/Oras/Definition.cls'],
 'artifact':['src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls'],
 'update':['src/cls/IPM/General/Update/VersionBase.cls','src/cls/IPM/Storage/Module.cls','tests/integration_tests/Test/PM/Integration/Update.cls'],
 'devtools':['src/cls/IPM/Main.cls','src/cls/IPM/General/Sync/Pipeline.cls','tests/integration_tests/Test/PM/Integration/Sync.cls'],
 'module':['src/cls/IPM/Storage/Module.cls','src/cls/IPM/Storage/ModuleSetting/Default.cls','src/cls/IPM/ResourceProcessor/WebApplication.cls'],
 'default':['src/cls/IPM/Main.cls','src/cls/IPM/Utils/Module.cls','src/cls/IPM/Lifecycle/Base.cls']}
for p in sorted((R/'.g5/ontology').rglob('*.md')):
 s=p.read_text();body=s.split('## Metadata')[0]
 for m in re.finditer(r'\[Q · ([^\]]+)\]\s*\n(.*?)(?=\n<a id=|\Z)',body,re.S):
  id=m[1];text=m[2].strip();mm=re.search(r'^### '+re.escape(id)+r'\s*\n(.*?)(?=^### |\Z)',s,re.M|re.S)
  metadata=mm[1].strip() if mm else ''
  metadata='\n'.join(line for line in metadata.splitlines() if not (line.startswith('Findings audit:') and 'findings-audit-iter28/QUESTION-REVIEW.md' in line))
  group=id.split('.')[0];paths=production.get(group,production['default'])
  paths=[x for x in paths if (R/x).exists()]
  history=subprocess.run(['git','log','-4','--format=%h %s','571e04a1','--',*paths],cwd=R,capture_output=True,text=True).stdout.strip()
  tests=[x for x in paths if x.startswith('tests/')]
  record={'id':id,'path':str(p.relative_to(R)),'canonical_text':text,'metadata':metadata,'code_and_tests_read':paths,'history':history,'status':'open; no new maintainer determination','security_scope':id.startswith('artifact') or any(t in id for t in ['credentials','integrity.policy','unknown-descendants','target-policy'])}
  records.append(record)
assert len(records)==63,len(records)
save(H/'questions.json',json.dumps(records,indent=2))
review=H/'QUESTION-REVIEW.md';lines=['# Question and source audit','',
 'All 63 current canonical Q blocks were traced (49 inherited and 14 newly restored/created). The 44 playbook decision rows include 20 assumptions: 17 concern product policy and three concern method or an unconfirmed observation. An assumption is open even when a builder called it resolved. The inherited 31 G IDs and 22 C IDs remain indexed. C:Q23 records the dropped-resource policy rather than upgrading C:D26 into an accepted obligation.','',
 'Source review reads the canonical block, its same-ID metadata, source/comments, repository tests/docs and pinned git history listed for each entry. Relevant public discussion was checked separately in UPSTREAM.md. Current code demonstrates behavior; merge/test history can support inferred intent without proving a maintainer chose an unstated edge-case policy. No new owner approval was found. Security questions receive defensive source/existing-evidence review only.','']
for x in records:
 lines += [f'<a id="{x["id"]}"></a>',f'### {x["id"]}','',f'Canonical: [{x["id"]}]({relative(R/x["path"],review)}#{x["id"]})','',move_links(x['canonical_text'],R/x['path'],review),'', 'Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.','', 'Inspected production/test paths: '+', '.join('`'+p+'`' for p in x['code_and_tests_read'])+'.','', 'Pinned history:', '', '```text',x['history'] or '(No path history in the available checkout.)','```','', 'Existing attributable evidence:', '',move_links(x['metadata'],R/x['path'],review),'']
lines += ['## Opening-arm decision audit','']
for id,x in P.items():
 status='open; working assumption' if id in assumptions else 'inherited source-supported interpretation; not new owner approval'
 if id in {'DEC-INST-1','DEC-INST-4','DEC-LOAD-2'}:status='open; conflicting canonical question takes precedence over the resolved label'
 if id in {'DEC-SYS-1','DEC-SEM-1','DEC-SEM-2'}:status='method/verification observation; not an unresolved IPM policy decision'
 if id in existing:
  file,q=existing[id];x['canonical_owner']={'path':'.g5/ontology/'+file,'id':q}
 x['audit_status']=status
 lines += [f'<a id="{id.lower()}"></a>',f'### {id}','',f'Disposition: {status}.','',x['row'],'']
 if 'canonical_owner' in x:lines += ['Canonical: ['+x['canonical_owner']['id']+']('+relative(R/x['canonical_owner']['path'],review)+'#'+x['canonical_owner']['id']+').','']
save(review,'\n'.join(lines));save(H/'playbook-decisions.json',json.dumps(list(P.values()),indent=2))

for x in records:
 file=str(Path(x['path']).relative_to('.g5/ontology'))
 text=f'Findings audit: [sources, alternatives, implications and settlement gate]({relative(review,R/x["path"])}#{x["id"]}). The recorded assumption is provisional; no new maintainer approval was found.'
 meta_append(file,x['id'],text)

# Keep old decision discussions verbatim; correct only the status that misrepresented an assumption.
for path in sorted((R/'playbook-uplift/decisions').glob('*.md')):
 s=path.read_text();lines=[]
 for line in s.splitlines():
  m=re.match(r'\| (DEC-[A-Z]+-\d+) \|',line)
  if m and m[1] in P:
   id=m[1];status=P[id]['audit_status']
   if id in assumptions or id in {'DEC-INST-1','DEC-INST-4','DEC-LOAD-2'}:
    cells=line.split('|');cells[-2]=' '+status+' ';line='|'.join(cells)
  lines.append(line)
 s='\n'.join(lines)
 if '## Findings audit status' not in s:s+='\n\n## Findings audit status\n\nThe table status above supersedes older resolution wording where an assumption was called resolved. Historical rationale is preserved. [Canonical owner crosswalk]('+relative(review,path)+'#opening-arm-decision-audit) distinguishes source support, behavior and owner decisions.\n'
 save(path,s)

GIDS=['dev-mode.sticky','load.refusal.update-required.dev-bypass','dev-mode.overwrite-guard.force','transaction.rollback-scope.policy','history.committed.nested','history.dependencies.parent-rollback','history.committed.side-effects','history.record.abnormal-end','history.record.unlogged-refusals','history.read.filter.invalid-column','history.delete.confirm-with-filter','history.delete.global-unfiltered','history.retention.effect','history.phases.returned-errors','history.fields.dependency-source','history.keep-on-ipm-uninstall.other-namespaces','load.refusal.requirements.timing','history.read.filter.default-modifiers','reinstall.version','history.delete.running-records','history.locked.unfinished','transaction.flag-sources.dependency-defaults','transaction.flag-sources.dependency-no-transaction','dev-mode.overwrite-guard.dependency-policy','transaction.flag-sources.dependency-closure','history.committed.escaped-dependency','history.delete.global-errors','history.read.global-shape','history.retention.running-policy','history.survives-rollback.repair-policy','history.committed.marking.refusal-policy']
lookup={}
for p in (R/'.g5/ontology').rglob('*.md'):
 for id in re.findall(r'\[[IDQ] · ([^\]]+)\]',p.read_text()):lookup[id]=p
qfile=R/'.g5/Verus_verification/open_questions.md';out=['# IPM guidance-arm question crosswalk','', 'The canonical ontology owns the question. This audit corrects stale resolved-by-assumption and no-owner-block labels; it does not manufacture approval. [Full source audit]('+relative(review,qfile)+').','', '| Starting ID | Canonical owner | Current disposition |','|---|---|---|']
for i,id in enumerate(GIDS,1):
 status='open' if id in {x['id'] for x in records} else 'settled meaning; inherited attributed source preserved'
 if i==4:status='rollback mechanics settled; compensation policy remains open'
 out.append(f'| G:Q-{i:02} | [{id}]({relative(lookup[id],qfile)}#{id}) | {status} |')
out += ['', 'G:Q-06b remains an alias of Q-05; G:Q-FIN remains an alias of Q-21. Q-06 and Q-07 no longer appear as unresolved merely because old prose lagged the canonical blocks. Q-12 remains the documented global-delete decision. Original source research remains in [the sealed register]('+relative(H/'sealed/.g5/Verus_verification/open_questions.md',qfile)+').'];save(qfile,'\n'.join(out))

qfile=R/'playbook-uplift/QUESTIONS.md';out=['# IPM playbook-arm question and evidence crosswalk','', 'The former assertion that no intent question remains open is withdrawn. The 20 assumed resolutions are provisional, with 17 product-policy assumptions routed to their canonical Q owners. Three method/verification items are tracked separately. Three evidence-labeled rows also conflict with canonical open questions. [Full source and owner audit]('+relative(review,qfile)+').','', '| Starting decision | Audit status | Owner/evidence |','|---|---|---|']
for id,x in P.items():
 owner=x.get('canonical_owner');href=relative(R/owner['path'],qfile)+'#'+owner['id'] if owner else relative(review,qfile)+'#'+id.lower();label=owner['id'] if owner else 'attributed source and limits'
 out.append(f'| {id} | {x["audit_status"]} | [{label}]({href}) |')
out += ['', '## Verification items V1–V18','', 'These are evidence limits, not automatic policy choices. All 18 remain individually accounted for in [the preserved register]('+relative(H/'sealed/playbook-uplift/QUESTIONS.md',qfile)+'#still-unresolved). V4 is partly strengthened by G:D13’s directory-repository failure; archive cleanup and analytics remain unobserved. V6 has only a substituted helper witness (C:D19). V7–V12 retain their formal premises and scope. Mirrors/real legacy installs/other platforms remain environment or coverage gaps. V17 is a severity judgment and V18 is a missing independent sign-off; neither is owner approval.'];save(qfile,'\n'.join(out))

qfile=R/'.g5/uplift/consolidated/QUESTIONS.md';s=(H/'sealed/.g5/uplift/consolidated/QUESTIONS.md').read_text();s+='\n\n## Findings audit against iteration 28\n\nAll C:Q1–C:Q22 were traced to their actual canonical owners; the source and settlement gate are in [the question review]('+relative(review,qfile)+'). No passing observation ratifies policy. Opening-arm assumptions and aliases are now reconciled in the G/P crosswalks. C:Q14 and the archive/target/integrity security questions remain outside active reproduction scope.\n\nC:Q23: [Should a higher-version load remove an omitted web application?](../../ontology/concepts/modules.g5.md#module.webapp.upgrade.policy) C:D26 observed retention after load; it did not establish update behavior or owner approval.\n';save(qfile,s)
print('canonical Q:',len(records),'playbook decisions:',len(P),'new canonical Q:',len(specs)+1)
