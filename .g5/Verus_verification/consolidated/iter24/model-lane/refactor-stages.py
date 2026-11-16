from pathlib import Path
import hashlib,json
p=Path('.g5/Verus_verification/consolidated/iter24/snapshot_dependencies.rs');s=p.read_text();o=p.parent/'model-lane';(o/'attempt-2.rs').write_text(s);(o/'attempt-2.json').write_text(json.dumps({'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'exit':130,'credited':False,'observed_process_elapsed_before_stop':'1:50','exact_wall_seconds_recorded':False,'reason':'Manual interruption of prolonged full named-World computation; empty log, no result. No limit increase.'},indent=2)+'\n')
s=s.replace('pub struct Outcome {pub world:World,pub graph:Graph,pub requested:Text,pub parent_requested:Text}','pub struct Outcome {pub world:World,pub graph:Graph,pub requested:Text,pub parent_requested:Text}')
# Cutpoints are not assumptions. Below, transition proofs establish them from
# the same active graph/consumer producer definitions.
a=s.index('// Definition-only witnesses;')
pre=s[:a];proof=s[a:]
pre+='''// Finite cutpoints for factoring proof cost; established by transition proofs.
pub open spec fn installed_cutpoint(c:Case)->World {let d=package(c);World{installed:Some(Installed{name:d.name,version:d.version,repository:d.repository,root:d.root,code:d.code}),..empty(c)}}
pub open spec fn prepared(c:Case)->World {edit(installed_cutpoint(c))}
pub open spec fn refreshed(c:Case)->World {let w=prepared(c);successful_load(w,w.source)}
pub open spec fn no_force()->Graph {build(prepared(Case::Snapshot),snap(),seq![],defaults())}
pub open spec fn force()->Graph {build(prepared(Case::Snapshot),snap(),seq![],params(command()))}
pub open spec fn force_permit()->Graph {build(prepared(Case::Snapshot),snap(),seq![],params(Command{permit_downgrade:true,..command()}))}
pub open spec fn release_force()->Graph {build(prepared(Case::Release),one(),seq![],params(command()))}
pub open spec fn payload_graph(g:Graph)->Option<Node> {g.node}
'''
proof=proof[:proof.index('pub proof fn lifecycle_neighbor()')]
proof=proof.replace('initial(Case::Snapshot)','prepared(Case::Snapshot)')
proof+='''pub proof fn literal_membership()
 ensures satisfies(one(),exact(one())),satisfies(one(),lower(one())),
 satisfies(snap(),exact(snap())),satisfies(snap(),lower(snap()))
{assert(satisfies(one(),exact(one()))) by(compute);assert(satisfies(one(),lower(one()))) by(compute);
 assert(satisfies(snap(),exact(snap()))) by(compute);assert(satisfies(snap(),lower(snap()))) by(compute);}
pub proof fn decision_neighbor()
 ensures graph_ok(no_force()),no_force().node.unwrap().repository.len()==0,
 !graph_ok(force()),force().error==Some(Error::NoOverlap),force().node.is_none(),
 graph_ok(force_permit()),force_permit().node.unwrap().repository==alpha(),
 graph_ok(release_force()),release_force().node.unwrap().repository.len()==0
{assert(graph_ok(no_force())) by(compute);assert(no_force().node.unwrap().repository.len()==0) by(compute);
 assert(!graph_ok(force())) by(compute);assert(force().error==Some(Error::NoOverlap)) by(compute);assert(force().node.is_none()) by(compute);
 assert(graph_ok(force_permit())) by(compute);assert(force_permit().node.unwrap().repository==alpha()) by(compute);
 assert(graph_ok(release_force())) by(compute);assert(release_force().node.unwrap().repository.len()==0) by(compute);}
pub proof fn load_neighbor()
 ensures dep_payload(prepared(Case::Snapshot))==Some(1int),parent_payload(prepared(Case::Snapshot))==Some(1int),
 dep_payload(consume(prepared(Case::Snapshot),seq![Node{name:name(Case::Snapshot),repository:seq![],version:snap()}],0))==Some(1int),
 dep_payload(consume(prepared(Case::Snapshot),seq![Node{name:name(Case::Snapshot),repository:alpha(),version:snap()}],0))==Some(2int),
 parent_payload(refreshed(Case::Snapshot))==Some(2int),installed_repository(refreshed(Case::Snapshot))==alpha(),installed_root(refreshed(Case::Snapshot))==root(Case::Snapshot)
{reveal_with_fuel(consume,2);}
pub proof fn request_neighbor(w:World,c:Command)
 ensures reinstall(w,c).requested==w.parent.dependency_version,
 reinstall(w,c).parent_requested==w.parent.version,
 params(command()).force_snapshots,!params(Command{snapshots:Some(false),..command()}).force_snapshots,
 params(Command{snapshots:Some(true),..command()}).force_snapshots,
 !params(command()).permit_downgrade,params(Command{permit_downgrade:true,..command()}).permit_downgrade
{}
pub proof fn initial_transition(c:Case)
 ensures initial(c)==installed_cutpoint(c),changed(c)==prepared(c)
{literal_membership();match c{Case::Snapshot=>{},Case::Release=>{}}reveal_with_fuel(consume,2);}
pub proof fn lifecycle_staged()
 ensures initial(Case::Snapshot)==installed_cutpoint(Case::Snapshot),initial(Case::Release)==installed_cutpoint(Case::Release),
 changed(Case::Snapshot)==prepared(Case::Snapshot),changed(Case::Release)==prepared(Case::Release),
 ok(shallow()),shallow().world==prepared(Case::Snapshot),
 !ok(default_snapshot()),default_snapshot().graph.error==Some(Error::NoOverlap),default_snapshot().world==prepared(Case::Snapshot),
 ok(permitted()),permitted().world==refreshed(Case::Snapshot),
 ok(default_release()),default_release().world==prepared(Case::Release)
{initial_transition(Case::Snapshot);initial_transition(Case::Release);literal_membership();classification_neighbor();intersection_neighbor();decision_neighbor();reveal_with_fuel(consume,2);}
}
fn main(){}
'''
p.write_text(pre+proof)
