#!/usr/bin/env python3
"""Join existing inventories. Counts represent a scoped observable distinction, never an entire method proof.
Unresolved rows stay in every denominator. No citation is promoted into a formal behavior edge.
"""
from pathlib import Path
import csv,json,re,hashlib,subprocess,collections,sys
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];VV=ROOT/'.g5/Verus_verification/consolidated';PIN='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
def sha(b):return hashlib.sha256(b).hexdigest()
def read(p):return Path(p).read_text(errors='replace')
def save(name,obj): (HERE/name).write_text(json.dumps(obj,indent=2,sort_keys=True)+'\n')
MEM={r['member']:r for r in csv.DictReader(open(HERE.parent/'coverage/members.tsv'),delimiter='\t')}
SOURCES={p:subprocess.check_output(['git','show',PIN+':src/cls/IPM/'+p]).decode() for p in sorted({m.split('::')[0] for m in MEM})}
for m,r in MEM.items():
 lines=SOURCES[m.split('::')[0]].splitlines(keepends=True);r['source_sha256']=sha(''.join(lines[int(r['start'])-1:int(r['end'])]).encode())
IDS={}
for p in (ROOT/'.g5/ontology').rglob('*.g5.md'):
 for id in re.findall(r'^\[[IDQ] · ([^\]]+)\]',read(p),re.M):IDS[id]=str(p.relative_to(ROOT))
def governing(m):
 f,n=m.split('::');pairs=[('General/History','history'),('General/AbstractHistory','history.fields'),('General/Environment','module.env'),('General/SemanticVersionExpression','version.range'),('General/SemanticVersion','version'),('General/TempLocal','load.dependencies.local-repo'),('Repo/Filesystem','repo.filesystem'),('Repo/UniversalSettings','settings'),('Repo/','repo'),('General/Sync/','devtools.sync'),('Storage/FileHash','devtools.sync'),('Utils/JournalManager','transaction.opt-out'),('Utils/LockManager','load.core'),('ResourceProcessor/FileCopy','module.processor.file-copy'),('ResourceProcessor/','module.resource'),('Storage/ModuleSetting','module.parameter.expressions'),('Lifecycle/','lifecycle'),('StudioDocument/Module','module'),('General/Update/','update'),('Storage/SystemRequirements','module.requirement')]
 exact={'SetCommitted':'history.committed.marking','InitPhase':'history.phases','FinalizePhase':'history.phases','GetMapInfo':'namespace','IPMInstalled':'namespace','IsIPMEnabled':'namespace','GetListModules':'namespace','AddPackageMapping':'namespace','RemovePackageMapping':'namespace','RemoveGlobalMapping':'namespace','PerformBatchActivation':'namespace','GetDefaultValue':'settings','PrintOne':'settings','SortOrder':'repo.manage','PublishingServerFetch':'repo.default-registry','GetAddUpdateStep':'update','MarkFailed':'devtools.sync','MarkTestPending':'devtools.sync','SyncReportUnroutedPaths':'devtools.sync','Update':'update','Install':'install','Uninstall':'uninstall','IsInScope':'module.resource.scope','ModuleVersion':'devtools.authoring.module-version','UpdateLanguageExtensions':'setup.language-extension','UpdateLanguageExtensionsOne':'setup.language-extension','History':'history','Config':'settings','Search':'repo.search','Sync':'devtools.sync','EnableIPM':'namespace','UnmapIPM':'namespace','Load':'load','LoadInternal':'load','LoadNewModule':'load.outcome','LoadDependencies':'load.dependencies','LoadModuleReference':'load.dependencies','BuildDependencyGraph':'deps','HasScope':'module.resource.scope'}
 candidate=exact.get(n) or next((b for a,b in pairs if f.startswith(a)),'ipm')
 return candidate if candidate in IDS else 'ipm'
def sympaths(model_ref,arm):
 out=[]
 for path,tail in re.findall(r'([\w/.-]+\.rs)::([^;]+)',model_ref):
  p=ROOT/path if path.startswith('playbook-uplift/') else ROOT/'.g5/Verus_verification'/path
  if not p.exists():continue
  names=set(re.findall(r'\b(?:fn|struct|enum)\s+(\w+)',read(p)))
  # The initial explicitly named function; following comma-separated exact identifiers are added only when they resolve.
  for tok in re.findall(r'\b\w+\b',tail):
   if tok in names:out.append(str(p.relative_to(ROOT))+'::'+tok)
 return list(dict.fromkeys(out))
EDGES=[]
def edge(m,artifact,role,ontology,symbol=None,prop=None,premises=None,native=None,origin=None,status=None,slice_=None):
 r=MEM[m];p,a,b=(slice_.split(':')[0],*re.findall(r'\d+',slice_.split(':')[-1])) if slice_ else (m.split('::')[0],r['start'],r['end'])
 assert p==m.split('::')[0] and int(r['start'])<=int(a)<=int(b)<=int(r['end']),(m,slice_)
 data={'member':m,'pin':PIN,'slice':f'{p}:{a}-{b}','source_sha256':sha(''.join(SOURCES[p].splitlines(keepends=True)[int(a)-1:int(b)]).encode()),'artifact':artifact,'role':role,'ontology':ontology,'formal_symbol':symbol,'property':prop,'premises':premises or [],'native':native,'origin':origin,'status':status or 'linked-only'}
 EDGES.append(data)
BASE=set();OLD=set()
for arm in 'GP':
 for r in csv.DictReader(open(HERE.parent/f'baseline/{arm}-baseline.tsv'),delimiter='\t'):
  m=r['member']
  if m not in MEM:continue
  if r['qualifies']=='yes':OLD.add(m)
  syms=sympaths(r['model_ref'],arm)
  if r['qualifies']=='yes' and syms:
   BASE.add(m)
   for s in syms:edge(m,'Verus','delegates' if any(x in m for x in ['MapGlobalEquivalently','CompareOneFile','StampOneFileWithHash']) else 'owns',governing(m),s,r['outcome_distinguished'],[r['reason'],'Original model TRUST / PROJECTIONS premises apply; total version domain narrowed by C:D15.'],r['binding'],f'baseline-{arm}','represented-with-inherited-evidence')
  else:
   edge(m,'Python' if '.py::' in r['model_ref'] else 'trust-note','excludes' if re.search(r'\bexclud',r['reason'],re.I) else 'assumes',governing(m),None,r['reason'],[],r['binding'],f'baseline-{arm}',r['qualifies'])
# Preserve the old citation-only measure as a distinct artifact, including trust-note exclusions.
x=json.loads(read(ROOT.parent/'results/experiments/ipm-uplift-comparison-20261113/source-links.json'))
for kind,rows in x['linked_member_examples'].items():
 for m,r in rows.items():
  if m in MEM:edge(m,'ontology-citation' if 'ontology' in kind else 'model-citation','assumes',governing(m),None,'Citation at '+r['in']+':'+str(r['line']),origin=kind)
# Representation does not require old native-rule qualification. Independently audited inherited
# projections/normal-path actions are recovered into BEFORE, never counted as new verification.
audit_path=HERE/'inherited-representation-audit.json'
if audit_path.exists():
    for r in json.loads(read(audit_path)):
        if not r['represented']:continue
        m=r['member'];BASE.add(m)
        sl=r['source_slice'].replace('src/cls/IPM/','')
        edge(m,'Verus',r['role'],governing(m),r['symbol'],r['scope'],r['premises']+[r['missing']],None,'baseline-representation-audit','represented-not-necessarily-proved-or-native-checked',sl)

# Initial consolidation Python bindings are explicit links too, including uncredited members.
sys.path.insert(0,str(VV/'checks'));import framework
for p in sorted((VV/'models').glob('*.py')):
    if p.name.startswith('_'):continue
    model=framework.load_family(p);result=framework.check_family(model)
    for m,meta in model.MEMBERS.items():
        if m not in MEM:continue
        edge(m,'Python-function','owns' if result['members'][m]['credited'] else 'assumes',governing(m),None,meta.get('behavior',''),['Python oracle is independent evidence; not a Verus representation'],{'runs':model.RUNS},'initial-consolidation-Python','native-matched' if result['as_built_matches'] else 'native-mismatch',meta['slice'])
        EDGES[-1]['python_symbol']=str(p.relative_to(ROOT))+'::'+meta['fn']
BASE_EDGES=list(EDGES)
# New family manifests are checked, not inferred from Python function names.
OBL=[];OPEN=[]
for p in sorted((VV/'verus').glob('*.manifest.json')):
 x=json.loads(read(p));family=x.get('family',p.stem)
 for b in x.get('bindings',[]):
  m=b.get('member');symbol=b.get('symbol') or b.get('formal_symbol');formal=x.get('formal_file') or x.get('model') or str(p.with_suffix('.rs').name).replace('.manifest','')
  if m not in MEM:continue
  if symbol and '::' not in symbol:
   formal_p=Path(formal);formal_p=formal_p if formal_p.is_absolute() else (ROOT/formal if (ROOT/formal).exists() else VV/'verus'/formal_p.name)
   symbol=str(formal_p.relative_to(ROOT))+'::'+symbol
  ont=b.get('ontology',governing(m));ont=ont[0] if isinstance(ont,list) else ont
  prop=b.get('property') or b.get('behavior') or family
  # Open/unrepresented rows are disposition edges, not Verus member credits.
  represented=b.get('represented',True) and b.get('status','') not in ('open','unrepresented','not-represented') and bool(symbol)
  if represented:edge(m,'Verus',b.get('role','owns'),ont,symbol,prop,b.get('premises',[]),b.get('native'),family,b.get('status','represented'),b.get('slice'))
  else:OPEN.append({'member':m,'family':family,'gap':b})
 for o in x.get('obligations',[]):
  o=dict(o);o['family']=family;o['manifest']=str(p.relative_to(ROOT));o.setdefault('critical',True);OBL.append(o)
 OPEN.extend({'family':family,'gap':g} for g in x.get('open_boundaries',[]))
# Validate both endpoints and exact member slices. An invalid formal edge fails, not merely loses a count.
errors=[]
for e in EDGES:
 if e['ontology'] not in IDS:errors.append('ontology:'+str(e['ontology']))
 if e['formal_symbol']:
  path,s=e['formal_symbol'].rsplit('::',1);f=ROOT/path
  if not f.exists() or not re.search(r'\b(?:fn|struct|enum)\s+'+re.escape(s)+r'\b',read(f)):errors.append('symbol:'+e['formal_symbol'])
 if e.get('python_symbol'):
  path,s=e['python_symbol'].rsplit('::',1);f=ROOT/path
  if not f.exists() or not re.search(r'^def '+re.escape(s)+r'\(',read(f),re.M):errors.append('python-symbol:'+e['python_symbol'])
 e['id']='edge-'+sha(json.dumps(e,sort_keys=True,default=str).encode())[:16]
# Source-first caller/writer view independent of the intent inventory.
CALLERS=collections.defaultdict(list);WRITERS=[];TX=[]
for m,r in MEM.items():
 f=m.split('::')[0];body=''.join(SOURCES[f].splitlines(keepends=True)[int(r['start'])-1:int(r['end'])])
 for cl,method in re.findall(r'##class\(\%?IPM\.([\w.]+)\)\.(\w+)',body):
  target=cl.replace('.','/')+'.cls::'+method
  if target in MEM:CALLERS[target].append(m)
 for method in re.findall(r'\.\.([\w%]+)\s*\(',body):
  target=f+'::'+method
  if target in MEM:CALLERS[target].append(m)
 effects=[]
 for pattern,name in [(r'\btstart\b','transaction'),(r'\.(?:%Save|%DeleteId|%Delete|Write|Copy|CreateDirectory|DeleteFile|Put|Post)\s*\(','write-call'),(r'\b(?:set|kill|merge)\s+\^','global-write'),(r'\b(?:INSERT|UPDATE|DELETE)\b','SQL-write'),(r'\$zf\(','external-process')]:
  if re.search(pattern,body,re.I):effects.append(name)
 if effects:WRITERS.append({'member':m,'effects':effects,'discovery':'conservative lexical candidate; dynamic dispatch and generated SQL require investigation'})
 for i,line in enumerate(SOURCES[f].splitlines(),1):
  if int(r['start'])<=i<=int(r['end']) and re.match(r'^\s*tstart\b',line,re.I):TX.append({'member':m,'line':i,'source_sha256':r['source_sha256'],'caller_candidates':sorted(set(CALLERS.get(m,[])))})
assert len(TX)==18,len(TX)
# Existing 46 command write sets: last observation per name, same rule as inherited gate.
records=json.loads(read(ROOT/'playbook-uplift/evidence/iris-observations.json'))['records']
ws={r['name'][3:]:r for r in records if r['name'].startswith('ws-') and 'written' in r['values']}
handlers={'repo':'Repo','load':'Load','install':'Install','update':'Update','uninstall':'Uninstall','reinstall':'Reinstall','compile':'Compile','reload':'Reload','activate':'Activate','sync':'Sync','history':'History','config':'Config','enable':'EnableIPM','unmap':'UnmapIPM','module-version':'ModuleVersion','test':'Test','verify':'Verify','package':'Package','publish':'Publish','list-installed':'ListInstalled','version':'Version','show':'Show','search':'Search','orphans':'Orphans','default-modifiers':'DefaultModifiers'}
command_rows=[]
for name,r in ws.items():
 command=r['values']['cmd'];root=command.split()[0];handler=handlers.get(root);m='Main.cls::'+str(handler)
 command_rows.append({'observation':name,'command':command,'entry_member':m if m in MEM else None,'observed_write_set':r['values']['written'].split(','),'receipt':'playbook-uplift/'+r['log'],'status':r['values'].get('status'),'owner_resolution':'entry + semantic mechanism owner; dynamic dispatch not mechanically complete'})
assert len(command_rows)==46,len(command_rows)
# Join stores backward through the existing semantic mechanisms. This is owner discovery, not proof coverage.
sem=json.loads(read(ROOT/'playbook-uplift/semantics/semantics.json'))
for r in command_rows:
 touched={c for c,gs in sem['global_classes'].items() if any(g.replace('(killed)','') in gs for g in r['observed_write_set'])}
 r['store_classes']=sorted(touched);r['mechanism_owners']={k:v['where'] for k,v in sem['mechanisms'].items() if touched & set(v['owns'])}
# All P behavior/exception IDs remain in the existing inventory. Exact duplicate statements coalesce;
# semantic actor/precondition/event/result/state decompositions not reviewed are explicitly unresolved.
intent=[];groups=collections.defaultdict(list)
for p in sorted((ROOT/'playbook-uplift/ontology').glob('[0-9][0-9]-*.md')):
 for line in read(p).splitlines():
  if not re.match(r'^\| [A-Z]+-(?:B|N)\d+ \|',line):continue
  cols=[v.strip() for v in line.strip('|').split('|')]
  if len(cols)<7:continue
  id,parent,kind,statement,evidence,source,notes=cols[:7]
  key=re.sub(r'\s+',' ',statement.lower().replace('**','')).strip()
  groups[key].append(id)
  intent.append({'id':'P:'+id,'parent':'P:'+parent,'kind':kind,'statement':statement,'source_inventory':str(p.relative_to(ROOT)),'source':source,'evidence':evidence,'notes':notes,'dedup_key':key,'scope_disposition':'unresolved canonical decomposition','actor':None,'precondition':None,'event':None,'observable_result':statement,'preserved_state':None})
# Reconcile each existing command/state cell and intent row against the frozen member census.
for row in intent:
    found=[]
    for path,a,b in re.findall(r'(?:ipm/)?src/cls/IPM/([\w/%.]+\.cls)#L(\d+)(?:-L?(\d+))?',row['source']):
        a=int(a);b=int(b or a)
        for m,r in MEM.items():
            if m.split('::')[0]==path and int(r['start'])<=a<=int(r['end']):found.append(m)
    row['candidate_members']=sorted(set(found))
    row['candidate_canonical_owners']=sorted({governing(m) for m in found})
    row['binding_status']='source citation candidates only; no semantic equivalence inferred'
intent_by_id={r['id'][2:]:r for r in intent};cells=[]
for p in sorted((ROOT/'playbook-uplift/ontology').glob('[0-9][0-9]-*.md')):
    for line in read(p).splitlines():
        if not re.match(r'^\| [A-Z]+-C\d+ \|',line):continue
        cols=[v.strip() for v in line.strip('|').split('|')]
        id,state,command,outcome,decision=(cols+['']*5)[:5]
        refs=re.findall(r'[A-Z]+-[BN]\d+',outcome)
        cells.append({'id':id,'source_inventory':str(p.relative_to(ROOT)),'state':state,'command':command,'outcome_ids':['P:'+x for x in refs],'decision':decision,'candidate_members':sorted({m for x in refs for m in intent_by_id.get(x,{}).get('candidate_members',[])}),'status':'requires behavioral applicability review; original cell is preserved'})
assert len(cells)==389,len(cells)
save('command-state-reconciliation.json',cells)

# Every source member has a disposition. Open is a disposition, not completion of a behavioral audit.
linked={e['member'] for e in EDGES};represented={e['member'] for e in EDGES if e['artifact']=='Verus' and e['formal_symbol']}
roles={m:{e['role'] for e in EDGES if e['member']==m and e['artifact']=='Verus'} for m in represented}
DIS=[]
for m,r in MEM.items():
 es=[e['id'] for e in EDGES if e['member']==m]
 disposition='owns' if 'owns' in roles.get(m,set()) else 'delegates' if m in represented else 'unresolved'
 DIS.append({'member':m,**r,'disposition':disposition,'scope':'one or more scoped observable behaviors' if m in represented else 'source census only; full caller/intent audit incomplete','edges':es,'caller_candidates':sorted(set(CALLERS.get(m,[]))),'criticality':'unreviewed' if m not in represented else 'scoped property only','unresolved_reason':None if m in represented else 'No checked Verus observable-behavior binding. Citation and trust-note edges do not establish representation.'})
def breakdown(edges,field):
 out=collections.defaultdict(set)
 for e in edges:out[e[field]].add(e['member'])
 return {k:len(v) for k,v in sorted(out.items())}
byarea={}
for area in sorted({m.split('/')[0] if '/' in m.split('::')[0] else '(root)' for m in MEM}):
 ms={m for m in MEM if (m.split('/')[0] if '/' in m.split('::')[0] else '(root)')==area}
 byarea[area]={'denominator':len(ms),'linked':len(ms&linked),'verus_before':len(ms&BASE),'verus_after':len(ms&represented),'additions':len(ms&(represented-BASE)),'withdrawals':len(ms&(BASE-represented)),'unresolved':len(ms-represented)}
counts={'pin':PIN,'denominator':1216,'explicit_links':len(linked),'explicit_links_before':len({e['member'] for e in BASE_EDGES}),'linked_by_role':breakdown(EDGES,'role'),'linked_by_artifact':breakdown(EDGES,'artifact'),'verus_before':len(BASE),'verus_qualified_only_before':72,'inherited_representation_audit_additions':len(BASE)-72,'representation_measure':'Established lower bound from resolved observable-behavior bindings; remaining members unresolved, not proved absent.','verus_after':len(represented),'verus_direct':len({e['member'] for e in EDGES if e['artifact']=='Verus' and e['role']=='owns'}),'verus_shared_only':len({m for m in represented if 'owns' not in roles[m]}),'verus_additions':len(represented-BASE),'verus_withdrawals':len(BASE-represented),'member_dispositions':dict(collections.Counter(r['disposition'] for r in DIS)),'by_subsystem':byarea,'old_rule_baseline':len(OLD),'intent_behaviors':sum(r['kind']=='B' for r in intent),'intent_exceptions':sum(r['kind']=='N' for r in intent),'intent_distinct_exact_texts':len(groups),'semantic_dedup_unresolved':len(intent),'command_state_cells':len(cells),'command_write_sets':len(command_rows),'transaction_starts':len(TX),'new_scoped_obligations':len(OBL),'represented_obligations':sum(o.get('represented',False) is True for o in OBL),'proved_obligations':sum(o.get('proved',False) is True for o in OBL),'refuted_obligations':sum(o.get('refuted',False) is True for o in OBL),'open_obligations':sum(bool(o.get('open',False)) for o in OBL),'open_boundaries':len(OPEN),'critical_selected':sum(o.get('critical') is True for o in OBL),'critical_represented':sum(o.get('critical') is True and o.get('represented') is True for o in OBL),'critical_proved':sum(o.get('critical') is True and o.get('proved') is True for o in OBL),'critical_refuted':sum(o.get('critical') is True and o.get('refuted') is True for o in OBL),'critical_open':sum(o.get('critical') is True and bool(o.get('open')) for o in OBL),'critical_universe_disposition':'incomplete: no 100% critical representation claim','validation_errors':errors,'mechanical_product_body_proofs':0}
save('bindings.json',EDGES);save('before-verus-bindings.json',BASE_EDGES);save('member-dispositions.json',DIS);save('coverage.json',counts);save('obligations.json',OBL);save('open-boundaries.json',OPEN);save('intent-reconciliation.json',intent);save('transaction-starts.json',TX);save('command-write-owners.json',command_rows);save('writer-candidates.json',WRITERS)
reverse=collections.defaultdict(list)
for e in EDGES:
 if e['formal_symbol']:reverse[e['formal_symbol']].append({'member':e['member'],'ontology':e['ontology'],'edge_id':e['id']})
save('reverse-bindings.json',reverse)
print(json.dumps(counts,indent=1));assert not errors,errors
