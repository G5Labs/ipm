#[path="../../../../Verus_verification/consolidated/iter48/cpfmerge.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
reveal_with_fuel(merged_by, 6); assert(((if ok(BAD(), seq![49int]) { 1int } else { 0int })) == 0);assert(((if ok(BAD(), seq![49int]) { 1int } else { 0int })) == 1); // x1.ok
}
}
fn main() {}
