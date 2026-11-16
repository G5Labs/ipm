#[path="../../../../Verus_verification/consolidated/iter56/locmsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((l2().0.msgs[1]) == 0);assert((l2().0.msgs[1]) == 1); // l2.msg.B
}
}
fn main() {}
