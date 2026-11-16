#[path="../../../../Verus_verification/consolidated/iter59/mappings.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // b2.b.map
}
}
fn main() {}
