#[path="../../../../Verus_verification/consolidated/iter54/helpcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert(((if shows_only(COMPILE()) { 1int } else { 0int })) == 1);assert(((if shows_only(COMPILE()) { 1int } else { 0int })) == 1); // h6.only
}
}
fn main() {}
