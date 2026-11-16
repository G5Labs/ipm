#[path="../../../../Verus_verification/consolidated/iter48/cpfmerge.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

reveal_with_fuel(merged_by, 6); assert(((if ok(BAD(), seq![49int]) { 1int } else { 0int })) == 0);assert(((if ok(BAD(), seq![49int]) { 1int } else { 0int })) == 0); // x1.ok
reveal_with_fuel(merged_by, 6); assert(((if merged_by(B(), load()) || merged_by(B(), seq![COMPILE()]) { 1int } else { 0int })) == 0);assert(((if merged_by(B(), load()) || merged_by(B(), seq![COMPILE()]) { 1int } else { 0int })) == 0); // c1.mapB
reveal_with_fuel(merged_by, 6); assert(((if merged_by(A(), load()) { 1int } else { 0int })) == 1);assert(((if merged_by(A(), load()) { 1int } else { 0int })) == 1); // l1.mapA
reveal_with_fuel(merged_by, 6); assert(((if merged_by(B(), load()) { 1int } else { 0int })) == 0);assert(((if merged_by(B(), load()) { 1int } else { 0int })) == 0); // l1.mapB
reveal_with_fuel(merged_by, 6); assert(((if merged_by(B(), seq![48int]) { 1int } else { 0int })) == 1);assert(((if merged_by(B(), seq![48int]) { 1int } else { 0int })) == 1); // p1.mapB
reveal_with_fuel(merged_by, 6); assert(((if ok(A(), load()) && ok(B(), load()) && ok(BAD(), load()) { 1int } else { 0int })) == 1);assert(((if ok(A(), load()) && ok(B(), load()) && ok(BAD(), load()) { 1int } else { 0int })) == 1); // l1.ok
}
}
fn main() {}
