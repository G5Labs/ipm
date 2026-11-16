#[path="../../../../Verus_verification/consolidated/iter45/scan.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
reveal_with_fuel(any_unreadable_change, 2); assert(((match sync(seq![inner(2, 1)]).files[0].ns { Some(v) => v, None => -1 })) == 2);assert(((match sync(seq![inner(2, 1)]).files[0].ns { Some(v) => v, None => -1 })) == 2); // f1.inner
}
}
fn main() {}
