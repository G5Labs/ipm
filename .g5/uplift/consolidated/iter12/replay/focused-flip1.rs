#[path="../../../../Verus_verification/consolidated/iter12/all.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,true).ok)) == 0); // map-preview
}
}
fn main() {}
