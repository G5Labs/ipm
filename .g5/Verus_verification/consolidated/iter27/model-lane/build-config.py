from pathlib import Path
import json,shutil
r=Path(__file__).resolve().parent.parent
models={};selectors={}
def add(name,old,new,selector):
 assert (r/'purge.rs').read_text().count(old)==1,(name,old)
 models[name]=[old,new];selectors[name]=selector
add('data-path-flattens-dots','pub open spec fn data_path(s:Text)->Path {split_from(s,0,seq![])}','pub open spec fn data_path(s:Text)->Path {seq![s]}','path_neighbor')
add('data-path-retains-empty-segment','(if part.len()==0{seq![]}else{seq![part]})+split_from(s,i+1,seq![])','seq![part]+split_from(s,i+1,seq![])','path_neighbor')
add('purge-alias-writes-nested','Flag::Purge=>set_data(Map::empty(),top_name(),1)','Flag::Purge=>set_data(Map::empty(),dotted_name(),1)','command_neighbor')
add('custom-config-writes-top','Flag::Configure=>set_data(Map::empty(),dotted_name(),1)','Flag::Configure=>set_data(Map::empty(),top_name(),1)','command_neighbor')
add('plain-invokes-installer','if !has_installer(w.module.unwrap()){w}else{','if false{w}else{','plain_neighbor')
add('hook-never-invoked','if !has_installer(w.module.unwrap()){w}else{','if true{w}else{','hook_neighbor')
add('purge-argument-reads-top','let purge=get(p,nested_key());','let purge=get(p,top_key());','hook_neighbor')
add('callback-count-missing','Callback{called:prior_called(w)+1,purge,','Callback{called:prior_called(w),purge,','hook_neighbor')
add('callback-argument-erased','Callback{called:prior_called(w)+1,purge,','Callback{called:prior_called(w)+1,purge:0,','hook_neighbor')
add('vars-top-erased','top:get(p,top_key()),','top:0,','hook_neighbor')
add('vars-nested-erased','nested:get(p,nested_key()),','nested:0,','hook_neighbor')
add('after-hook-missing','nested:get(p,nested_key()),after:prior_after(w)+1','nested:get(p,nested_key()),after:prior_after(w)','hook_neighbor')
add('callback-counter-resets','called:prior_called(w)+1','called:1nat','hook_neighbor')
add('after-counter-resets','after:prior_after(w)+1','after:1nat','hook_neighbor')
add('hook-deletes-unconditionally','let changed=if purge!=0{delete_extent(w)}else{w};','let changed=if true{delete_extent(w)}else{w};','hook_neighbor')
add('hook-never-deletes','let changed=if purge!=0{delete_extent(w)}else{w};','let changed=if false{delete_extent(w)}else{w};','hook_neighbor')
add('extent-provider-keeps-rows','pub open spec fn delete_extent(w:World)->World {World{rows:Map::empty(),..w}}','pub open spec fn delete_extent(w:World)->World {w}','sdk_neighbor')
add('extent-provider-removes-class','pub open spec fn delete_extent(w:World)->World {World{rows:Map::empty(),..w}}','pub open spec fn delete_extent(w:World)->World {World{rows:Map::empty(),class:false,..w}}','sdk_neighbor')
add('class-provider-removes-extent','pub open spec fn delete_class(w:World)->World {World{class:false,..w}}','pub open spec fn delete_class(w:World)->World {World{class:false,rows:Map::empty(),..w}}','delete_neighbor')
add('class-provider-keeps-class','pub open spec fn delete_class(w:World)->World {World{class:false,..w}}','pub open spec fn delete_class(w:World)->World {w}','delete_neighbor')
add('uninstall-keeps-module','World{module:None,..after_class}','World{module:w.module,..after_class}','lifecycle_frame')
add('reload-clears-data','World{module:Some(c),class:true,..w}','World{module:Some(c),class:true,rows:Map::empty(),..w}','reload_neighbor')
add('reload-keeps-class-absent','World{module:Some(c),class:true,..w}','World{module:Some(c),class:w.class,..w}','reload_neighbor')
add('sql-count-always-zero','pub open spec fn row_count(rows:Rows)->int {if rows.contains_key(token()){1}else{0}}','pub open spec fn row_count(rows:Rows)->int {0}','reader_neighbor')
add('sql-value-always-zero','pub open spec fn row_value(rows:Rows)->int {if rows.contains_key(token()){rows[token()]}else{0}}','pub open spec fn row_value(rows:Rows)->int {0}','reader_neighbor')
add('sql-available-without-schema','if w.class{Some(Read{count:row_count(w.rows),value:row_value(w.rows)})}else{None}','if true{Some(Read{count:row_count(w.rows),value:row_value(w.rows)})}else{None}','delete_neighbor')
replay={
 'hook-purge-hook-purge':'observed_callback(removed(Case::Hook,Flag::Purge)).purge',
 'hook-config-hook-purge':'observed_callback(removed(Case::Hook,Flag::Configure)).purge',
 'hook-purge-hook-top':'observed_callback(removed(Case::Hook,Flag::Purge)).top',
}
for kind,case in [('plain','Plain'),('hook','Hook')]:
 for suffix,flag in [('default','Default'),('purge','Purge'),('config','Configure')]:
  key=kind+'-'+suffix;before='seeded(Case::'+case+')';gone='removed(Case::'+case+',Flag::'+flag+')';after='after(Case::'+case+',Flag::'+flag+')'
  for state,expr in [('before',before),('after',after)]:
   replay[key+'-'+state+'-count']='count('+expr+')';replay[key+'-'+state+'-value']='value('+expr+')'
  replay[key+'-module-gone']='b(('+gone+').module.is_none())'
  replay[key+'-class-gone']='b(!('+gone+').class)'
  if case=='Hook':
   for field in ['called','purge','top','nested','after']:
    replay[key+'-hook-'+field]='observed_callback('+gone+').'+field+' as int'
for state,expr in [('before','sdk_before()'),('after','sdk_after()')]:
 replay['calibration-'+state+'-count']='count('+expr+')';replay['calibration-'+state+'-value']='value('+expr+')'
replay['calibration-class-present']='b(sdk_after().class)'
native_rel='.g5/uplift/consolidated/iter27/native/20261116T012851413419Z/observations.json'
base=r.parents[3];native=json.loads((base/native_rel).read_text())['observations']
expected={}
for k in native:
 if k not in replay:expected[k]=0 if k=='final-tlevel' else 27 if k.endswith('-marker') else 1
assert set(native)==set(replay)|set(expected)
assert all(native[k]==v for k,v in expected.items())
cfg={'models':{'purge.rs':models},'dependency_controls':{},'control_functions':{'purge.rs':selectors},'control_options':['--multiple-errors','0'],'control_isolation_helper':'model-lane/isolate_witness.py','control_isolation_function':'isolate','focused_replay_barrier':True,'replay_model':'purge.rs','replay_grouped':True,'replay':replay,'replay_prelude':'assert(data_path(top_name())==top_key()) by(compute);assert(data_path(dotted_name())==nested_key()) by(compute);lifecycle_neighbors();','native_expected':expected,'native_receipt':native_rel,'scope_notes':[
 'Builder-selected public inherited P:F-UNIN-2 repair; no new defect/discovery or implemented product repair.',
 'One ordinary directory module/class, exact successful providers/no mapped resources/projections/locks/dependents; unchanged stable persistent schema.',
 'CLI path segmentation and selected alias/custom literal inputs modeled; no entire command lexer/parser/value conversion or custom no-dot zpm-prefix branch.',
 'Rows keyed by actual unique Token sentinel, Val271; no physical IRIS-ID or generic storage/SQL proof.',
 'Fixture installer callback behavior is explicit fixture code, not a product-default table purge policy.',
 'SDK class deletion/no-extent and extent deletion + compile/storage reattachment are qualified provider relations; provider controls do not establish owned-body proof.',
 'Native load/reset/seed/uninstall/reload/SQL/status/Marker/tlevel gates remain independent qualification.',
 'Deployed mapping lead stopped: no established XML-only SDK producer, no producer invoked; no deployed closure claim.',
 'Archive/package/verify/product-export boundary unassessed/unpursued; no investigation/reproduction/probe; private evaluator inputs not accessed.'
]}
(r/'purge-cases.json').write_text(json.dumps(cfg,indent=2)+'\n')
shutil.copyfile(r.parent/'iter26/model-lane/isolate_witness.py',r/'model-lane/isolate_witness.py')
print('controls',len(models),'selectors',len(set(selectors.values())),'replay',len(replay),'gates',len(expected))
