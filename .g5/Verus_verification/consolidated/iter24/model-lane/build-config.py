from pathlib import Path
import json
r=Path('.g5/Verus_verification/consolidated/iter24');s=(r/'snapshot_dependencies.rs').read_text();models={};selectors={}
def c(n,a,b,f):
 assert s.count(a)==1,(n,s.count(a));models[n]=[a,b];selectors[n]=f
c('snapshot-never','inherited_semver::snapshot(inherited_semver::v(v))','false','classification_neighbor')
c('snapshot-always','inherited_semver::snapshot(inherited_semver::v(v))','true','classification_neighbor')
c('snapshot-case-sensitive','inherited_semver::snapshot(inherited_semver::v(v))',"inherited_semver::v(v).build==seq!['s','n','a','p','s','h','o','t']",'classification_neighbor')
c('snapshot-uses-prerelease','inherited_semver::snapshot(inherited_semver::v(v))',"inherited_semver::v(v).pre==seq!['s','n','a','p','s','h','o','t']",'classification_neighbor')
c('default-refresh-off','match c.snapshots{Some(v)=>v,None=>true}','match c.snapshots{Some(v)=>v,None=>false}','request_neighbor')
c('shallow-override-ignored','match c.snapshots{Some(v)=>v,None=>true}','true','request_neighbor')
c('permit-override-ignored','permit_downgrade:c.permit_downgrade','permit_downgrade:false','request_neighbor')
c('request-from-source-name','run(w,w.parent.dependency_version,params(c))','run(w,if w.source.name==name(Case::Snapshot){snap()}else{one()},params(c))','request_neighbor')
c('parent-version-hardcoded','parent_requested:w.parent.version','parent_requested:one()','request_neighbor')
c('ignore-installed-name','m.name==w.parent.dependency_name && satisfies','true && satisfies','reuse_neighbor')
c('ignore-installed-membership','satisfies(m.version,exact(request)) && (prior.len()','true && (prior.len()','reuse_neighbor')
c('ignore-prior-version','(prior.len()==0 || prior==m.version)','true','reuse_neighbor')
c('ignore-snapshot-force','!(is_snapshot(m.version) && p.force_snapshots)','true','reuse_neighbor')
c('force-reloads-release','!(is_snapshot(m.version) && p.force_snapshots)','!p.force_snapshots','reuse_neighbor')
c('omit-generated-lower','if !p.ignore_installed && !p.permit_downgrade','if false','decision_neighbor')
c('ignore-permit-downgrade','if !p.ignore_installed && !p.permit_downgrade','if !p.ignore_installed','decision_neighbor')
c('lower-from-parent-version','lower(m.version),true,false','lower(w.parent.version),true,false','decision_neighbor')
c('exact-drops-build','seq![inherited_semver::expand(v)]','seq![inherited_semver::expand(one())]','intersection_neighbor')
c('reuse-node-has-alpha','Node{name:m.name,repository:seq![],version:m.version}','Node{name:m.name,repository:alpha(),version:m.version}','decision_neighbor')
c('overlap-error-still-searches','inherited_semver::ExprAnd::NoOverlap=>Graph{node:None,error:Some(Error::NoOverlap)}','inherited_semver::ExprAnd::NoOverlap=>searched(w,exact(request))','decision_neighbor')
c('flat-loses-repository','Node{name:n.name,repository:n.repository,version:n.version}','Node{name:n.name,repository:seq![],version:n.version}','flat_neighbor')
c('flat-loses-version','Node{name:n.name,repository:n.repository,version:n.version}','Node{name:n.name,repository:n.repository,version:seq![]}','flat_neighbor')
c('consumer-loads-empty-sentinel','if n.repository.len()==0{w}','if n.repository.len()==0{successful_load(w,w.source)}','load_neighbor')
c('consumer-skips-qualified-load','Some(d)=>successful_load(w,d)','Some(d)=>w','load_neighbor')
c('writer-keeps-old-code','root:source.root,code:source.code','root:source.root,code:w.installed.unwrap().code','load_neighbor')
c('writer-erases-build','Installed{name:source.name,version:source.version','Installed{name:source.name,version:one()','load_neighbor')
c('writer-erases-root','root:source.root,code:source.code','root:seq![],code:source.code','load_neighbor')
c('parent-reads-physical-code','if w.parent.target==m.code.class{Some(m.code.who)}','if w.parent.target==m.code.class{Some(w.source.code.who)}','load_neighbor')
c('root-reader-uses-source','pub open spec fn installed_root(w:World)->Text {match w.installed{None=>seq![],Some(m)=>m.root}}','pub open spec fn installed_root(w:World)->Text {w.source.root}','reader_neighbor')
c('supplier-reader-uses-source','pub open spec fn installed_repository(w:World)->Text {match w.installed{None=>seq![],Some(m)=>m.repository}}','pub open spec fn installed_repository(w:World)->Text {w.source.repository}','reader_neighbor')
n=Path('.g5/uplift/consolidated/iter24/native/20261115T234149601861Z/observations.json');obs=json.loads(n.read_text())['observations'];replay={}
def add(k,v):assert k in obs,k;replay[k]=v
for k,v in [('shallow-snap','b(ok(shallow()))'),('default-snap','b(ok(default_snapshot()))'),('permit-snap','b(ok(permitted()))'),('default-release','b(ok(default_release()))')]:add(k,v)
add('semver-build',"b(inherited_semver::parse(snap()).build==seq!['s','n','a','p','s','h','o','t'])");add('semver-snapshot','b(is_snapshot(snap()))');add('semver-exact-satisfied','b(satisfies(snap(),exact(snap())))');add('semver-lower-satisfied','b(satisfies(snap(),lower(snap())))');add('semver-and-rejected','b(inherited_semver::expr_and(exact(snap()),lower(snap()),true,false) is NoOverlap)')
for label,w,case in [('initial-snap','initial(Case::Snapshot)','Snapshot'),('initial-release','initial(Case::Release)','Release'),('changed-snap','changed(Case::Snapshot)','Snapshot'),('changed-release','changed(Case::Release)','Release'),('shallow-snap','shallow().world','Snapshot'),('default-snap','default_snapshot().world','Snapshot'),('permit-snap','permitted().world','Snapshot'),('default-release','default_release().world','Release')]:
 C='Case::'+case
 for suffix,expr in [('dep-present',f'b(({w}).installed.is_some() && installed_name({w})==name({C}))'),('dep-version',f'b(installed_version({w})==required({C}))'),('dep-snapshot',f'b(is_snapshot(installed_version({w})))'),('dep-build',f'b(inherited_semver::parse(installed_version({w})).build==inherited_semver::parse(required({C})).build)'),('dep-supplier',f'b(installed_repository({w})==alpha())'),('dep-root',f'b(installed_root({w})==root({C}))'),('dep-code',f'b(({w}).installed.is_some() && ({w}).installed.unwrap().code.class==class({C}))'),('dep-payload',f'dep_payload({w}).unwrap()'),('parent-dep-version',f'b(({w}).parent.dependency_version==required({C}))'),('parent-payload',f'parent_payload({w}).unwrap()')]:add(label+'-'+suffix,expr)
add('physical-snap-two','b(changed(Case::Snapshot).source.code.who==2)');add('physical-release-two','b(changed(Case::Release).source.code.who==2)')
for label,g,C in [('snap-no-force','no_force()','Snapshot'),('snap-force','force()','Snapshot'),('snap-force-permit','force_permit()','Snapshot'),('release-force','release_force()','Release')]:
 add(label+'-status',f'b(graph_ok({g}))');add(label+'-entry',f'b(({g}).node.is_some())')
 if label=='snap-force':continue
 for suffix,expr in [('reuse',f'b(({g}).node.unwrap().repository.len()==0)'),('alpha',f'b(({g}).node.unwrap().repository==alpha())'),('version',f'b(({g}).node.unwrap().version==required(Case::{C}))'),('flat-reuse',f'b(flat_selected({g})[0].repository.len()==0)'),('flat-alpha',f'b(flat_selected({g})[0].repository==alpha())'),('flat-version',f'b(flat_selected({g})[0].version==required(Case::{C}))'),('flat-count',f'flat_selected({g}).len() as int')]:add(label+'-'+suffix,expr)
cfg={'models':{'snapshot_dependencies.rs':models},'dependency_controls':{},'control_functions':{'snapshot_dependencies.rs':selectors},'control_options':['--multiple-errors','0'],'control_isolation_helper':'model-lane/isolate_witness.py','control_isolation_function':'isolate','replay_model':'snapshot_dependencies.rs','replay_grouped':True,'replay':replay,'replay_prelude':'lifecycle_staged();literal_fields();literal_membership();classification_neighbor();decision_neighbor();flat_neighbor();','native_expected':{k:v for k,v in obs.items() if k not in replay},'native_receipt':str(n),'scope_notes':['One exact dependency and stable manifest/version; code-only physical edit.','Stored parent reference is the request producer; collection count/SQL/XML/class providers independently qualified.','Default snapshot failure derives installed-version lower bound plus unchanged inherited AND, not a supplied failure flag.','Selected flat reference only, not entire graph/sort/count.','Same-full-version or fresh successful directory effect only; selected_load_domain is a scope annotation, not a production guard.','Whole top-level parent load/status/reporting, other constraints/errors and concurrency remain unassessed.','Private evaluator inputs never accessed; excluded delivery boundaries remain unassessed/unpursued.']}
(r/'snapshot-dependency-cases.json').write_text(json.dumps(cfg,indent=2)+'\n');print(len(models),'controls',len(set(selectors.values())),'selectors',len(replay),'aliases',len(cfg['native_expected']),'gates')
