#[path="../../../../Verus_verification/consolidated/iter37/copy.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((defined(u2(), 3)) == 0) by (compute);assert((defined(u2(), 3)) == 1); // u2.defined.CopyB.D1
}
}
fn main() {}
