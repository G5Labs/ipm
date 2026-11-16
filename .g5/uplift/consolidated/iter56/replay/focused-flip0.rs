#[path="../../../../Verus_verification/consolidated/iter56/locmsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(r1().1)) == 0);assert((b(r1().1)) == 1); // r1.ok
}
}
fn main() {}
