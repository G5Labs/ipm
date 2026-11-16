#[path="../../../../Verus_verification/consolidated/iter14/protection.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(seq![1int],roots(),before(false,false)))) == 1); // classify-ancestor
}
}
fn main() {}
