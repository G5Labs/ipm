#[path="../../../../Verus_verification/consolidated/iter44/comments.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(arrange(false).ok)) == 0) by (compute);assert((b(arrange(false).ok)) == 1); // a1.ok
}
}
fn main() {}
