#[path="../../../../Verus_verification/consolidated/iter32/sync.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((value(f1().files, 5)) == 1) by (compute);assert((value(f1().files, 5)) == 1); // f1.value.SyncF.A
}
}
fn main() {}
