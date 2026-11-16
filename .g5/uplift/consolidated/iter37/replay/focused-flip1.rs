#[path="../../../../Verus_verification/consolidated/iter37/copy.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((value(o2(), 3)) == 4) by (compute);assert((value(o2(), 3)) == 5); // o2.value.CopyB.D1
}
}
fn main() {}
