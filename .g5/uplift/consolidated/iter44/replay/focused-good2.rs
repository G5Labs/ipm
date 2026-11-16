#[path="../../../../Verus_verification/consolidated/iter44/comments.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(after(c(1), true, stored(1)))) == 1) by (compute);assert((b(after(c(1), true, stored(1)))) == 1); // s1.c1.count
}
}
fn main() {}
