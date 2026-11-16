#!/usr/bin/env python3
"""Run formal properties, model mutants and native comparisons without changing inherited evidence."""
from pathlib import Path
import importlib.util,json,subprocess,re,tempfile,hashlib,datetime,sys,copy
HERE=Path(__file__).resolve().parent
ROOT=HERE.parent
REPO=ROOT.parents[2]
VERUS=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
OUT=HERE/'results'/'env-attr'
OUT.mkdir(parents=True,exist_ok=True)
def run(cmd):
    p=subprocess.run([str(v) for v in cmd],capture_output=True,text=True,timeout=120,cwd=OUT)
    return {'command':[str(v) for v in cmd],'returncode':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
checks=[]
for filename in ['env_config.rs','attr_evaluator.rs']:
    result=run([VERUS,HERE/filename]);checks.append({'model':filename,**result});assert result['returncode']==0,result
# Run each model mutation alongside the decisive unmutated result, in disposable files.
mutants=[
    ('env-array-object-replace','env_config.rs','get(dst, k), src[k]','None, src[k]'),
    ('env-later-file-keeps-old','env_config.rs','config: empty(false), ok: false','config: old, ok: false'),
    ('env-array-tail-dropped','env_config.rs','src.dom().union(dst.dom())','src.dom()'),
    ('env-always-worker-compile','env_config.rs','-> bool { !config_empty }','-> bool { false }'),
    ('env-literal-dollar','env_config.rs',"else if text[0] == '$'",'else if false'),
    ('attr-gate-removed','attr_evaluator.rs',"if !raw.contains('{')",'if false'),
    ('attr-cycle-raises','attr_evaluator.rs','params: cp, circular_error: false','params: cp, circular_error: true'),
    ('attr-command-ignored','attr_evaluator.rs','if command.dom().contains(k) { command[k] }','if command.dom().contains(k) { defaults[k] }'),
    ('attr-late-error-rolls-back','attr_evaluator.rs','module: true, committed: true, history_exists: true','module: !message.error, committed: true, history_exists: true'),
    ('attr-dot-literal','attr_evaluator.rs',"pattern[0] == '.' || ",'false || '),
]
controls=[]
with tempfile.TemporaryDirectory(prefix='ipm-env-attr-controls-') as t:
    tmp=Path(t)
    for name,filename,old,new in mutants:
        for f in ['env_config.rs','attr_evaluator.rs']:(tmp/f).write_text((HERE/f).read_text())
        source=(tmp/filename).read_text();assert old in source,(name,old);(tmp/filename).write_text(source.replace(old,new))
        result=run([VERUS,tmp/filename]);rejected=result['returncode']!=0 and ('postcondition not satisfied' in result['stderr'] or 'postcondition not satisfied' in result['stdout'])
        controls.append({'control':name,'model':filename,'rejected_at_property':rejected,**result});assert rejected,controls[-1]
# Every old family native field remains an independent oracle.
framework=load('env_attr_framework',ROOT/'checks/framework.py')
family=[]
for n in ['env_config','attr_evaluator']:
    m=framework.load_family(ROOT/'models'/f'{n}.py');r=framework.check_family(m);family.append(r)
    assert not r['mismatches'],r['mismatches']
# Held-out loader composition: array whose element is an object; primitive-array neighbor.
# Native output is consumed, parsed, and fed to a proof of actual formal equality.
run_dir=ROOT/'native/results/20261114T-env-ported-probes'
raw=(run_dir/'raw.log').read_text();assert 'NATIVE-END' in raw
observations={c:json.loads(v) for c,v in re.findall(r'^OBS (EP[12]) json=(.*)$',raw,re.M)}
env=load('env_config_probe',ROOT/'models/env_config.py');env.V['v']='as-built'
probe_cases=[('EP1',[{'x':1}],[{'y':2}],observations['EP1']),('EP2',[1,2],[3],observations['EP2'])]
keys={'x':1,'y':2}
def formal(j):
    if isinstance(j,list):return 'Json::Container { array: true, entries: '+fmap(list(enumerate(j)))+' }'
    if isinstance(j,dict):return 'Json::Container { array: false, entries: '+fmap([(keys[k],v) for k,v in j.items()])+' }'
    assert type(j)==int;return f'Json::Atom({j}int)'
def fmap(items):return 'map!['+', '.join(f'{k}int => {formal(v)}' for k,v in items)+']'
def observed_clauses(expr,j):
    if type(j)==int:return [f'{expr} == Json::Atom({j}int)']
    items=list(enumerate(j)) if isinstance(j,list) else [(keys[k],v) for k,v in j.items()]
    clauses=[f'entries(Some({expr})).dom() =~= set!['+', '.join(f'{k}int' for k,_ in items)+']',f'array_tag({expr}) == Some({str(isinstance(j,list)).lower()})']
    for k,v in items:
        clauses.append(f'at({expr}, {k}int).is_some()')
        clauses+=observed_clauses(f'at({expr}, {k}int).unwrap()',v)
    return clauses
bridges=[]
for cid,a,b,observed in probe_cases:
    actual=copy.deepcopy(a);env.merge(actual,b)
    # EP1 prints the element object, EP2 prints the full array.
    expected=[observed] if cid=='EP1' else observed
    assert actual==expected,(cid,actual,expected)
    expr=f'merge_value(Some({formal(a)}), {formal(b)}, 8nat)'
    clauses=[expr+'.is_some()']+observed_clauses(expr+'.unwrap()',expected)
    bridges.append(f'pub proof fn native_{cid.lower()}() ensures '+', '.join(clauses)+' { reveal_with_fuel(merge_value, 9); }')
# Scalar-lookup and clear/reset observations are checked separately.
assert 'OBS EP3 ok=0 empty=1' in raw and 'OBS EP4 scalar-path=[]' in raw and 'OBS EP5 reset.empty=1' in raw
bridge=HERE/'env_native_bridge.rs'
bridge.write_text('use vstd::prelude::*;\nmod env_config;\nuse env_config::*;\nverus! {\n'+'\n'.join(bridges)+'\n}\nfn main(){}\n')
bridge_result=run([VERUS,bridge]);assert bridge_result['returncode']==0,bridge_result
# Attribute native observations are used as expected values of concrete model expressions.
# Matcher location, macro compiler result and expression interpreter remain explicit input premises.
aobs=framework.parse_obs('20261114T025636Z-r1-evaluator')
pobs=framework.parse_obs('20261114T025551Z-r1-eval-postcommit',{'aftermsg':r'^(Done .*)$'})
def chars(text):
    def lit(c):
        return "'"+({'\\':'\\\\',"'":"\\'",'\n':'\\n','\r':'\\r','\t':'\\t'}.get(c,c))+"'"
    return 'seq!['+', '.join(lit(c) for c in text)+']'
attribute_bridges=[]
def ab(case,claim,body=''):
    attribute_bridges.append(f'pub proof fn native_{case.lower()}() ensures {claim} {{ {body} }}')
ab('E05',f'system_expression(SystemName::NamespaceLower,{chars("USER")},{chars(aobs[("ENV","mgrdir")])}) == Some({chars(aobs[("E05","value")])})','reveal_with_fuel(lower_text,6);')
ab('E23',f'system_expression(SystemName::ManagerDirectory,{chars("USER")},{chars(aobs[("ENV","mgrdir")])}) == Some({chars(aobs[("E23","value")])})')
raw_macro='$$$OK no brace'
ab('E15',f'evaluate({chars(raw_macro)}, macro_helper(None)).value == {chars(aobs[("E15","value")])}')
ab('E20',f'with_builtins(parameters(map![1int => {chars("9.9.9")}], Map::empty()), map![1int => {chars("1.2.3")}])[1] == {chars(aobs[("E20","value")])}')
ab('E21',f'name_matches({chars("a.b")}, {chars("axb")}), regex_replace_match({chars("{$axb}")},0,6,true,{chars("DOT")}, Seq::empty()) == Some({chars(aobs[("E21","value")])})','reveal_with_fuel(name_matches,5); reveal_with_fuel(quote_backslashes,5); reveal_with_fuel(env_config::replacement,6);')
assert aobs[('E32','error')]=='<REGULAR EXPRESSION>'
ab('E32',f'regex_replace_match({chars("{$price}")},0,8,true,{chars("$5")}, Seq::empty()).is_none()','reveal_with_fuel(quote_backslashes,4); reveal_with_fuel(env_config::replacement,4);')
ab('E34',f'regex_replace_match({chars("${win}")},0,6,true,{chars("C:\\dir\\sub")}, Seq::empty()) == Some({chars(aobs[("E34","value")])})',f'full_match_backslash_literal({chars("${win}")},{chars(aobs[("E34","value")])},Seq::empty());')
assert aobs[('E30','resolve.raised')].startswith('0 ')
ab('E30','!resolve_bounded(map![1int => seq![Token::Reference(2)],2int => seq![Token::Reference(1)]],Map::empty(),seq![1int,2int],20).circular_error','declared_cycle_error_is_unreachable(map![1int => seq![Token::Reference(2)],2int => seq![Token::Reference(1)]],Map::empty(),seq![1int,2int],20);')
for case in ['P1','P2txn','P3']:
    error=pobs[(case,'status')]=='ERR'
    msg='macro_helper(None)' if error else 'macro_helper(Some(seq![]))'
    assert ('committed:1' in next(v for (c,k),v in pobs.items() if c==case and k.startswith('hist.')))
    assert next(v for (c,k),v in pobs.items() if c==case and k.startswith('mod.')).startswith('present')
    ab(case,f'after_message({msg}).module, after_message({msg}).committed, after_message({msg}).history_finalized, after_message({msg}).status_ok == {str(not error).lower()}, after_message({msg}).history_success == {str(not error).lower()}')
attr_bridge=HERE/'attr_native_bridge.rs'
attr_bridge.write_text('use vstd::prelude::*;\nmod attr_evaluator;\nuse attr_evaluator::*;\nverus! {\n'+'\n'.join(attribute_bridges)+'\n}\nfn main(){}\n'.replace('\\n','\n'))
# env_config is nested in attr_evaluator; the proof calls its shared replacement helper.
bridge_source=attr_bridge.read_text().replace('env_config::replacement','attr_evaluator::env_config::replacement')
attr_bridge.write_text(bridge_source)
attr_bridge_result=run([VERUS,attr_bridge]);assert attr_bridge_result['returncode']==0,attr_bridge_result

# Paired before/after oracle: original twin demonstrably lost x in EP1.
before=load('env_original',ROOT/'extension/before-models/env_config.py');before.V['v']='as-built'
old=[{'x':1}];before.merge(old,[{'y':2}]);assert old!=[observations['EP1']]
input_paths=[HERE/'env_config.rs',HERE/'attr_evaluator.rs',HERE/'env_native_bridge.rs',HERE/'attr_native_bridge.rs',HERE/'check_env_attr.py',ROOT/'checks/framework.py',VERUS,ROOT/'models/env_config.py',ROOT/'models/attr_evaluator.py',run_dir/'script.cos',run_dir/'raw.log',run_dir/'receipt.json',ROOT/'extension/before-models/env_config.py']
input_paths += [VERUS.parent/n for n in ['version.json','vstd.vir','libvstd.rlib','z3','rust_verify']]
for sub in ['native/fixtures/c5','native/fixtures/env-ported']:
    input_paths += sorted(p for p in (ROOT/sub).rglob('*') if p.is_file())
for native_run in ['20261114T033831Z-r5-env','20261114T033908Z-r5-env-b','20261114T025636Z-r1-evaluator','20261114T025551Z-r1-eval-postcommit']:
    input_paths += [ROOT/'native/results'/native_run/n for n in ['script.cos','raw.log','receipt.json']]
summary={'generated':datetime.datetime.now(datetime.timezone.utc).isoformat(),'product_pin':'571e04a19aba9f9b4bc0ea23fbe09182038f4e55','python_runtime':sys.version,'formal':checks,'controls':controls,'native_bridge':bridge_result,'attr_native_bridge':attr_bridge_result,'native_comparisons':[{'case':cid,'match':True,'observed':obs} for cid,_,_,obs in probe_cases],'before_env_EP1':old,'after_env_EP1':[observations['EP1']],'family_checks':family,'inputs':{str(p.relative_to(REPO)) if p.is_relative_to(REPO) else str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in input_paths}}
(OUT/'checks.json').write_text(json.dumps(summary,indent=2))
print(json.dumps({'formal':[(r['model'],re.findall(r'verification results::.*',r['stdout'])) for r in checks],'controls_rejected':sum(c['rejected_at_property'] for c in controls),'native_bridge':bridge_result['returncode'],'families':[(f.get('family'),len(f['mismatches'])) for f in family],'old_env_EP1':old,'new_env_EP1':observations['EP1']}))
