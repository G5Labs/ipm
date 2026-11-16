#[path="../../../../Verus_verification/consolidated/iter55/nsinit.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(!zn(Ns::Known { ipm: false }, false).switched)) == 1);assert((b(!zn(Ns::Known { ipm: false }, false).switched)) == 1); // z1.after.user
}
}
fn main() {}
