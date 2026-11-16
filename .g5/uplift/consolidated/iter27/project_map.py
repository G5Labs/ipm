"""One generated view of preserved binding conventions; resolution is not semantic credit."""
from pathlib import Path
import argparse, collections, csv, hashlib, json, re

HERE=Path(__file__).resolve().parent
CONS=HERE.parent
ROOT=CONS.parents[2]
PIN='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def rel(p): return str(p.relative_to(ROOT))
def load(p, inputs):
    inputs[rel(p)]=sha(p)
    return json.loads(p.read_text())

def generate():
    inputs={}
    census_file=CONS/'coverage/members.tsv';inputs[rel(census_file)]=sha(census_file)
    census={r['member']:r for r in csv.DictReader(census_file.open(),delimiter='\t')}
    entries={}
    auxiliary=[]
    def add(member, targets, role, origin, locator, scope, evidence, artifact='Verus', status='inherited-not-reaudited'):
        # A group target bundle preserves many-to-many roles; it does not claim every
        # listed declaration transcribes every branch of each production member.
        key=json.dumps([member,sorted(set(targets)),role,scope,artifact],sort_keys=True)
        if key in entries:
            entries[key]['origins'].append({'origin':origin,'locator':locator})
            entries[key]['evidence_sources'].append({'origin':origin,'evidence':evidence})
            return
        path='src/cls/IPM/'+member.split('::')[0] if '::' in member else ''
        row=census.get(member); flags=[]
        if not row:flags.append('production-member-unresolved')
        source_file=ROOT/path
        if not path or not source_file.is_file():flags.append('production-source-missing')
        elif row and not re.search(r'^(?:ClassMethod|Method|Query|Trigger)\s+'+re.escape(member.split('::',1)[1])+r'(?=\s|\()',source_file.read_text(),re.M):
            flags.append('production-declaration-unresolved')
        target_rows=[]
        for target in sorted(set(targets)):
            file,sep,symbol=target.rpartition('::')
            if not sep:file,symbol=target,''
            p=ROOT/file
            exists=p.is_file()
            resolved=exists and bool(symbol) and bool(re.search(r'\b(?:fn|def|struct|enum|type|const)\s+'+re.escape(symbol)+r'\b',p.read_text()))
            if not resolved:flags.append('formal-target-unresolved')
            target_rows.append({'target':target,'resolved_declaration':resolved,'sha256':sha(p) if exists else None})
            if exists:inputs[file]=sha(p)
        if not targets:flags.append('formal-target-missing')
        if artifact!='Verus':flags.append('not-active-Verus-binding')
        if 'challenge-only' in status:flags.append('challenge-only')
        # Iter14 explicitly withdrew this earlier Unix tree-clearing correspondence.
        # Preserve the historical link without restoring its semantic credit.
        if member=='ResourceProcessor/FileCopy.cls::DoCopy' and any('thin_surfaces.rs::copy_step' in t for t in targets):
            flags.append('includes-superseded-Unix-clearing-correspondence-iter14')
        entries[key]={'id':'map-'+hashlib.sha256(key.encode()).hexdigest()[:16],
            'member':member,'source':path,'start':int(row['start']) if row else None,
            'end':int(row['end']) if row else None,'source_sha256':sha(ROOT/path) if path and (ROOT/path).is_file() else None,
            'formal_targets':target_rows,'role':role,'scope':scope,'artifact':artifact,
            'evidence':evidence,'status':status,'flags':sorted(set(flags)),
            'direct_implementation_proof':False,'whole_member_verified':False,
            'origins':[{'origin':origin,'locator':locator}],'evidence_sources':[{'origin':origin,'evidence':evidence}]}

    p=CONS/'extension/bindings.json'
    for i,b in enumerate(load(p,inputs)):
        add(b['member'],[b['formal_symbol']] if b.get('formal_symbol') else [],b.get('role','unspecified'),
            'extension',rel(p)+f'#/{i}',str(b.get('property',b.get('premises','Historical location only'))),
            {'native':b.get('native'),'premises':b.get('premises'),'original_status':b.get('status')},b.get('artifact','unknown'))
    # Reconcile manifests that were not carried into the extension view; both
    # locations are immutable inputs, with duplicate links recorded only once.
    for p in sorted((ROOT/'.g5/Verus_verification/consolidated/verus').glob('*.manifest.json')):
        d=load(p,inputs);model=d['formal_file'];model=model if model.startswith('.g5/') else rel(p.parent/model)
        for i,b in enumerate(d.get('bindings',[])):
            symbols=b.get('symbols') or ([b['symbol']] if b.get('symbol') else [])
            targets=[model+'::'+symbol for symbol in symbols]
            add(b['member'],targets,b.get('role','unspecified'),'manifest',rel(p)+f'#/bindings/{i}',
                str(b.get('property',b.get('premises','See manifest'))),{'premises':b.get('premises'),'manifest':rel(p)})
    p=CONS/'iter2/bindings.json'
    for i,b in enumerate(load(p,inputs)):
        target=b.get('formal_symbol') or '.g5/Verus_verification/consolidated/iter2/'+b['model']+'::'+b['symbol']
        add(b['member'],[target],b.get('role','unspecified'),'iter2',rel(p)+f'#/{i}',
            b.get('checked_scope',b.get('property','Scoped inherited relation')),{'premises':b.get('premises',b.get('remaining_scope'))},status=b.get('credit','inherited-not-reaudited'))
    # Nested authoritative boundary records are mirrored in iter2's main ledger.
    # Merge their specific properties/evidence/origins, instead of losing scope or
    # counting the mirrored declaration as a new link.
    p=CONS/'iter2/boundaries/bindings.json'
    boundary_rows=load(p,inputs)
    for i,b in enumerate(boundary_rows):
        add(b['member'],[b['formal_symbol']],b.get('role','unspecified'),'iter2-boundary-authority',rel(p)+f'#/{i}',
            b['property'],{'premises':b.get('premises'),'native':b.get('native'),'original_status':b.get('status')})
    withdrawal_file=CONS/'iter2/boundaries/withdrawals.json'
    withdrawals=load(withdrawal_file,inputs)
    p=CONS/'iter3/INDEX.json'
    for i,b in enumerate(load(p,inputs)['bindings']):
        add(b['member'],[b['formal']+'::'+s for s in b['symbols']],'group-role','iter3',rel(p)+f'#/bindings/{i}',
            b['scope'],{'original':b.get('evidence'),'proofs':b.get('proofs')})
    for n in range(4,28):
        p=CONS/f'iter{n}/bindings.json';p=p if p.exists() else p.with_name('INDEX.json')
        if not p.exists():continue
        d=load(p,inputs)
        for i,b in enumerate(d['rows']):
            targets=b.get('verus') or [d['verus_dir']+'/'+b['model']+'::'+s for s in b['symbols']]
            for extra in b.get('auxiliary_source_ranges',[]):
                source=ROOT/extra['path'];inputs[extra['path']]=sha(source)
                lines=source.read_text().splitlines();assert 1<=extra['start']<=extra['end']<=len(lines),extra
                auxiliary.append({'origin':f'iter{n}','binding':b.get('id'),'formal_targets':targets,'source_sha256':sha(source),'named_member_credit':False,'direct_implementation_proof':False,**extra})
            for role in ['direct','reader']:
                for path,name in b.get(role,[]):
                    add(path.removeprefix('src/cls/IPM/')+'::'+name,targets,role,f'iter{n}',rel(p)+f'#/rows/{i}',
                        b.get('scope',b.get('group',b.get('id','Scoped grouped relation'))),
                        {'owner':b.get('owner'),'audit':rel(CONS/f'iter{n}/binding-audit.json') if (CONS/f'iter{n}/binding-audit.json').exists() else 'See iteration handoff'},
                        status='current-scoped-audit' if n==27 else 'inherited-not-reaudited')
    # Keep every previously claimed locator visible if its formal edge is missing.
    p=CONS/'iter2/inventory/baseline-reach.json'
    for member in load(p,inputs)['represented_members']:
        if not any(e['member']==member and e['artifact']=='Verus' for e in entries.values()):
            add(member,[],'historical-claim','iter2-baseline',rel(p),
                'Inherited represented-member claim has no resolved Verus edge in consolidated sources; unestablished pending reconciliation',{})
    for entry in entries.values():
        if entry['member']=='Repo/Filesystem/Definition.cls::BuildCache' and any('owners.rs::build_cache' in t['target'] for t in entry['formal_targets']):
            entry['flags'].append('partial-outer-lock-correspondence-withdrawn-OW-CONSTRUCTOR')
            entry['withdrawn_scopes']=[w for w in withdrawals if w['id']=='OW-CONSTRUCTOR']
            entry['retained_scope']='Other cache/purge/rollback distinctions remain represented under their premises; no entire-member withdrawal. Correct return-time lock correspondence is in the boundary authority.'
    rows=sorted(entries.values(),key=lambda e:(e['member'],e['id']))
    mapped={t['target'].split('::')[0] for e in rows for t in e['formal_targets']}
    canonical_refs=collections.defaultdict(list)
    for p in sorted((ROOT/'.g5/ontology').rglob('*.g5.md')):
        inputs[rel(p)]=sha(p)
        for href in re.findall(r'\]\(([^)]+\.rs)(?:#[^)]*)?\)',p.read_text()):
            target=(p.parent/href).resolve()
            if target.is_relative_to(ROOT):canonical_refs[rel(target)].append(rel(p))
    linked={e['member'] for e in rows if not e['flags']}
    unresolved=[e['id'] for e in rows if e['flags']]
    by=collections.defaultdict(list)
    for member in sorted(set(census)-linked):by[member.split('/')[0] if '/' in member else '(root)'].append(member)
    return {'product_pin':PIN,'convention':'Generated view from immutable legacy maps/manifests and per-iteration bindings; edit the owning binding record, then regenerate. Group target bundles preserve scoped shared roles.',
        'semantic_limit':'Resolution only; all earlier semantic audits carried, not re-audited. No direct product body proofs. Citation/trust/Python/challenge/superseded/unresolved rows retained with flags and excluded from active Verus locator count.',
        'archive_boundary':'Historical inherited records retained for bookkeeping only; flagged archive-path behavior remains unassessed/unpursued by user instruction.',
        'inputs':dict(sorted(inputs.items())),'entries':rows,'production_denominator':len(census),
        'auxiliary_source_bindings':auxiliary,
        'boundary_authority_rows':len(boundary_rows),'known_partial_withdrawals':withdrawals,
        'binding_map_inputs':sorted(k for k in inputs if k.endswith('/bindings.json')),
        'unregistered_binding_files':sorted(rel(f) for f in CONS.rglob('bindings.json') if not any(part in ['scratch','mutants','native','proofs'] for part in f.relative_to(CONS).parts) and rel(f) not in inputs),

        'canonical_formal_files':dict(canonical_refs),
        'canonical_files_missing_from_map':sorted(set(canonical_refs)-mapped),
        'resolved_Verus_locator_members':sorted(linked),'resolved_Verus_locator_count':len(linked),
        'flagged_entries':unresolved,'unlinked_by_subsystem':dict(by),
        'reverse':{m:[e['id'] for e in rows if e['member']==m] for m in sorted({e['member'] for e in rows})}}

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');parser.add_argument('--member');parser.add_argument('--formal');parser.add_argument('--source-file');args=parser.parse_args()
    data=generate();out=HERE/'project-map.json'
    if args.check:
        assert json.loads(out.read_text())==data,'Project map stale; regenerate and inspect changed inputs/flags'
    elif not args.member and not args.formal and not args.source_file:out.write_text(json.dumps(data,indent=2)+'\n')
    if args.member or args.formal or args.source_file:
        rows=[e for e in data['entries'] if (not args.member or e['member']==args.member) and (not args.formal or any(args.formal in t['target'] for t in e['formal_targets']))]
        if args.source_file:rows=[e for e in rows if e['source']==args.source_file]
        auxiliary=[a for a in data['auxiliary_source_bindings'] if (not args.source_file or a['path']==args.source_file) and (not args.formal or any(args.formal in t for t in a['formal_targets']))]
        print(json.dumps({'named_member_bindings':rows,'auxiliary_source_bindings':auxiliary},indent=2))
    else:print(json.dumps({'ok':True,'entries':len(data['entries']),'flagged_entries':len(data['flagged_entries']),'resolved_Verus_locator_count':data['resolved_Verus_locator_count'],'denominator':data['production_denominator']}))
