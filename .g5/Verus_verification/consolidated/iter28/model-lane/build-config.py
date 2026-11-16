from pathlib import Path
import json
r=Path(__file__).resolve().parent.parent;source=(r/'script.rs').read_text();models={};selectors={}
def add(name,old,new,selector):
 assert source.count(old)==1,(name,old,source.count(old));models[name]=[old,new];selectors[name]=selector
add('missing-file-ignored','if !file.exists{Prepared','if false{Prepared','shape_neighbor')
add('unsupported-extension-treated-as-txt','}else{Prepared{commands:seq![],error:Some(Error::UnsupportedExtension)}}','}else{Prepared{commands:lines(file.raw,0),error:None}}','shape_neighbor')
add('root-array-accepted','Some(JsonDoc::Array(_))=>Prepared{commands:seq![],error:Some(Error::ExpectedObject)},','Some(JsonDoc::Array(cs))=>translate_all(cs,0),','shape_neighbor')
add('commands-object-accepted','Some(JsonDoc::Object(CommandField::WrongType))=>Prepared{commands:seq![],error:Some(Error::ExpectedCommandsArray)},','Some(JsonDoc::Object(CommandField::WrongType))=>Prepared{commands:seq![],error:None},','shape_neighbor')
add('json-skips-preflight','let error=validate_all(cs,i);','let error=None;','json_preparation')
add('json-ignores-later-unknown','if head.error.is_some(){head.error}else{validate_all(cs,i+1)}','if false{head.error}else{validate_all(cs,i+1)}','json_preparation')
add('txt-prevalidates-known-unknown','Prepared{commands:lines(file.raw,0),error:None}','Prepared{commands:lines(file.raw,0),error:if file.raw==raw_txtunknown(){Some(Error::UnknownCommand)}else{None}}','text_preparation')
add('txt-does-not-trim','let t=trim(s);','let t=s;','text_preparation')
add('txt-keeps-blank-line','if t.len()==0{seq![]}else{seq![t]}','seq![t]','text_preparation')
add('txt-reverses-line-order','line_item(s.subrange(start,end))+lines(s,end+1)','lines(s,end+1)+line_item(s.subrange(start,end))','text_preparation')
add('json-reverses-parameter-values','ps[i].1+(if i+1<ps.len(){seq![\' \']+values(ps,i+1)}else{seq![]})','(if i+1<ps.len(){values(ps,i+1)+seq![\' \']}else{seq![]})+ps[i].1','translation_neighbor')
add('json-accepts-unknown-command',"else if c.name!=seq!['c','o','n','f','i','g'] && c.name!=seq!['e','x','e','c']",'else if false','translation_neighbor')
add('json-missing-command-accepted','if c.name.len()==0{Prepared','if false{Prepared','translation_neighbor')
add('json-accepts-invalid-parameter','else if !params_ok(c.name,c.params,0){Prepared','else if false{Prepared','translation_neighbor')
add('continue-after-shell-error','if step.error.is_some(){Out{world:step.world','if false{Out{world:step.world','execution_neighbor')
add('rollback-earlier-success','Out{world:step.world,error:step.error,listed:seq![command],attempted}','Out{world:initial(),error:step.error,listed:seq![command],attempted}','execution_neighbor')
add('dry-run-executes-shell','let step=if dry{Step{world:w,error:None}}else{shell(w,command)};','let step=if false{Step{world:w,error:None}}else{shell(w,command)};','dry_execution_neighbor')
add('dry-run-skips-json-validation','pub open spec fn planned(file:File,dry:bool)->Prepared {prepare(file)}','pub open spec fn planned(file:File,dry:bool)->Prepared {if dry{Prepared{commands:seq![],error:None}}else{prepare(file)}}','dry_json_validation_neighbor')
add('failed-command-not-listed','listed:seq![command],attempted}','listed:seq![],attempted}','execution_neighbor')
add('dry-run-hides-command-list','Out{listed:seq![command]+rest.listed,attempted:','Out{listed:if dry{rest.listed}else{seq![command]+rest.listed},attempted:','dry_execution_neighbor')
add('dry-run-attempts-command','let attempted=if dry{seq![]}else{seq![command]};','let attempted=seq![command];','dry_execution_neighbor')
add('runtime-fault-loses-side-effect','Step{world:World{calls:w.calls+1,..w},error:Some(Error::Runtime)}','Step{world:w,error:Some(Error::Runtime)}','runtime_neighbor')
add('runtime-fault-reports-success','Step{world:World{calls:w.calls+1,..w},error:Some(Error::Runtime)}','Step{world:World{calls:w.calls+1,..w},error:None}','runtime_neighbor')
add('settings-write-dropped','World{cfg:settings_projection::config_set(w.cfg,1,key(),v).0,..w}','w','settings_neighbor')
add('settings-write-uses-first-value','config_set(w.cfg,1,key(),v).0','config_set(w.cfg,1,key(),101).0','settings_neighbor')
add('raw-reader-returns-reset-value','pub open spec fn raw_value(w:World)->nat {if w.cfg.nodes.contains_key(key()) && w.cfg.nodes[key()].val.is_some(){w.cfg.nodes[key()].val.unwrap()}else{0}}','pub open spec fn raw_value(w:World)->nat {7}','settings_neighbor')
labels={'txt-valid':('TxtValid',False),'txt-unknown':('TxtUnknown',False),'txt-dry-unknown':('TxtUnknown',True),'txt-runtime':('TxtRuntime',False),'json-valid':('JsonValid',False),'json-unknown':('JsonUnknown',False),'json-runtime':('JsonRuntime',False),'json-dry-valid':('JsonValid',True),'json-dry-unknown':('JsonUnknown',True),'json-array':('JsonArray',False),'json-wrongcommands':('JsonWrong',False),'missing-file':('Missing',False),'unsupported-extension':('Unsupported',False)}
replay={}
for label in ['txt-valid','txt-unknown','txt-dry-unknown']:
 c,dry=labels[label];replay[label+'-ok']='b(ok(named(Case::'+c+','+str(dry).lower()+')))'
for label,(c,dry) in labels.items():
 n='named(Case::'+c+','+str(dry).lower()+')';replay[label+'-ok']='b(ok('+n+'))';replay[label+'-value']='setting_value(('+n+').world) as int';replay[label+'-raw-value']='raw_value(('+n+').world) as int';replay[label+'-runtime-calls']='('+n+').world.calls as int'
for label,case,dry,which in [('txt-unknown','TxtUnknown',False,'last'),('txt-dry-unknown','TxtUnknown',True,'last'),('json-unknown','JsonUnknown',False,'first'),('json-runtime','JsonRuntime',False,'last'),('json-dry-valid','JsonValid',True,'last')]:
 replay[label+'-'+which+'-listed']='b('+which+'_listed(named(Case::'+case+','+str(dry).lower()+')))'
native_rel='.g5/uplift/consolidated/iter28/native/20261116T014415191568Z/observations.json';native=json.loads((r.parents[3]/native_rel).read_text())['observations'];expected={k:(0 if k=='final-tlevel' else 1) for k in native if k not in replay};assert set(native)==set(replay)|set(expected);assert all(native[k]==v for k,v in expected.items())
cfg={'models':{'script.rs':models},'dependency_controls':{},'control_functions':{'script.rs':selectors},'control_options':['--multiple-errors','0'],'control_isolation_helper':'model-lane/isolate_witness.py','control_isolation_function':'isolate','focused_replay_barrier':True,'projection_check':'model-lane/check-settings-projection.py','reuse_source_paths':['playbook-uplift/models/verus/settings.rs'],'replay_model':'script.rs','replay_grouped':True,'replay':replay,'replay_prelude':'assert(first()!=last() && first()!=unknown() && last()!=unknown() && fault()!=first() && fault()!=last() && fault()!=unknown()) by(compute);text_preparation();json_preparation();shape_neighbor();execution_neighbor();dry_execution_neighbor();runtime_neighbor();named_neighbors();reveal_with_fuel(execute,4);','native_expected':expected,'native_receipt':native_rel,'scope_notes':[
 'Canonical-only valid TXT and object-form predictions independent; error/preflight/source guidance disclosed before model-lane on-disk ledger; do not claim those independent discoveries.',
 'Existing P:F-RES-5/DEC-RES-2 owns help/root-array documentation mismatch; no new bug or policy decision.',
 'Exact frozen ASCII file bytes to explicit JSON tree adapter; generic %FromJSON/UTF8/parser/iteration/lexer and selected runtime fault semantics provider-qualified.',
 'Literal JSON object parameter sequence preserved; no generic parameter-order intent or JSON key-order guarantee.',
 'Unchanged type/spec projection from inherited settings.rs checked read-only before use; no inherited proof-unit credit, unrelated algorithms omitted.',
 'DefaultLogEntryLimit uses explicit abstract key3 and selected numeric-string ranks7/101/202, not physical IDs; other settings/default nodes not exact native full-store state.',
 'arbitrary_default proves all outcome observables independent of the unknown prior default node; initialNone is representative only.',
 'Header/ANSI/error-message rendering, reset/status/namespace/tlevel and generic file I/O remain qualifications; listed command content projection modeled.',
 'No process-termination/history-count/ShellScript/full-interpreter proof; no archive/package/verify/export work or private evaluator access.'
]}
cfg['replay_prelude']+='output_neighbors();'
cfg['replay_preludes']={label+'-raw-value':'raw_reader(named(Case::'+c+','+str(dry).lower()+').world);' for label,(c,dry) in labels.items()}
(r/'script-cases.json').write_text(json.dumps(cfg,indent=2)+'\n');print('controls',len(models),'selectors',len(set(selectors.values())),'replay',len(replay),'gates',len(expected))
