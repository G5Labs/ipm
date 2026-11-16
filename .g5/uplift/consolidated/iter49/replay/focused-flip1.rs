#[path="../../../../Verus_verification/consolidated/iter49/importcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
reveal_with_fuel(load_fails, 3); assert((b(ok(i5(), i5items()))) == 0);assert((b(ok(i5(), i5items()))) == 1); // i5.ok
}
}
fn main() {}
