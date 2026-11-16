#[path="../../../../Verus_verification/consolidated/iter59/mappings.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b((e2()).module)) == 1) by (compute);assert((b((e2()).module)) == 1); // e2.e.module
}
}
fn main() {}
