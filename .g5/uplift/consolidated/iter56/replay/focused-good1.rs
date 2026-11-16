#[path="../../../../Verus_verification/consolidated/iter56/locmsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((u1().msgs[0]) == 2);assert((u1().msgs[0]) == 2); // u1.msg.A
}
}
fn main() {}
