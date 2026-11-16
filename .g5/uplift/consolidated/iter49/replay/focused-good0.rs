#[path="../../../../Verus_verification/consolidated/iter49/importcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
reveal_with_fuel(load_fails, 3); assert((b(defined(i3(), I(true, true, false)))) == 0);assert((b(defined(i3(), I(true, true, false)))) == 0); // i3.defined.I49N.Sub
}
}
fn main() {}
