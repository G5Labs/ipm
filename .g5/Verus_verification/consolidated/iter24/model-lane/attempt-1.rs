// IPM571e04a1. Selected one-dependency projection, ordinary FS directory provider.
// Versions/expressions and IsSnapshot actively reuse inherited semver contracts.
// XML/IRIS parsing, stable FS lookup, successful compile/load and class dispatch
// remain qualified providers. No complete graph/transaction/caller proof.
// Default refresh prediction was REFUTED: exact snapshot AND generated >=snapshot
// fails inherited intersection despite actual installed version satisfying both.
// Excluded delivery boundaries remain UNASSESSED/UNPURSUED; no representation.
#[path="../verus/semver.rs"]
pub mod inherited_semver;
use vstd::prelude::*;
verus! {
pub type Text=Seq<char>;
#[derive(PartialEq,Eq)]
pub enum Case {Snapshot,Release}
pub struct Code {pub class:Text,pub who:int}
pub struct Package {pub name:Text,pub version:Text,pub repository:Text,pub root:Text,pub code:Code}
pub struct Installed {pub name:Text,pub version:Text,pub repository:Text,pub root:Text,pub code:Code}
// Target is the literal parent Who() delegation parsed from the selected fixture.
// Actual parent class availability/compile/metadata remain separate native gates.
pub struct Parent {pub name:Text,pub version:Text,pub target:Text}
pub struct World {pub source:Package,pub installed:Option<Installed>,pub parent:Parent}
pub struct Params {pub force_snapshots:bool,pub permit_downgrade:bool,pub ignore_installed:bool}
pub struct Command {pub snapshots:Option<bool>,pub permit_downgrade:bool}
pub struct Node {pub name:Text,pub repository:Text,pub version:Text}
#[derive(PartialEq,Eq)]
pub enum Error {NoOverlap,ExactConflict,NoCandidate}
pub struct Graph {pub node:Option<Node>,pub error:Option<Error>}
pub struct Outcome {pub world:World,pub graph:Graph,pub requested:Text}
pub open spec fn one()->Text {seq!['1','.','0','.','0']}
pub open spec fn snap()->Text {seq!['1','.','0','.','0','+','s','n','a','p','s','h','o','t']}
pub open spec fn required(c:Case)->Text {if c==Case::Snapshot{snap()}else{one()}}
pub open spec fn alpha()->Text {seq!['a','l','p','h','a']}
pub open spec fn name(c:Case)->Text {if c==Case::Snapshot{seq!['i','t','e','r','2','4','-','s','n','a','p']}else{seq!['i','t','e','r','2','4','-','r','e','l','e','a','s','e']}}
pub open spec fn parent_name(c:Case)->Text {if c==Case::Snapshot{seq!['i','t','e','r','2','4','-','p','a','r','e','n','t','-','s','n','a','p']}else{seq!['i','t','e','r','2','4','-','p','a','r','e','n','t','-','r','e','l','e','a','s','e']}}
pub open spec fn class(c:Case)->Text {if c==Case::Snapshot{seq!['I','t','e','r','2','4','S','n','a','p','.','V','a','l','u','e']}else{seq!['I','t','e','r','2','4','R','e','l','e','a','s','e','.','V','a','l','u','e']}}
pub open spec fn root(c:Case)->Text {seq!['/','t','m','p','/','i','t','e','r','2','4','/','r','e','p','o','/']+name(c)+seq!['/']}
pub open spec fn package(c:Case)->Package {Package{name:name(c),version:required(c),repository:alpha(),root:root(c),code:Code{class:class(c),who:1}}}
pub open spec fn empty(c:Case)->World {World{source:package(c),installed:None,parent:Parent{name:parent_name(c),version:one(),target:class(c)}}}
pub open spec fn defaults()->Params {Params{force_snapshots:false,permit_downgrade:false,ignore_installed:false}}
pub open spec fn command()->Command {Command{snapshots:None,permit_downgrade:false}}
// Source-reused22 flag rule, without importing unrelated repository/history worlds.
// CLI -shallow -> explicit false and -DPermitDowngrade=1 -> true are parser ports.
pub open spec fn params(c:Command)->Params {Params{force_snapshots:match c.snapshots{Some(v)=>v,None=>true},permit_downgrade:c.permit_downgrade,ignore_installed:false}}
pub open spec fn exact(v:Text)->inherited_semver::Expr {seq![inherited_semver::expand(v)]}
pub open spec fn lower(v:Text)->inherited_semver::Expr {seq![inherited_semver::expand(seq!['>','=']+v)]}
pub open spec fn satisfies(v:Text,e:inherited_semver::Expr)->bool {inherited_semver::expr_satisfied(e,inherited_semver::v(v),false,0)}
pub open spec fn is_snapshot(v:Text)->bool {inherited_semver::snapshot(inherited_semver::v(v))}
pub open spec fn installed_valid(w:World,request:Text,prior:Text)->bool {match w.installed{None=>false,Some(m)=>satisfies(m.version,exact(request)) && (prior.len()==0 || prior==m.version)}}
pub open spec fn reuse(w:World,request:Text,prior:Text,p:Params)->bool {match w.installed{None=>false,Some(m)=>!p.ignore_installed && installed_valid(w,request,prior) && !(is_snapshot(m.version) && p.force_snapshots)}}
pub open spec fn accumulated(w:World,request:Text,p:Params)->inherited_semver::ExprAnd {match w.installed{
 Some(m)=>if !p.ignore_installed && !p.permit_downgrade{inherited_semver::expr_and(exact(request),lower(m.version),true,false)}else{inherited_semver::ExprAnd::Good(exact(request))},
 None=>inherited_semver::ExprAnd::Good(exact(request))}}
// Single valid catalog row, stable manifest version, exact dependency; other
// constraints/search ordering/providers are outside this selected projection.
pub open spec fn searched(w:World,e:inherited_semver::Expr)->Graph {
 if satisfies(w.source.version,e){Graph{node:Some(Node{name:w.source.name,repository:w.source.repository,version:w.source.version}),error:None}}
 else{Graph{node:None,error:Some(Error::NoCandidate)}}
}
pub open spec fn build(w:World,request:Text,prior:Text,p:Params)->Graph {
 if reuse(w,request,prior,p){let m=w.installed.unwrap();Graph{node:Some(Node{name:m.name,repository:seq![],version:m.version}),error:None}}
 else{match accumulated(w,request,p){inherited_semver::ExprAnd::NoOverlap=>Graph{node:None,error:Some(Error::NoOverlap)},
 inherited_semver::ExprAnd::ExactConflict=>Graph{node:None,error:Some(Error::ExactConflict)},
 inherited_semver::ExprAnd::Good(e)=>searched(w,e)}}
}
pub open spec fn graph_ok(g:Graph)->bool {g.error.is_none() && g.node.is_some()}
// Selected dependency reference projection through inversion+flatten. A root
// reference may also exist; count/order of the complete graph is not represented.
pub open spec fn flat_selected(g:Graph)->Seq<Node> {match g.node{None=>seq![],Some(n)=>seq![Node{name:n.name,repository:n.repository,version:n.version}]}}
pub open spec fn directory(w:World,n:Node)->Option<Package> {if n.name==w.source.name && n.repository==w.source.repository && n.version==w.source.version{Some(w.source)}else{None}}
// Successful normal directory-load provider: stored fields/compiled method come
// from the actual selected physical package, not a final payload outcome input.
pub open spec fn successful_load(w:World,source:Package)->World {World{installed:Some(Installed{name:source.name,version:source.version,repository:source.repository,root:source.root,code:source.code}),..w}}
pub open spec fn consume(w:World,refs:Seq<Node>,i:nat)->World
 decreases if i<refs.len(){(refs.len()-i) as nat}else{0nat}
{if i>=refs.len(){w}else{let n=refs[i as int];let next=if n.repository.len()==0{w}else{match directory(w,n){Some(d)=>successful_load(w,d),None=>w}};consume(next,refs,i+1)}}
pub open spec fn run(w:World,request:Text,p:Params)->Outcome {let g=build(w,request,seq![],p);Outcome{world:if graph_ok(g){consume(w,flat_selected(g),0)}else{w},graph:g,requested:request}}
pub open spec fn initial(c:Case)->World {run(empty(c),required(c),defaults()).world}
pub open spec fn edit(w:World)->World {World{source:Package{code:Code{who:2,..w.source.code},..w.source},..w}}
pub open spec fn changed(c:Case)->World {edit(initial(c))}
pub open spec fn reinstall(w:World,c:Command)->Outcome {run(w,if w.source.name==name(Case::Snapshot){snap()}else{one()},params(c))}
pub open spec fn shallow()->Outcome {reinstall(changed(Case::Snapshot),Command{snapshots:Some(false),..command()})}
pub open spec fn default_snapshot()->Outcome {reinstall(shallow().world,command())}
pub open spec fn permitted()->Outcome {reinstall(default_snapshot().world,Command{permit_downgrade:true,..command()})}
pub open spec fn default_release()->Outcome {reinstall(changed(Case::Release),command())}
pub open spec fn dep_payload(w:World)->Option<int> {match w.installed{None=>None,Some(m)=>Some(m.code.who)}}
pub open spec fn parent_payload(w:World)->Option<int> {match w.installed{None=>None,Some(m)=>if w.parent.target==m.code.class{Some(m.code.who)}else{None}}}
pub open spec fn installed_version(w:World)->Text {match w.installed{None=>seq![],Some(m)=>m.version}}
pub open spec fn installed_repository(w:World)->Text {match w.installed{None=>seq![],Some(m)=>m.repository}}
pub open spec fn installed_root(w:World)->Text {match w.installed{None=>seq![],Some(m)=>m.root}}
pub open spec fn ok(o:Outcome)->bool {graph_ok(o.graph)}
pub open spec fn b(v:bool)->int {if v{1}else{0}}

// Definition-only witnesses; isolation helper removes unrelated local proofs.
pub proof fn classification_neighbor()
 ensures is_snapshot(snap()),!is_snapshot(one()),
 is_snapshot(seq!['1','.','0','.','0','+','S','N','A','P','S','H','O','T']),
 !is_snapshot(seq!['1','.','0','.','0','-','s','n','a','p','s','h','o','t'])
{assert(is_snapshot(snap())) by(compute);assert(!is_snapshot(one())) by(compute);
 assert(is_snapshot(seq!['1','.','0','.','0','+','S','N','A','P','S','H','O','T'])) by(compute);
 assert(!is_snapshot(seq!['1','.','0','.','0','-','s','n','a','p','s','h','o','t'])) by(compute);}
pub proof fn intersection_neighbor()
 ensures satisfies(snap(),exact(snap())),satisfies(snap(),lower(snap())),
 inherited_semver::expr_and(exact(snap()),lower(snap()),true,false) is NoOverlap,
 inherited_semver::expr_and(exact(one()),lower(one()),true,false) is Good
{assert(satisfies(snap(),exact(snap()))) by(compute);assert(satisfies(snap(),lower(snap()))) by(compute);
 assert(inherited_semver::expr_and(exact(snap()),lower(snap()),true,false) is NoOverlap) by(compute);
 assert(inherited_semver::expr_and(exact(one()),lower(one()),true,false) is Good) by(compute);}
pub proof fn reuse_neighbor()
 ensures installed_valid(initial(Case::Snapshot),snap(),seq![]),
 !installed_valid(initial(Case::Snapshot),one(),seq![]),
 !installed_valid(initial(Case::Snapshot),snap(),one()),
 reuse(changed(Case::Snapshot),snap(),seq![],defaults()),
 !reuse(changed(Case::Snapshot),snap(),seq![],params(command())),
 reuse(changed(Case::Release),one(),seq![],params(command()))
{assert(installed_valid(initial(Case::Snapshot),snap(),seq![])) by(compute);
 assert(!installed_valid(initial(Case::Snapshot),one(),seq![])) by(compute);
 assert(!installed_valid(initial(Case::Snapshot),snap(),one())) by(compute);
 assert(reuse(changed(Case::Snapshot),snap(),seq![],defaults())) by(compute);
 assert(!reuse(changed(Case::Snapshot),snap(),seq![],params(command()))) by(compute);
 assert(reuse(changed(Case::Release),one(),seq![],params(command()))) by(compute);}
pub proof fn lifecycle_neighbor()
 ensures dep_payload(initial(Case::Snapshot))==Some(1int),dep_payload(changed(Case::Snapshot))==Some(1int),
 ok(shallow()),dep_payload(shallow().world)==Some(1int),
 !ok(default_snapshot()),default_snapshot().graph.error==Some(Error::NoOverlap),dep_payload(default_snapshot().world)==Some(1int),
 ok(permitted()),dep_payload(permitted().world)==Some(2int),parent_payload(permitted().world)==Some(2int),
 ok(default_release()),dep_payload(default_release().world)==Some(1int)
{assert(dep_payload(initial(Case::Snapshot))==Some(1int)) by(compute);assert(dep_payload(changed(Case::Snapshot))==Some(1int)) by(compute);
 assert(ok(shallow())) by(compute);assert(dep_payload(shallow().world)==Some(1int)) by(compute);
 assert(!ok(default_snapshot())) by(compute);assert(default_snapshot().graph.error==Some(Error::NoOverlap)) by(compute);assert(dep_payload(default_snapshot().world)==Some(1int)) by(compute);
 assert(ok(permitted())) by(compute);assert(dep_payload(permitted().world)==Some(2int)) by(compute);assert(parent_payload(permitted().world)==Some(2int)) by(compute);
 assert(ok(default_release())) by(compute);assert(dep_payload(default_release().world)==Some(1int)) by(compute);}
}
fn main(){}
