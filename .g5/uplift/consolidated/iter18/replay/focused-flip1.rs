#[path="../../../../Verus_verification/consolidated/iter18/reloading.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(reload(after_a(),request(beta())).ok)) == 0); // b
}
}
fn main() {}
