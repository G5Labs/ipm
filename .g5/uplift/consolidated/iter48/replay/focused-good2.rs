#[path="../../../../Verus_verification/consolidated/iter48/cpfmerge.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
reveal_with_fuel(merged_by, 6); assert(((if merged_by(A(), load()) { 1int } else { 0int })) == 1);assert(((if merged_by(A(), load()) { 1int } else { 0int })) == 1); // l1.mapA
}
}
fn main() {}
