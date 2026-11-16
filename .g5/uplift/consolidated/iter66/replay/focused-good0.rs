#[path="../../../../Verus_verification/consolidated/iter66/docphase.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((pos(document(full(), 4, true).marks, 3)) == 0);assert((pos(document(full(), 4, true).marks, 3)) == 0); // c4.hook.e66
}
}
fn main() {}
