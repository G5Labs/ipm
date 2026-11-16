#[path="../../../../Verus_verification/consolidated/iter14/protection.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(seq![1int,2,5],roots(),before(false,false)))) == 0); // classify-child
}
}
fn main() {}
