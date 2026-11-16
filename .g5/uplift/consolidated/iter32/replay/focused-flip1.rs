#[path="../../../../Verus_verification/consolidated/iter32/sync.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((present(s2().files, 1)) == 0) by (compute);assert((present(s2().files, 1)) == 1); // s2.exists.SyncC.B
}
}
fn main() {}
