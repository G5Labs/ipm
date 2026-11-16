#[path="../../../../Verus_verification/consolidated/iter57/cspapp.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }).deploy_files) == 2);assert((load(W { source_files: 2, ..load(start(), Res { has_deploy: true }) }, Res { has_deploy: true }).deploy_files) == 3); // a2.deployA
}
}
fn main() {}
