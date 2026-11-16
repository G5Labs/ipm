#[path="../../../../Verus_verification/consolidated/iter18/reloading.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(reload(initial(),request(alpha())).ok)) == 0); // a
}
}
fn main() {}
