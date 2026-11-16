#[path="../../../../Verus_verification/consolidated/iter66/docphase.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((pos(document(full(), 2, false).marks, 1)) == 0);assert((pos(document(full(), 2, false).marks, 1)) == 0); // c2.hook.d66
}
}
fn main() {}
