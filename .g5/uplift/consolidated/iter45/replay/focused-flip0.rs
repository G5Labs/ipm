#[path="../../../../Verus_verification/consolidated/iter45/scan.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
reveal_with_fuel(any_unreadable_change, 2); assert(((if sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: false, disk: 4, base: Some(1), ns: Some(1) }]).ok { 1int } else { 0int })) == 0);assert(((if sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: false, disk: 4, base: Some(1), ns: Some(1) }]).ok { 1int } else { 0int })) == 1); // f4.ok
}
}
fn main() {}
