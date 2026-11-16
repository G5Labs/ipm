#[path="../../../../Verus_verification/consolidated/iter57/cspapp.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().admin_desc)) == 0);assert((b(load(admin(), Res { has_deploy: true }).app.unwrap().admin_desc)) == 0); // c1.appc.descadmin
}
}
fn main() {}
