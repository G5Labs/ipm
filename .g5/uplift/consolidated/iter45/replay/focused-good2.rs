#[path="../../../../Verus_verification/consolidated/iter45/scan.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
reveal_with_fuel(any_unreadable_change, 2); assert(((match sync(seq![F { depth: 1, upper_ext: true, skip_dir: false, readable: true, disk: 3, base: None, ns: None }]).files[0].ns { Some(v) => v, None => -1 })) == 3);assert(((match sync(seq![F { depth: 1, upper_ext: true, skip_dir: false, readable: true, disk: 3, base: None, ns: None }]).files[0].ns { Some(v) => v, None => -1 })) == 3); // f3.up
}
}
fn main() {}
