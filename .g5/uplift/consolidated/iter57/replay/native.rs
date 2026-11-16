#[path="../../../../Verus_verification/consolidated/iter57/cspapp.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((uninstall(load(start(), Res { has_deploy: false }), Res { has_deploy: false }).source_files) == 0);assert((uninstall(load(start(), Res { has_deploy: false }), Res { has_deploy: false }).source_files) == 0); // b2.sourceB
assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().admin_desc)) == 0);assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().admin_desc)) == 0); // c1.appc.descadmin
assert((load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }).deploy_files) == 2);assert((load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }).deploy_files) == 2); // a2.deployA
assert((b(uninstall(load(start(), Res { has_deploy: false }), Res { has_deploy: false }).source_files != 0)) == 0);assert((b(uninstall(load(start(), Res { has_deploy: false }), Res { has_deploy: false }).source_files != 0)) == 0); // b2.sourceBdir
assert((load(start(), Res { has_deploy: true }).deploy_files) == 1);assert((load(start(), Res { has_deploy: true }).deploy_files) == 1); // a1.deployA
assert((b(load(start(), Res { has_deploy: true }).app.is_some())) == 1);assert((b(load(start(), Res { has_deploy: true }).app.is_some())) == 1); // a1.appa
assert((b(load(start(), Res { has_deploy: true }).app.unwrap().path_is_deploy)) == 1);assert((b(load(start(), Res { has_deploy: true }).app.unwrap().path_is_deploy)) == 1); // a1.appa.pathdeploy
assert((b(load(start(), Res { has_deploy: true }).app.unwrap().unauth)) == 1);assert((b(load(start(), Res { has_deploy: true }).app.unwrap().unauth)) == 1); // a1.appa.unauth
assert((b(uninstall(load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }), Res { has_deploy: true }).app.is_some())) == 0);assert((b(uninstall(load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }), Res { has_deploy: true }).app.is_some())) == 0); // a3.appa
assert((uninstall(load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }), Res { has_deploy: true }).deploy_files) == 0);assert((uninstall(load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }), Res { has_deploy: true }).deploy_files) == 0); // a3.deployA
assert((b(!load(start(), Res { has_deploy: false }).app.unwrap().path_is_deploy)) == 1);assert((b(!load(start(), Res { has_deploy: false }).app.unwrap().path_is_deploy)) == 1); // b1.appb.pathsource
assert((load(start(), Res { has_deploy: false }).source_files) == 1);assert((load(start(), Res { has_deploy: false }).source_files) == 1); // b1.sourceB
assert((b(admin().app.unwrap().admin_desc)) == 1);assert((b(admin().app.unwrap().admin_desc)) == 1); // c0.appc.descadmin
assert((b(admin().app.unwrap().admin_timeout)) == 1);assert((b(admin().app.unwrap().admin_timeout)) == 1); // c0.appc.timeout1234
assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().admin_timeout)) == 0);assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().admin_timeout)) == 0); // c1.appc.timeout1234
assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().unauth)) == 1);assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().unauth)) == 1); // c1.appc.unauth
assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().path_is_deploy)) == 1);assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().path_is_deploy)) == 1); // c1.appc.pathdeploy
assert((b(uninstall(load(admin(), Res { has_deploy: true }), Res { has_deploy: true }).app.is_some())) == 0);assert((b(uninstall(load(admin(), Res { has_deploy: true }), Res { has_deploy: true }).app.is_some())) == 0); // c2.appc
}
}
fn main() {}
