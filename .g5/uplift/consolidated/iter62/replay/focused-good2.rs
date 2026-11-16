#[path="../../../../Verus_verification/consolidated/iter62/interopres.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert(((if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int }) * 9 + (1 - (if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int })) * -1) == 9);assert(((if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int }) * 9 + (1 - (if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int })) * -1) == 9); // f1.I62F.k
}
}
fn main() {}
