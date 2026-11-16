#[path="../../../../Verus_verification/consolidated/iter19/duplicates.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(build(ab_a_first()).ok)) == 1); // repo-ab
}
}
fn main() {}
