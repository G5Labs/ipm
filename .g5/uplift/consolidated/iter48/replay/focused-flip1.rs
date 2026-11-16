#[path="../../../../Verus_verification/consolidated/iter48/cpfmerge.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
reveal_with_fuel(merged_by, 6); assert(((if merged_by(B(), load()) || merged_by(B(), seq![COMPILE()]) { 1int } else { 0int })) == 0);assert(((if merged_by(B(), load()) || merged_by(B(), seq![COMPILE()]) { 1int } else { 0int })) == 1); // c1.mapB
}
}
fn main() {}
