use vstd::prelude::*;
#[path="../snapshot_dependencies.rs"] pub mod model;
use model::*;
verus!{pub proof fn observed_group(){
lifecycle_staged();literal_fields();literal_membership();classification_neighbor();decision_neighbor();flat_neighbor();
// shallow-snap

assert((b(ok(shallow())))==1);
// default-snap

assert((b(ok(default_snapshot())))==0);
// permit-snap

assert((b(ok(permitted())))==1);
// default-release

assert((b(ok(default_release())))==1);
// semver-build
assert((b(inherited_semver::parse(snap()).build==seq!['s','n','a','p','s','h','o','t']))==1) by(compute);
assert((b(inherited_semver::parse(snap()).build==seq!['s','n','a','p','s','h','o','t']))==1);
// semver-snapshot
assert((b(is_snapshot(snap())))==1) by(compute);
assert((b(is_snapshot(snap())))==1);
// semver-exact-satisfied
assert((b(satisfies(snap(),exact(snap()))))==1) by(compute);
assert((b(satisfies(snap(),exact(snap()))))==1);
// semver-lower-satisfied
assert((b(satisfies(snap(),lower(snap()))))==1) by(compute);
assert((b(satisfies(snap(),lower(snap()))))==1);
// semver-and-rejected
assert((b(inherited_semver::expr_and(exact(snap()),lower(snap()),true,false) is NoOverlap))==1) by(compute);
assert((b(inherited_semver::expr_and(exact(snap()),lower(snap()),true,false) is NoOverlap))==1);
// initial-snap-dep-present
assert((b((installed_cutpoint(Case::Snapshot)).installed.is_some() && installed_name(installed_cutpoint(Case::Snapshot))==name(Case::Snapshot)))==1) by(compute);
assert((b((initial(Case::Snapshot)).installed.is_some() && installed_name(initial(Case::Snapshot))==name(Case::Snapshot)))==1);
// initial-snap-dep-version
assert((b(installed_version(installed_cutpoint(Case::Snapshot))==required(Case::Snapshot)))==1) by(compute);
assert((b(installed_version(initial(Case::Snapshot))==required(Case::Snapshot)))==1);
// initial-snap-dep-snapshot
assert((b(is_snapshot(installed_version(installed_cutpoint(Case::Snapshot)))))==1) by(compute);
assert((b(is_snapshot(installed_version(initial(Case::Snapshot)))))==1);
// initial-snap-dep-build
assert((b(inherited_semver::parse(installed_version(installed_cutpoint(Case::Snapshot))).build==inherited_semver::parse(required(Case::Snapshot)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(initial(Case::Snapshot))).build==inherited_semver::parse(required(Case::Snapshot)).build))==1);
// initial-snap-dep-supplier
assert((b(installed_repository(installed_cutpoint(Case::Snapshot))==alpha()))==1) by(compute);
assert((b(installed_repository(initial(Case::Snapshot))==alpha()))==1);
// initial-snap-dep-root
assert((b(installed_root(installed_cutpoint(Case::Snapshot))==root(Case::Snapshot)))==1) by(compute);
assert((b(installed_root(initial(Case::Snapshot))==root(Case::Snapshot)))==1);
// initial-snap-dep-code
assert((b((installed_cutpoint(Case::Snapshot)).installed.is_some() && (installed_cutpoint(Case::Snapshot)).installed.unwrap().code.class==class(Case::Snapshot)))==1) by(compute);
assert((b((initial(Case::Snapshot)).installed.is_some() && (initial(Case::Snapshot)).installed.unwrap().code.class==class(Case::Snapshot)))==1);
// initial-snap-dep-payload
assert((dep_payload(installed_cutpoint(Case::Snapshot)).unwrap())==1) by(compute);
assert((dep_payload(initial(Case::Snapshot)).unwrap())==1);
// initial-snap-parent-dep-version
assert((b((installed_cutpoint(Case::Snapshot)).parent.dependency_version==required(Case::Snapshot)))==1) by(compute);
assert((b((initial(Case::Snapshot)).parent.dependency_version==required(Case::Snapshot)))==1);
// initial-snap-parent-payload
assert((parent_payload(installed_cutpoint(Case::Snapshot)).unwrap())==1) by(compute);
assert((parent_payload(initial(Case::Snapshot)).unwrap())==1);
// initial-release-dep-present
assert((b((installed_cutpoint(Case::Release)).installed.is_some() && installed_name(installed_cutpoint(Case::Release))==name(Case::Release)))==1) by(compute);
assert((b((initial(Case::Release)).installed.is_some() && installed_name(initial(Case::Release))==name(Case::Release)))==1);
// initial-release-dep-version
assert((b(installed_version(installed_cutpoint(Case::Release))==required(Case::Release)))==1) by(compute);
assert((b(installed_version(initial(Case::Release))==required(Case::Release)))==1);
// initial-release-dep-snapshot
assert((b(is_snapshot(installed_version(installed_cutpoint(Case::Release)))))==0) by(compute);
assert((b(is_snapshot(installed_version(initial(Case::Release)))))==0);
// initial-release-dep-build
assert((b(inherited_semver::parse(installed_version(installed_cutpoint(Case::Release))).build==inherited_semver::parse(required(Case::Release)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(initial(Case::Release))).build==inherited_semver::parse(required(Case::Release)).build))==1);
// initial-release-dep-supplier
assert((b(installed_repository(installed_cutpoint(Case::Release))==alpha()))==1) by(compute);
assert((b(installed_repository(initial(Case::Release))==alpha()))==1);
// initial-release-dep-root
assert((b(installed_root(installed_cutpoint(Case::Release))==root(Case::Release)))==1) by(compute);
assert((b(installed_root(initial(Case::Release))==root(Case::Release)))==1);
// initial-release-dep-code
assert((b((installed_cutpoint(Case::Release)).installed.is_some() && (installed_cutpoint(Case::Release)).installed.unwrap().code.class==class(Case::Release)))==1) by(compute);
assert((b((initial(Case::Release)).installed.is_some() && (initial(Case::Release)).installed.unwrap().code.class==class(Case::Release)))==1);
// initial-release-dep-payload
assert((dep_payload(installed_cutpoint(Case::Release)).unwrap())==1) by(compute);
assert((dep_payload(initial(Case::Release)).unwrap())==1);
// initial-release-parent-dep-version
assert((b((installed_cutpoint(Case::Release)).parent.dependency_version==required(Case::Release)))==1) by(compute);
assert((b((initial(Case::Release)).parent.dependency_version==required(Case::Release)))==1);
// initial-release-parent-payload
assert((parent_payload(installed_cutpoint(Case::Release)).unwrap())==1) by(compute);
assert((parent_payload(initial(Case::Release)).unwrap())==1);
// changed-snap-dep-present
assert((b((prepared(Case::Snapshot)).installed.is_some() && installed_name(prepared(Case::Snapshot))==name(Case::Snapshot)))==1) by(compute);
assert((b((changed(Case::Snapshot)).installed.is_some() && installed_name(changed(Case::Snapshot))==name(Case::Snapshot)))==1);
// changed-snap-dep-version
assert((b(installed_version(prepared(Case::Snapshot))==required(Case::Snapshot)))==1) by(compute);
assert((b(installed_version(changed(Case::Snapshot))==required(Case::Snapshot)))==1);
// changed-snap-dep-snapshot
assert((b(is_snapshot(installed_version(prepared(Case::Snapshot)))))==1) by(compute);
assert((b(is_snapshot(installed_version(changed(Case::Snapshot)))))==1);
// changed-snap-dep-build
assert((b(inherited_semver::parse(installed_version(prepared(Case::Snapshot))).build==inherited_semver::parse(required(Case::Snapshot)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(changed(Case::Snapshot))).build==inherited_semver::parse(required(Case::Snapshot)).build))==1);
// changed-snap-dep-supplier
assert((b(installed_repository(prepared(Case::Snapshot))==alpha()))==1) by(compute);
assert((b(installed_repository(changed(Case::Snapshot))==alpha()))==1);
// changed-snap-dep-root
assert((b(installed_root(prepared(Case::Snapshot))==root(Case::Snapshot)))==1) by(compute);
assert((b(installed_root(changed(Case::Snapshot))==root(Case::Snapshot)))==1);
// changed-snap-dep-code
assert((b((prepared(Case::Snapshot)).installed.is_some() && (prepared(Case::Snapshot)).installed.unwrap().code.class==class(Case::Snapshot)))==1) by(compute);
assert((b((changed(Case::Snapshot)).installed.is_some() && (changed(Case::Snapshot)).installed.unwrap().code.class==class(Case::Snapshot)))==1);
// changed-snap-dep-payload
assert((dep_payload(prepared(Case::Snapshot)).unwrap())==1) by(compute);
assert((dep_payload(changed(Case::Snapshot)).unwrap())==1);
// changed-snap-parent-dep-version
assert((b((prepared(Case::Snapshot)).parent.dependency_version==required(Case::Snapshot)))==1) by(compute);
assert((b((changed(Case::Snapshot)).parent.dependency_version==required(Case::Snapshot)))==1);
// changed-snap-parent-payload
assert((parent_payload(prepared(Case::Snapshot)).unwrap())==1) by(compute);
assert((parent_payload(changed(Case::Snapshot)).unwrap())==1);
// changed-release-dep-present
assert((b((prepared(Case::Release)).installed.is_some() && installed_name(prepared(Case::Release))==name(Case::Release)))==1) by(compute);
assert((b((changed(Case::Release)).installed.is_some() && installed_name(changed(Case::Release))==name(Case::Release)))==1);
// changed-release-dep-version
assert((b(installed_version(prepared(Case::Release))==required(Case::Release)))==1) by(compute);
assert((b(installed_version(changed(Case::Release))==required(Case::Release)))==1);
// changed-release-dep-snapshot
assert((b(is_snapshot(installed_version(prepared(Case::Release)))))==0) by(compute);
assert((b(is_snapshot(installed_version(changed(Case::Release)))))==0);
// changed-release-dep-build
assert((b(inherited_semver::parse(installed_version(prepared(Case::Release))).build==inherited_semver::parse(required(Case::Release)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(changed(Case::Release))).build==inherited_semver::parse(required(Case::Release)).build))==1);
// changed-release-dep-supplier
assert((b(installed_repository(prepared(Case::Release))==alpha()))==1) by(compute);
assert((b(installed_repository(changed(Case::Release))==alpha()))==1);
// changed-release-dep-root
assert((b(installed_root(prepared(Case::Release))==root(Case::Release)))==1) by(compute);
assert((b(installed_root(changed(Case::Release))==root(Case::Release)))==1);
// changed-release-dep-code
assert((b((prepared(Case::Release)).installed.is_some() && (prepared(Case::Release)).installed.unwrap().code.class==class(Case::Release)))==1) by(compute);
assert((b((changed(Case::Release)).installed.is_some() && (changed(Case::Release)).installed.unwrap().code.class==class(Case::Release)))==1);
// changed-release-dep-payload
assert((dep_payload(prepared(Case::Release)).unwrap())==1) by(compute);
assert((dep_payload(changed(Case::Release)).unwrap())==1);
// changed-release-parent-dep-version
assert((b((prepared(Case::Release)).parent.dependency_version==required(Case::Release)))==1) by(compute);
assert((b((changed(Case::Release)).parent.dependency_version==required(Case::Release)))==1);
// changed-release-parent-payload
assert((parent_payload(prepared(Case::Release)).unwrap())==1) by(compute);
assert((parent_payload(changed(Case::Release)).unwrap())==1);
// shallow-snap-dep-present
assert((b((prepared(Case::Snapshot)).installed.is_some() && installed_name(prepared(Case::Snapshot))==name(Case::Snapshot)))==1) by(compute);
assert((b((shallow().world).installed.is_some() && installed_name(shallow().world)==name(Case::Snapshot)))==1);
// shallow-snap-dep-version
assert((b(installed_version(prepared(Case::Snapshot))==required(Case::Snapshot)))==1) by(compute);
assert((b(installed_version(shallow().world)==required(Case::Snapshot)))==1);
// shallow-snap-dep-snapshot
assert((b(is_snapshot(installed_version(prepared(Case::Snapshot)))))==1) by(compute);
assert((b(is_snapshot(installed_version(shallow().world))))==1);
// shallow-snap-dep-build
assert((b(inherited_semver::parse(installed_version(prepared(Case::Snapshot))).build==inherited_semver::parse(required(Case::Snapshot)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(shallow().world)).build==inherited_semver::parse(required(Case::Snapshot)).build))==1);
// shallow-snap-dep-supplier
assert((b(installed_repository(prepared(Case::Snapshot))==alpha()))==1) by(compute);
assert((b(installed_repository(shallow().world)==alpha()))==1);
// shallow-snap-dep-root
assert((b(installed_root(prepared(Case::Snapshot))==root(Case::Snapshot)))==1) by(compute);
assert((b(installed_root(shallow().world)==root(Case::Snapshot)))==1);
// shallow-snap-dep-code
assert((b((prepared(Case::Snapshot)).installed.is_some() && (prepared(Case::Snapshot)).installed.unwrap().code.class==class(Case::Snapshot)))==1) by(compute);
assert((b((shallow().world).installed.is_some() && (shallow().world).installed.unwrap().code.class==class(Case::Snapshot)))==1);
// shallow-snap-dep-payload
assert((dep_payload(prepared(Case::Snapshot)).unwrap())==1) by(compute);
assert((dep_payload(shallow().world).unwrap())==1);
// shallow-snap-parent-dep-version
assert((b((prepared(Case::Snapshot)).parent.dependency_version==required(Case::Snapshot)))==1) by(compute);
assert((b((shallow().world).parent.dependency_version==required(Case::Snapshot)))==1);
// shallow-snap-parent-payload
assert((parent_payload(prepared(Case::Snapshot)).unwrap())==1) by(compute);
assert((parent_payload(shallow().world).unwrap())==1);
// default-snap-dep-present
assert((b((prepared(Case::Snapshot)).installed.is_some() && installed_name(prepared(Case::Snapshot))==name(Case::Snapshot)))==1) by(compute);
assert((b((default_snapshot().world).installed.is_some() && installed_name(default_snapshot().world)==name(Case::Snapshot)))==1);
// default-snap-dep-version
assert((b(installed_version(prepared(Case::Snapshot))==required(Case::Snapshot)))==1) by(compute);
assert((b(installed_version(default_snapshot().world)==required(Case::Snapshot)))==1);
// default-snap-dep-snapshot
assert((b(is_snapshot(installed_version(prepared(Case::Snapshot)))))==1) by(compute);
assert((b(is_snapshot(installed_version(default_snapshot().world))))==1);
// default-snap-dep-build
assert((b(inherited_semver::parse(installed_version(prepared(Case::Snapshot))).build==inherited_semver::parse(required(Case::Snapshot)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(default_snapshot().world)).build==inherited_semver::parse(required(Case::Snapshot)).build))==1);
// default-snap-dep-supplier
assert((b(installed_repository(prepared(Case::Snapshot))==alpha()))==1) by(compute);
assert((b(installed_repository(default_snapshot().world)==alpha()))==1);
// default-snap-dep-root
assert((b(installed_root(prepared(Case::Snapshot))==root(Case::Snapshot)))==1) by(compute);
assert((b(installed_root(default_snapshot().world)==root(Case::Snapshot)))==1);
// default-snap-dep-code
assert((b((prepared(Case::Snapshot)).installed.is_some() && (prepared(Case::Snapshot)).installed.unwrap().code.class==class(Case::Snapshot)))==1) by(compute);
assert((b((default_snapshot().world).installed.is_some() && (default_snapshot().world).installed.unwrap().code.class==class(Case::Snapshot)))==1);
// default-snap-dep-payload
assert((dep_payload(prepared(Case::Snapshot)).unwrap())==1) by(compute);
assert((dep_payload(default_snapshot().world).unwrap())==1);
// default-snap-parent-dep-version
assert((b((prepared(Case::Snapshot)).parent.dependency_version==required(Case::Snapshot)))==1) by(compute);
assert((b((default_snapshot().world).parent.dependency_version==required(Case::Snapshot)))==1);
// default-snap-parent-payload
assert((parent_payload(prepared(Case::Snapshot)).unwrap())==1) by(compute);
assert((parent_payload(default_snapshot().world).unwrap())==1);
// permit-snap-dep-present
assert((b((refreshed(Case::Snapshot)).installed.is_some() && installed_name(refreshed(Case::Snapshot))==name(Case::Snapshot)))==1) by(compute);
assert((b((permitted().world).installed.is_some() && installed_name(permitted().world)==name(Case::Snapshot)))==1);
// permit-snap-dep-version
assert((b(installed_version(refreshed(Case::Snapshot))==required(Case::Snapshot)))==1) by(compute);
assert((b(installed_version(permitted().world)==required(Case::Snapshot)))==1);
// permit-snap-dep-snapshot
assert((b(is_snapshot(installed_version(refreshed(Case::Snapshot)))))==1) by(compute);
assert((b(is_snapshot(installed_version(permitted().world))))==1);
// permit-snap-dep-build
assert((b(inherited_semver::parse(installed_version(refreshed(Case::Snapshot))).build==inherited_semver::parse(required(Case::Snapshot)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(permitted().world)).build==inherited_semver::parse(required(Case::Snapshot)).build))==1);
// permit-snap-dep-supplier
assert((b(installed_repository(refreshed(Case::Snapshot))==alpha()))==1) by(compute);
assert((b(installed_repository(permitted().world)==alpha()))==1);
// permit-snap-dep-root
assert((b(installed_root(refreshed(Case::Snapshot))==root(Case::Snapshot)))==1) by(compute);
assert((b(installed_root(permitted().world)==root(Case::Snapshot)))==1);
// permit-snap-dep-code
assert((b((refreshed(Case::Snapshot)).installed.is_some() && (refreshed(Case::Snapshot)).installed.unwrap().code.class==class(Case::Snapshot)))==1) by(compute);
assert((b((permitted().world).installed.is_some() && (permitted().world).installed.unwrap().code.class==class(Case::Snapshot)))==1);
// permit-snap-dep-payload
assert((dep_payload(refreshed(Case::Snapshot)).unwrap())==2) by(compute);
assert((dep_payload(permitted().world).unwrap())==2);
// permit-snap-parent-dep-version
assert((b((refreshed(Case::Snapshot)).parent.dependency_version==required(Case::Snapshot)))==1) by(compute);
assert((b((permitted().world).parent.dependency_version==required(Case::Snapshot)))==1);
// permit-snap-parent-payload
assert((parent_payload(refreshed(Case::Snapshot)).unwrap())==2) by(compute);
assert((parent_payload(permitted().world).unwrap())==2);
// default-release-dep-present
assert((b((prepared(Case::Release)).installed.is_some() && installed_name(prepared(Case::Release))==name(Case::Release)))==1) by(compute);
assert((b((default_release().world).installed.is_some() && installed_name(default_release().world)==name(Case::Release)))==1);
// default-release-dep-version
assert((b(installed_version(prepared(Case::Release))==required(Case::Release)))==1) by(compute);
assert((b(installed_version(default_release().world)==required(Case::Release)))==1);
// default-release-dep-snapshot
assert((b(is_snapshot(installed_version(prepared(Case::Release)))))==0) by(compute);
assert((b(is_snapshot(installed_version(default_release().world))))==0);
// default-release-dep-build
assert((b(inherited_semver::parse(installed_version(prepared(Case::Release))).build==inherited_semver::parse(required(Case::Release)).build))==1) by(compute);
assert((b(inherited_semver::parse(installed_version(default_release().world)).build==inherited_semver::parse(required(Case::Release)).build))==1);
// default-release-dep-supplier
assert((b(installed_repository(prepared(Case::Release))==alpha()))==1) by(compute);
assert((b(installed_repository(default_release().world)==alpha()))==1);
// default-release-dep-root
assert((b(installed_root(prepared(Case::Release))==root(Case::Release)))==1) by(compute);
assert((b(installed_root(default_release().world)==root(Case::Release)))==1);
// default-release-dep-code
assert((b((prepared(Case::Release)).installed.is_some() && (prepared(Case::Release)).installed.unwrap().code.class==class(Case::Release)))==1) by(compute);
assert((b((default_release().world).installed.is_some() && (default_release().world).installed.unwrap().code.class==class(Case::Release)))==1);
// default-release-dep-payload
assert((dep_payload(prepared(Case::Release)).unwrap())==1) by(compute);
assert((dep_payload(default_release().world).unwrap())==1);
// default-release-parent-dep-version
assert((b((prepared(Case::Release)).parent.dependency_version==required(Case::Release)))==1) by(compute);
assert((b((default_release().world).parent.dependency_version==required(Case::Release)))==1);
// default-release-parent-payload
assert((parent_payload(prepared(Case::Release)).unwrap())==1) by(compute);
assert((parent_payload(default_release().world).unwrap())==1);
// physical-snap-two
assert((b(prepared(Case::Snapshot).source.code.who==2))==1) by(compute);
assert((b(changed(Case::Snapshot).source.code.who==2))==1);
// physical-release-two
assert((b(prepared(Case::Release).source.code.who==2))==1) by(compute);
assert((b(changed(Case::Release).source.code.who==2))==1);
// snap-no-force-status

assert((b(graph_ok(no_force())))==1);
// snap-no-force-entry
assert((b((retained_graph(Case::Snapshot)).node.is_some()))==1) by(compute);
assert((b((no_force()).node.is_some()))==1);
// snap-no-force-reuse
assert((b((retained_graph(Case::Snapshot)).node.unwrap().repository.len()==0))==1) by(compute);
assert((b((no_force()).node.unwrap().repository.len()==0))==1);
// snap-no-force-alpha
assert((b((retained_graph(Case::Snapshot)).node.unwrap().repository==alpha()))==0) by(compute);
assert((b((no_force()).node.unwrap().repository==alpha()))==0);
// snap-no-force-version
assert((b((retained_graph(Case::Snapshot)).node.unwrap().version==required(Case::Snapshot)))==1) by(compute);
assert((b((no_force()).node.unwrap().version==required(Case::Snapshot)))==1);
// snap-no-force-flat-reuse
assert((b(flat_selected(retained_graph(Case::Snapshot))[0].repository.len()==0))==1) by(compute);
assert((b(flat_selected(no_force())[0].repository.len()==0))==1);
// snap-no-force-flat-alpha
assert((b(flat_selected(retained_graph(Case::Snapshot))[0].repository==alpha()))==0) by(compute);
assert((b(flat_selected(no_force())[0].repository==alpha()))==0);
// snap-no-force-flat-version
assert((b(flat_selected(retained_graph(Case::Snapshot))[0].version==required(Case::Snapshot)))==1) by(compute);
assert((b(flat_selected(no_force())[0].version==required(Case::Snapshot)))==1);
// snap-no-force-flat-count
assert((flat_selected(retained_graph(Case::Snapshot)).len() as int)==1) by(compute);
assert((flat_selected(no_force()).len() as int)==1);
// snap-force-status

assert((b(graph_ok(force())))==0);
// snap-force-entry
assert((b((Graph{node:None,error:Some(Error::NoOverlap)}).node.is_some()))==0) by(compute);
assert((b((force()).node.is_some()))==0);
// snap-force-permit-status

assert((b(graph_ok(force_permit())))==1);
// snap-force-permit-entry
assert((b((loaded_graph(Case::Snapshot)).node.is_some()))==1) by(compute);
assert((b((force_permit()).node.is_some()))==1);
// snap-force-permit-reuse
assert((b((loaded_graph(Case::Snapshot)).node.unwrap().repository.len()==0))==0) by(compute);
assert((b((force_permit()).node.unwrap().repository.len()==0))==0);
// snap-force-permit-alpha
assert((b((loaded_graph(Case::Snapshot)).node.unwrap().repository==alpha()))==1) by(compute);
assert((b((force_permit()).node.unwrap().repository==alpha()))==1);
// snap-force-permit-version
assert((b((loaded_graph(Case::Snapshot)).node.unwrap().version==required(Case::Snapshot)))==1) by(compute);
assert((b((force_permit()).node.unwrap().version==required(Case::Snapshot)))==1);
// snap-force-permit-flat-reuse
assert((b(flat_selected(loaded_graph(Case::Snapshot))[0].repository.len()==0))==0) by(compute);
assert((b(flat_selected(force_permit())[0].repository.len()==0))==0);
// snap-force-permit-flat-alpha
assert((b(flat_selected(loaded_graph(Case::Snapshot))[0].repository==alpha()))==1) by(compute);
assert((b(flat_selected(force_permit())[0].repository==alpha()))==1);
// snap-force-permit-flat-version
assert((b(flat_selected(loaded_graph(Case::Snapshot))[0].version==required(Case::Snapshot)))==1) by(compute);
assert((b(flat_selected(force_permit())[0].version==required(Case::Snapshot)))==1);
// snap-force-permit-flat-count
assert((flat_selected(loaded_graph(Case::Snapshot)).len() as int)==1) by(compute);
assert((flat_selected(force_permit()).len() as int)==1);
// release-force-status

assert((b(graph_ok(release_force())))==1);
// release-force-entry
assert((b((retained_graph(Case::Release)).node.is_some()))==1) by(compute);
assert((b((release_force()).node.is_some()))==1);
// release-force-reuse
assert((b((retained_graph(Case::Release)).node.unwrap().repository.len()==0))==1) by(compute);
assert((b((release_force()).node.unwrap().repository.len()==0))==1);
// release-force-alpha
assert((b((retained_graph(Case::Release)).node.unwrap().repository==alpha()))==0) by(compute);
assert((b((release_force()).node.unwrap().repository==alpha()))==0);
// release-force-version
assert((b((retained_graph(Case::Release)).node.unwrap().version==required(Case::Release)))==1) by(compute);
assert((b((release_force()).node.unwrap().version==required(Case::Release)))==1);
// release-force-flat-reuse
assert((b(flat_selected(retained_graph(Case::Release))[0].repository.len()==0))==1) by(compute);
assert((b(flat_selected(release_force())[0].repository.len()==0))==1);
// release-force-flat-alpha
assert((b(flat_selected(retained_graph(Case::Release))[0].repository==alpha()))==0) by(compute);
assert((b(flat_selected(release_force())[0].repository==alpha()))==0);
// release-force-flat-version
assert((b(flat_selected(retained_graph(Case::Release))[0].version==required(Case::Release)))==1) by(compute);
assert((b(flat_selected(release_force())[0].version==required(Case::Release)))==1);
// release-force-flat-count
assert((flat_selected(retained_graph(Case::Release)).len() as int)==1) by(compute);
assert((flat_selected(release_force()).len() as int)==1);
}}
fn main(){}
