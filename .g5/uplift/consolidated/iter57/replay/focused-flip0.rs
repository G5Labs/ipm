#[path="../../../../Verus_verification/consolidated/iter57/cspapp.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((uninstall(load(start(), Res { has_deploy: false }), Res { has_deploy: false }).source_files) == 0);assert((uninstall(load(start(), Res { has_deploy: false }), Res { has_deploy: false }).source_files) == 1); // b2.sourceB
}
}
fn main() {}
