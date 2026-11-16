#[path="../../../../Verus_verification/consolidated/iter49/importcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
reveal_with_fuel(load_fails, 3); assert((b(added(i7(), i7items(), I(false, true, false)))) == 1);assert((b(added(i7(), i7items(), I(false, true, false)))) == 0); // i7.add.new
}
}
fn main() {}
