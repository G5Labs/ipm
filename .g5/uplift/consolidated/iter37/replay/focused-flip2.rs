#[path="../../../../Verus_verification/consolidated/iter37/copy.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((value(o4(), 6)) == 6) by (compute);assert((value(o4(), 6)) == 7); // o4.value.CopyD.Owned
}
}
fn main() {}
