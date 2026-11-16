#!/usr/bin/env python3
"""Turn observed native fields into assertions against input-derived Verus contracts, never assumptions."""
from pathlib import Path
import hashlib,json,os,subprocess,time
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated/iter3/proofs';NATIVE=ROOT/'.g5/uplift/consolidated/iter3/native'
def text(s):return 'seq!['+','.join(str(ord(c))+('int' if i==0 else '') for i,c in enumerate(s))+']'
def main():
    native=json.loads((NATIVE/'receipt.json').read_text());obs=native['observations'];checks=[]
    def b(key,expression):
        assert obs[key] in ['0','1'],(key,obs[key]);checks.append((key,f'assert(({expression}) == {str(obs[key]=="1").lower()});'))
    def t(key,expression):checks.append((key,f'assert(({expression}) == {text(obs[key])});'))
    def n(key,expression):checks.append((key,f'assert(({expression}) == {int(obs[key])});'))
    b('secure-bytes','a::provider_status(a::Auth::Bearer,a::bytes_request())==200')
    b('filter-2',f'a::parse_fields(seq![(a::Key::Repo,a::Condition::Scalar({text("r")}))],a::empty_criteria()).is_some()')
    b('filter-3',f'a::parse_field(a::empty_criteria(),a::Key::Repo,a::Condition::Object(seq![(a::Op::Match,Some({text("r*")}))])).is_some()')
    b('filter-4',f'a::parse_field(a::empty_criteria(),a::Key::Other,a::Condition::Scalar({text("3")})).is_some()')
    for key,value,pattern in [('wild-1','a.b','a.b'),('wild-2','axb','a.b'),('wild-3','ab','a?'),('wild-4','xab','a*')]:b(key,f'a::wildcard({text(value)},{text(pattern)})')
    t('cached-bytes',f'a::save_download(a::SavedFile{{exists:true,bytes:{text("old")}}},{text("provider-content")},200).bytes')
    b('activate-local','a::activate(true,true,a::download(false,a::Auth::Anonymous,a::Download::MissingServer),true).status_ok')
    b('export-old','a::copy_export(a::ExportTree{destination:Map::empty().insert(seq![1int],seq![2int]),siblings:Map::empty()},Map::empty().insert(seq![3int],seq![4int])).destination.contains_key(seq![1int])')
    b('export-sibling','a::copy_export(a::ExportTree{destination:Map::empty(),siblings:Map::empty().insert(seq![1int],seq![2int])},Map::empty()).siblings.contains_key(seq![1int])')
    n('default-mode','r::apply_defaults(initial,Set::empty(),seq![one,two],class)[1]')
    n('default-other','r::apply_defaults(initial,Set::empty(),seq![one,two],class)[2]')
    b('default-explicit-empty','r::apply_defaults(initial.insert(1,-1),Set::empty().insert(1),seq![one,two],class)[1]==-1')
    b('health-unset','r::requirement_failures(unset,health).len()==0')
    b('health-false','r::requirement_failures(r::Requirements{health:r::Tri::Off,..unset},health).len()==0')
    b('health-implies-interop','r::requirement_failures(r::Requirements{health:r::Tri::On,interoperability:2,..unset},r::Environment{interoperability:false,..health}).len()==0')
    b('disabled-allows-enabled','r::requirement_failures(r::Requirements{interoperability:2,..unset},health).len()==0')
    b('requires-sys','r::requirement_failures(r::Requirements{sys_namespace:true,..unset},health).len()==0')
    b('requires-sys-in-sys',f'r::requirement_failures(r::Requirements{{sys_namespace:true,..unset}},r::Environment{{namespace:{text("%SYS")},..health}}).len()==0')
    for key,tri in [('invoke-default','Unset'),('invoke-unchecked','Off'),('invoke-checked','On')]:b(key,f'p::invoke_ok(p::Tri::{tri},true,false,false)')
    t('setting-no-overwrite',f'p::set_value(first,{text("second")},false).current')
    t('setting-empty-allows-write',f'p::set_value(p::set_value(first,seq![],true),{text("third")},false).current')
    t('setting-first',f'p::set_value(p::set_value(first,seq![],true),{text("third")},false).first.unwrap()')
    t('format-case',f'p::set_format(first,{text("JsOn")},true).unwrap().current')
    b('format-invalid',f'p::set_format(first,{text("invalid")},true).is_some()')
    b('test-prefix',f'p::test_selected({text("tEsT")},phases,{text("A")},None,{text("AB")})')
    b('owns-sibling',f'p::owns_class({text("AB")},seq![],{text("A")})')
    b('owns-child',f'p::owns_class({text("A.C")},seq![],{text("A")})')
    b('suite-empty-shadows',f'p::test_selected({text("Test")},phases,{text("A")},Some(seq![]),{text("Z")})')
    b('case-alone',f'p::test_selected({text("Test")},phases,{text("A")},None,{text("Z")})')
    b('class-ignores-case',f'p::test_selected({text("Test")},phases,seq![],None,{text("Z")})')
    b('deploy-parameter-false','r::deploy(true,true,Some(0),r::Tri::On,false)')
    b('deploy-resource-true','r::deploy(true,true,None,r::Tri::On,false)')
    b('deploy-module-default','r::deploy(true,true,None,r::Tri::Unset,false)')
    b('deploy-deployedmodule-default','r::deploy(true,true,None,r::Tri::Unset,true)')
    b('export-overwrites-error','r::export_item(false,false,true,true).ok')
    n('project-after-check','r::check_project(seq![1int],Set::empty()).len()')
    b('project-deployed-on-error','r::load_project(true,true,false,false,false,true,false,false,true,false).marked_deployed')
    b('project-deployed-stored-on-error','r::project_database(false,true,false,r::resource_save_allowed(false,seq![65int],Set::empty().insert(seq![65int])))')
    b('project-duplicate-store','r::project_database(false,true,false,r::resource_save_allowed(true,seq![65int],Set::empty().insert(seq![65int])))')
    b('project-duplicate-object','r::load_project(true,true,false,false,false,true,false,false,r::resource_save_allowed(true,seq![65int],Set::empty().insert(seq![65int])),false).marked_deployed')
    b('project-marker-on-error','r::load_project(true,true,false,false,false,true,false,false,true,false).marker')
    b('archive-bad-gzip','a::header_action(a::HeaderRead{status_ok:false,name:seq![],expected:0,stored:0})!=a::HeaderAction::Reject')
    b('archive-bad-checksum','a::header_action(a::HeaderRead{status_ok:true,name:seq![97int],expected:353,stored:0})!=a::HeaderAction::Reject')
    for key in obs:
        if key.startswith('branch-missing-dev-'):b(key,'a::download(true,a::Auth::Anonymous,a::Download::MissingServer).ok')
        if key.startswith('branch-missing-normal-'):b(key,'a::download(false,a::Auth::Anonymous,a::Download::MissingServer).ok')
        if key.startswith('branch-auth-failed-dev-'):b(key,'a::download(true,a::Auth::Bearer,a::Download::SearchOrFetchError).ok')
        if key.startswith('branch-auth-success-'):b(key,'a::download(false,a::Auth::Bearer,a::Download::Complete).ok')
        if key.startswith('branch-anonymous-extract-dev-'):b(key,'a::artifact_first_header(true,a::Auth::Anonymous,a::HeaderRead{status_ok:false,name:seq![],expected:0,stored:0}).ok')
        if key.startswith('branch-anonymous-extract-normal-'):b(key,'a::artifact_first_header(false,a::Auth::Anonymous,a::HeaderRead{status_ok:false,name:seq![],expected:0,stored:0}).ok')
    imports='\n'.join(f'#[path="{os.path.relpath(HERE/file,OUT)}"] mod {alias};' for file,alias in [('artifacts.rs','a'),('resources.rs','r'),('public_repairs.rs','p')])
    header=f'''use vstd::prelude::*;
{imports}
verus! {{
proof fn native_fields() {{
    reveal_with_fuel(a::wildcard,12); reveal_with_fuel(a::parse_fields,5);
    reveal_with_fuel(r::apply_defaults,5); reveal_with_fuel(r::check_project,5);
    reveal_with_fuel(p::lower,12); reveal_with_fuel(p::suite_path,8);
    let class={text('Consolidated.Iter3.Defaults')};
    let initial=Map::empty().insert(1int,0int).insert(2int,0int);
    let one=r::DefaultRule{{class_name:class,conditions:seq![],values:Map::empty().insert(1int,1int)}};
    let two=r::DefaultRule{{class_name:class,conditions:seq![(1int,1int)],values:Map::empty().insert(1int,2int).insert(2int,3int)}};
    assert(r::satisfies(initial,seq![]));
    let step=r::defaults_step(initial,Set::empty(),one,class);
    assert(step[1]==1); assert(r::satisfies(step,two.conditions));
    let unset=r::Requirements{{health:r::Tri::Unset,interoperability:0,sys_namespace:false}};
    let health=r::Environment{{health:true,interoperability:true,namespace:{text('USER')}}};
    let first=p::Setting{{current:{text('first')},first:Some({text('first')})}};
    let phases=seq![{text('test')}];
    assert(p::lower({text('JsOn')}) =~= {text('json')});
    assert(p::lower({text('tEsT')}) =~= {text('test')});
    assert(p::lower({text('Test')}) =~= {text('test')});
    assert(p::phase_enabled({text('Test')},phases)) by {{assert(p::lower(phases[0])==p::lower({text('Test')}));}}
    assert(p::phase_enabled({text('tEsT')},phases)) by {{assert(p::lower(phases[0])==p::lower({text('tEsT')}));}}
    assert({text('AB')}.take(1) =~= {text('A')});
    assert({text('A.C')}.take(2) =~= {text('A')}+seq![46int]);
    assert({text('AB')}[1] != ({text('A')}+seq![46int])[1]); assert({text('A')}[0]!={text('Z')}[0]);
'''
    standard=header+'\n'.join('    // '+key+'\n    '+line for key,line in checks)+'\n}\n'
    extra=f'''proof fn native_selection() {{
    reveal_with_fuel(a::newest,6); reveal_with_fuel(a::wildcard,8); reveal_with_fuel(a::later,5);
    let branch={text('branch')};let main={text('main')};let dev={text('dev')};
    let c=a::Criteria{{repo:{text('r')},name:Some(a::Pattern{{value:{text('a*')},wildcard:true}}),path:None,props:Map::empty().insert(branch,main)}};
    let one=a::Artifact{{id:1,repo:{text('r')},name:{text('a1')},folder:seq![],updated:{text('2')},props:Map::empty().insert(branch,seq![main])}};
    let two=a::Artifact{{id:2,repo:{text('bad')},name:{text('a2')},updated:{text('9')},..one}};
    let three=a::Artifact{{id:3,name:{text('a3')},updated:{text('3')},props:Map::empty().insert(branch,seq![dev,main]),..one}};
    let four=a::Artifact{{id:4,name:{text('a4')},updated:{text('8')},props:Map::empty().insert(branch,seq![dev]),..one}};
    assert(c.props.contains_key(branch));
    assert(one.props[branch][0]==main);assert(one.props[branch].contains(main));
    assert(three.props[branch][1]==main);assert(three.props[branch].contains(main));
    assert(four.props[branch].len()==1);assert(four.props[branch][0][0]!=main[0]);
    assert(!four.props[branch].contains(main));
    assert(a::properties(one.props,c.props));assert(a::properties(three.props,c.props));
    assert(a::accepted(one,c));assert(a::accepted(three,c));
    assert(two.repo[0]!=c.repo[0]);assert(!a::accepted(two,c));assert(!a::accepted(four,c));
    // selection
    assert(a::newest(seq![one,two,three,four],c,None).unwrap().name=={text(obs['selection'].rsplit('/',1)[1])});
    let first=a::Artifact{{updated:{text('3')},..one}};
    let second=a::Artifact{{name:{text('a2')},..first}};
    // tie
    assert(a::newest(seq![first,second],c,None).unwrap().name=={text(obs['tie'].rsplit('/',1)[1])});
    let older=a::Artifact{{name:{text('older.tgz')},..one}};
    let newer=a::Artifact{{name:{text('newer.tgz')},..three}};
    // aql-first
    assert(a::authenticated_first(seq![older,newer]).unwrap().name=={text(obs['aql-first'].rsplit('/',1)[1])});
    // aql-second
    assert(a::authenticated_first(seq![newer,older]).unwrap().name=={text(obs['aql-second'].rsplit('/',1)[1])});
}}
'''
    checks += [(key,'native_selection proof') for key in ['selection','tie','aql-first','aql-second']]
    source=standard+extra+'}\nfn main() {}\n'
    bridge=OUT/'native_bridge.rs';bridge.write_text(source)
    tool=Path(os.environ.get('VERUS',str(Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))
    rows=[]
    for name,body in [('baseline',source),('flipped-native-byte-outcome',source.replace('// secure-bytes\n    assert((a::provider_status(a::Auth::Bearer,a::bytes_request())==200) == false);','// secure-bytes\n    assert((a::provider_status(a::Auth::Bearer,a::bytes_request())==200) == true);'))]:
        assert name=='baseline' or body!=source
        path=bridge if name=='baseline' else OUT/(name+'.rs');path.write_text(body);start=time.monotonic()
        p=subprocess.run([str(tool),str(path),'--triggers-mode','silent'],capture_output=True,text=True,timeout=120)
        raw=p.stdout+p.stderr;(OUT/(name+'-native.log')).write_text(raw)
        ok=p.returncode==0 if name=='baseline' else p.returncode!=0 and 'assertion failed' in raw
        rows.append({'name':name,'exit':p.returncode,'pass':ok,'wall_seconds':time.monotonic()-start,'command':[str(tool),str(path),'--triggers-mode','silent']})
        if not ok:print(raw[-5000:]);break
    result={'native_receipt_sha256':hashlib.sha256((NATIVE/'receipt.json').read_bytes()).hexdigest(),'bridge_sha256':hashlib.sha256(source.encode()).hexdigest(),'native_assertions':len(checks),'keys':[x[0] for x in checks],'results':rows,'scope':'Input-to-field comparison assertions; hand mapping, explicit provider ports, not implementation proof.'}
    (OUT/'native-bridge.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
    assert len(rows)==2 and all(x['pass'] for x in rows)
if __name__=='__main__':main()
