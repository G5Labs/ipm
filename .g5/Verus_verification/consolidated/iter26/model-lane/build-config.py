from pathlib import Path
import json
r=Path('.g5/Verus_verification/consolidated/iter26');s=(r/'sync_summary.rs').read_text();models={};selectors={}
def c(n,a,b,f):
 assert s.count(a)==1,(n,s.count(a));models[n]=[a,b];selectors[n]=f
c('inactive-records','if !s.active{s}else','if false{s}else','inactive_neighbor')
c('active-does-not-record','if !s.active{s}else','if true{s}else','append_neighbor')
c('counter-not-incremented','let sequence=count(s,module)+1','let sequence=count(s,module)','append_neighbor')
c('counter-global-module','let sequence=count(s,module)+1','let sequence=count(s,zeta())+1','append_neighbor')
c('counter-wrong-module-key','counts:s.counts.insert(module,sequence)','counts:s.counts.insert(zeta(),sequence)','append_neighbor')
c('row-wrong-module-key','rows:s.rows.insert((module,sequence),class)','rows:s.rows.insert((alpha(),sequence),class)','append_neighbor')
c('row-old-sequence','rows:s.rows.insert((module,sequence),class)','rows:s.rows.insert((module,count(s,module)),class)','append_neighbor')
c('row-erases-class','rows:s.rows.insert((module,sequence),class)','rows:s.rows.insert((module,sequence),seq![])','append_neighbor')
c('append-erases-old-rows','rows:s.rows.insert((module,sequence),class)','rows:Map::<(Text,nat),Text>::empty().insert((module,sequence),class)','duplicate_neighbor')
c('counter-resets-each-call','let sequence=count(s,module)+1','let sequence=1nat','duplicate_neighbor')
c('duplicate-class-deduplicated','if !s.active{s}else','if !s.active || row_value(s,module,1)==Some(class){s}else','duplicate_neighbor')
c('begin-retains-counts','pub open spec fn begin(_old:Store)->Store {Store{active:true,counts:Map::empty(),rows:Map::empty()}}','pub open spec fn begin(_old:Store)->Store {Store{active:true,counts:_old.counts,rows:Map::empty()}}','clear_neighbor')
c('begin-retains-rows','pub open spec fn begin(_old:Store)->Store {Store{active:true,counts:Map::empty(),rows:Map::empty()}}','pub open spec fn begin(_old:Store)->Store {Store{active:true,counts:Map::empty(),rows:_old.rows}}','clear_neighbor')
c('end-only-deactivates','pub open spec fn end(_old:Store)->Store {empty()}','pub open spec fn end(_old:Store)->Store {Store{active:false,.._old}}','clear_neighbor')
c('end-keeps-active','pub open spec fn end(_old:Store)->Store {empty()}','pub open spec fn end(_old:Store)->Store {Store{active:true,..empty()}}','clear_neighbor')
c('named-sync-begins','module_argument.len()==0 && ordered.len()>0','true && ordered.len()>0','tracking_neighbor')
c('empty-list-sync-begins','module_argument.len()==0 && ordered.len()>0','module_argument.len()==0 && true','tracking_neighbor')
c('finish-does-not-clear','if started{end(s)}else{s}','if started{s}else{s}','tracking_neighbor')
c('verify-phase-treated-as-test','r.phases.contains(test_phase()) || i>=classes.len()','r.phases.contains(verify_phase()) || i>=classes.len()','selected_neighbor')
c('test-phase-recorded-as-skip','r.phases.contains(test_phase()) || i>=classes.len()','false || i>=classes.len()','selected_neighbor')
c('selected-owner-replaced-by-root','Call{module:r.owner_module,class:classes[i as int]}','Call{module:zeta(),class:classes[i as int]}','selected_owner_neighbor')
c('selected-class-replaced-by-owner','Call{module:r.owner_module,class:classes[i as int]}','Call{module:r.owner_module,class:r.owner_module}','selected_owner_neighbor')
c('selected-class-loop-truncated','i>=classes.len(){seq![]}else{seq![Call','i>=classes.len() || i>0{seq![]}else{seq![Call','selected_neighbor')
c('report-ignores-active','if !s.active{seq![]}else{report_from','if false{seq![]}else{report_from','inactive_neighbor')
c('report-forces-module-order','let m=names[i as int];','let m=if i==0{alpha()}else{zeta()};','report_neighbor')
c('report-skips-first-sequence','rows_from(s,m,1)','rows_from(s,m,2)','report_neighbor')
c('report-class-is-module','class:s.rows[(module,n)]','class:module','report_neighbor')
c('report-stops-before-last-row','if n>count(s,module){seq![]}','if n>=count(s,module){seq![]}','report_neighbor')
n=Path('.g5/uplift/consolidated/iter26/native/20261116T010227557083Z/observations.json');obs=json.loads(n.read_text())['observations'];replay={}
def add(k,v):assert k in obs,k;replay[k]=v
for k,v in [('inactive-active','b(inactive().active)'),('begun-active','b(begun().active)'),('a1-zeta-class1-exists','b(has_row(a1(),zeta(),1))')]:add(k,v)
for label,f in [('inactive','inactive()'),('begun','begun()'),('a1','a1()'),('b1','b1()'),('a2','a2()'),('a3','a3()'),('after-report','after_report()'),('after-end','after_end()'),('after-inactive','after_inactive()'),('reset-empty','reset_empty()'),('reset','reset()')]:
 fields={'active':f'b(({f}).active)','module-count':f'module_count({f}) as int','zeta-count':f'count({f},zeta()) as int','alpha-count':f'count({f},alpha()) as int','zeta-rows':f'rows_from({f},zeta(),1).len() as int','zeta-class1':f'b(matching({f},zeta(),1,verify_z()))','zeta-class2':f'b(matching({f},zeta(),2,verify_a()))','zeta-class3':f'b(matching({f},zeta(),3,verify_z()))','alpha-class1':f'b(matching({f},alpha(),1,alpha_class()))','zeta-class1-exists':f'b(has_row({f},zeta(),1))','alpha-class1-exists':f'b(has_row({f},alpha(),1))'}
 for suffix,v in fields.items():add(label+'-'+suffix,v)
# Inactive header-presence stays native-only; projection is skipped lines.
add('report-zeta-module-order','b(forward()[0].module==zeta() && forward()[3].module==alpha())')
add('report-alpha-module-order','b(reverse()[0].module==alpha() && reverse()[1].module==zeta())')
add('report-zeta-sequence-duplicates','b(forward()[0].class==verify_z() && forward()[1].class==verify_a() && forward()[2].class==verify_z())')
add('cli-single-skipped','b(verbose_skip_count(cli_resource(),seq![cli_class()],true)>0)')
# Aggregate header format/presence stays native-only.
# Tracking producer is modeled, but header is not an alias of row nonemptiness.
add('cli-all-reported-class','b(cli_all_report()==seq![Row{module:cli_module(),sequence:1,class:cli_class()}])')
cfg={'models':{'sync_summary.rs':models},'dependency_controls':{},'control_functions':{'sync_summary.rs':selectors},'control_options':['--multiple-errors','0'],'control_isolation_helper':'model-lane/isolate_witness.py','control_isolation_function':'isolate','focused_replay_barrier':True,'replay_model':'sync_summary.rs','replay_grouped':True,'replay':replay,'replay_prelude':'staged_store();report_neighbor();selected_neighbor();caller_neighbor();','native_expected':{k:v for k,v in obs.items() if k not in replay},'native_receipt':str(n),'scope_notes':['Disclosed public repair, not independent discovery; separateIRISprivate sample not accessed.','One-job process-private Store, explicit per-module numericincrement/default0, generated-prefix wfReport domain.','module_count is skipped-only DIRECTAPI store projection, not actualCLI allSummarymodulebucket count.','Report alias is skippedclass/module/sequence projection; otherfields/header/formatting remain outside.','Conditional selected verify resource/class group with rawPhase list; full byName/Owns assignment, callback effects/statuses and testsunassessed.','CLI reloadmarkers/statuses/testcounter0/availability stay native-only; no fakealiasofcallback_selected.','After-report Storeidentity is source-inspected/native-confirmed frame, not mechanicallyproved no-mutation body.','Priorunintendedverify crossed excluded boundary; stopped, rawretainedunassessed/uncredited; no source/probe/model followup.','No SQL/persistent rows/opaqueIDs/wholeCLIproof; no currentPrivateinputs or excluded investigation.']}
(r/'sync-summary-cases.json').write_text(json.dumps(cfg,indent=2)+'\n');print(len(models),'controls',len(set(selectors.values())),'selectors',len(replay),'aliases',len(cfg['native_expected']),'gates')
