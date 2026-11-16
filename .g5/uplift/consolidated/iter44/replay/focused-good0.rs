#[path="../../../../Verus_verification/consolidated/iter44/comments.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(after(c(4), true, stored(4)))) == 0) by (compute);assert((b(after(c(4), true, stored(4)))) == 0); // s1.c4.count
}
}
fn main() {}
