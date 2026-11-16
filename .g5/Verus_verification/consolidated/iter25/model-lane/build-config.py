from pathlib import Path
import json
r=Path('.g5/Verus_verification/consolidated/iter25');s=(r/'developer_sources.rs').read_text();models={};selectors={}
def c(n,a,b,f):
 assert s.count(a)==1,(n,s.count(a));models[n]=[a,b];selectors[n]=f
c('wrong-marker','pub open spec fn prefix()->Text {seq![8u8]}','pub open spec fn prefix()->Text {seq![9u8]}','constants_neighbor')
c('wrong-module-category','pub open spec fn module_category()->Text {seq![77u8,79,68,85,76,69]}','pub open spec fn module_category()->Text {seq![109u8,111,100,117,108,101]}','constants_neighbor')
c('constructor-omits-lower','let ext=lower(if r.extension_parameter.len()==0{suffix(r.internal)}else{r.extension_parameter});','let ext=if r.extension_parameter.len()==0{suffix(r.internal)}else{r.extension_parameter};','constructor_neighbor')
c('constructor-ignores-directory-attribute','directory:match r.directory_attribute{Some(d)=>if d.len()>0{d}else{ext},None=>ext}','directory:ext','constructor_neighbor')
c('constructor-loses-defined','directory_defined:r.directory_attribute.is_some()','directory_defined:false','constructor_neighbor')
c('constructor-keeps-resource-suffix','name:basename(r.internal)','name:r.internal','constructor_neighbor')
c('guard-ignores-existing-mode','Some(m)=>m.developer && !requested_developer(a)','Some(m)=>true && !requested_developer(a)','guard_neighbor')
c('guard-ignores-requested-mode','m.developer && !requested_developer(a)','m.developer','guard_neighbor')
c('guard-ignores-permission','!a.permit_downgrade && a.top_install','true && a.top_install','guard_neighbor')
c('guard-ignores-top-install','!a.permit_downgrade && a.top_install','!a.permit_downgrade && true','guard_neighbor')
c('perform-bypasses-guard','if overwrite_guard(w,c,a){Outcome','if false{Outcome','refusal_neighbor')
c('guard-reported-success','Outcome{world:w,kind:ResultKind::DeveloperGuard}','Outcome{world:w,kind:ResultKind::Loaded}','refusal_neighbor')
c('guard-erases-tracked-state','Outcome{world:w,kind:ResultKind::DeveloperGuard}','Outcome{world:initial(),kind:ResultKind::DeveloperGuard}','refusal_neighbor')
c('mode-writer-always-normal','let mode=effective_mode(w,c,a);','let mode=false;','writer_neighbor')
a='if mode{if c==Case::Implicit{World{a_module:Some(entry),no_folders:true,..w1}}else{World{b_module:Some(entry),no_folders:true,..w1}}}else{w1}'
c('no-folders-not-written',a,a.replace('no_folders:true','no_folders:false'),'writer_neighbor')
c('normal-clears-shared-global',a,a.replace('else{w1}','else{World{no_folders:false,..w1}}'),'writer_neighbor')
c('module-map-uses-source-root','value:prefix()+m.root','value:prefix()+source_directory(m)','writer_neighbor')
c('resource-ignores-dev','if developer && eligible && !generated','if true && eligible && !generated','resource_guard_neighbor')
c('resource-ignores-scope','developer && eligible && !generated','developer && true && !generated','resource_guard_neighbor')
c('resource-ignores-generated','eligible && !generated && root_exists','eligible && true && root_exists','resource_guard_neighbor')
c('resource-ignores-root','!generated && root_exists && p.extension','!generated && true && p.extension','resource_guard_neighbor')
c('resource-ignores-extension','root_exists && p.extension.len()>0 && p.name.len()>0','root_exists && true && p.name.len()>0','resource_guard_neighbor')
c('resource-ignores-name','p.extension.len()>0 && p.name.len()>0','p.extension.len()>0 && true','resource_guard_neighbor')
c('resource-key-uppercase','Key{category:p.extension,name:p.name+p.postfix}','Key{category:raw_cls(),name:p.name+p.postfix}','writer_neighbor')
c('resource-key-has-item-suffix','Key{category:p.extension,name:p.name+p.postfix}','Key{category:p.extension,name:p.name+seq![46u8,67,76,83]}','writer_neighbor')
c('wrong-finite-directory-binding','if starts(physical_file(c),category){category}else{source_directory(m)}','if true{category}else{source_directory(m)}','layout_neighbor')
c('dto-not-aware','Info{aware:true,resource_type:p.extension','Info{aware:false,resource_type:p.extension','reader_neighbor')
c('dto-wrong-type','resource_type:p.extension,prefix:p.name+p.postfix','resource_type:raw_cls(),prefix:p.name+p.postfix','reader_neighbor')
c('dto-import-directory-sidecar','relative:p.directory','relative:actual_import_directory(c)','reader_neighbor')
c('dto-fixture-name-sidecar','prefix:p.name+p.postfix,relative','prefix:class_name(c),relative','stored_input_neighbor')
c('path-fixture-name-sidecar','translate(basename(m.resource.internal))','translate(basename(item(c)))','stored_input_neighbor')
c('path-omits-declared-directory','m.sources_root+slash()+p.directory+slash()+translate','m.sources_root+slash()+translate','reader_neighbor')
c('sync-omits-sources-root','m.sources_root+slash()+construct(m.resource).directory','construct(m.resource).directory','reader_neighbor')
c('root-reader-fixture-sidecar','pub open spec fn stored_root(w:World,c:Case)->Text {match module(w,c){Some(m)=>m.root,None=>seq![]}}','pub open spec fn stored_root(w:World,c:Case)->Text {root(c)}','metadata_reader_neighbor')
c('version-reader-fixture-sidecar','pub open spec fn stored_version(w:World,c:Case)->Text {match module(w,c){Some(m)=>m.version,None=>seq![]}}','pub open spec fn stored_version(w:World,c:Case)->Text {one()}','metadata_reader_neighbor')
n=Path('.g5/uplift/consolidated/iter25/native/20261116T001150893638Z/observations.json');obs=json.loads(n.read_text())['observations'];replay={}
def add(k,v):assert k in obs,k;replay[k]=v
# First three are actual opposite-capable semantic statuses.
for k,v in [('reinstall-dev','b(ok(developer()))'),('reinstall-normal-after-dev','b(ok(refused()))'),('reinstall-explicit-dev','b(ok(explicit_developer()))'),('install-normal','b(ok(normal()))'),('install-explicit-normal','b(ok(explicit_normal()))'),('reinstall-explicit-normal-after-dev','b(ok(explicit_refused()))')]:add(k,v)
for label,w,C in [('normal','normal().world','Implicit'),('dev','developer().world','Implicit'),('refused','refused().world','Implicit'),('explicit-normal','explicit_normal().world','Explicit'),('explicit-dev','explicit_developer().world','Explicit'),('explicit-refused','explicit_refused().world','Explicit')]:
 case='Case::'+C;d='cls()' if C=='Implicit' else 'code()';rm=f'resource_mapping({w},{case})';mm=f'module_mapping({w},{case})';p=f'processor({w},{case}).unwrap()';info=f'source_info({w},{case}).unwrap()'
 fields={
  'mode':f'b(stored_mode({w},{case}))','version':f'b(stored_version({w},{case})==one())','root':f'b(stored_root({w},{case})==root({case}))',
  'class-map':f'b(({rm}).is_some())','module-map':f'b(({mm}).is_some())','module-no-folders':f'b(({w}).no_folders)',
  'class-map-prefix':f'b(({rm}).is_some() && ({rm}).unwrap().len()>0 && ({rm}).unwrap()[0]==8u8)',
  'module-map-prefix':f'b(({mm}).is_some() && ({mm}).unwrap().len()>0 && ({mm}).unwrap()[0]==8u8)',
  'class-map-target':f'b(({rm})==Some(seq![8u8]+actual_import_directory({case})))',
  'module-map-target':f'b(({mm})==Some(seq![8u8]+root({case})))',
  'processor-directory':f'b(({p}).directory=={d})','directory-defined':f'b(({p}).directory_defined)',
  'sources-root':f'b(module({w},{case}).unwrap().sources_root==src())',
  'info-directory':f'b(({info}).relative=={d})','sync-directory':f'b(sync_directory({w},{case})==expected_sync({case}))',
  'product-declared-path':f'b(declared_relative({w},{case})==expected_relative({case}))',
  'info-aware':f'b(({info}).aware)','info-type':f'b(({info}).resource_type==cls())','info-prefix':f'b(({info}).prefix==class_name({case}))',
  'product-path':f'b(declared_absolute({w},{case})==physical_file({case}))','product-file':f'b(file_exists(declared_absolute({w},{case})))'}
 for suffix,expr in fields.items():add(label+'-'+suffix,expr)
cfg={'models':{'developer_sources.rs':models},'dependency_controls':{},'control_functions':{'developer_sources.rs':selectors},'control_options':['--multiple-errors','0'],'control_isolation_helper':'model-lane/isolate_witness.py','control_isolation_function':'isolate','replay_model':'developer_sources.rs','replay_grouped':True,'replay':replay,'replay_prelude':'lifecycle_staged();constructor_neighbor();layout_neighbor();reader_neighbor();constants_neighbor();','native_expected':{k:v for k,v in obs.items() if k not in replay},'native_receipt':str(n),'scope_notes':['Two ordinary Class layouts, same1.0.0, no dependencies, successful metadata/file/class providers; caller/currentversion seam source-reused22, no unrelated imports.','Mode/root/MODULE writer is Lifecycle.Base.%Reload; initial CheckBeforeClean label was wrong and uncredited.','Typed Sources entries and product declared-layout readers are separate branches; no confirmed external modern Sources consumer, legacy SDK nulls not a product defect.','Finite directory applicability ONLY implicit0+missingcls and explicit1+existingcode; DirectoryDefined constructor/readback only; file_exists is static fixture membership.','Byte-prefix and exact-map-target replay expectations use literal8 independent of writer prefix helper.','Metadata projected from stored resource via equivalent processor properties; no fresh allocation/cache invalidation claim.','Present/supplier/code/payload/IRISresource-factory availability/file contents remain native qualification only.','Trigger queue, installer/Invoke flags, generic paths/saves/compilers/other errors and policies unassessed.','Excluded boundaries unassessed/unpursued; no private evaluator inputs accessed.'],'control_scope_notes':{'wrong-finite-directory-binding':'Finite provider-binding contrast only; no generic directory selection algorithm or DirectoryDefined guard credit.'}}
(r/'developer-source-cases.json').write_text(json.dumps(cfg,indent=2)+'\n');print(len(models),'controls',len(set(selectors.values())),'selectors',len(replay),'aliases',len(cfg['native_expected']),'gates')
