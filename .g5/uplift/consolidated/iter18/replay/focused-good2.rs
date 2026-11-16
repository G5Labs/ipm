#[path="../../../../Verus_verification/consolidated/iter18/reloading.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(missing_after_b().ok)) == 0); // missing-after-b
}
}
fn main() {}
