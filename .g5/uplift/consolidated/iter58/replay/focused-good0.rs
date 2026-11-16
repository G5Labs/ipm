#[path="../../../../Verus_verification/consolidated/iter58/dfiitems.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert(((if loaded(Item { wildcard: true, ext: 1 }) { 1int } else { 0int })) == 0);assert(((if loaded(Item { wildcard: true, ext: 1 }) { 1int } else { 0int })) == 0); // d1.item.IPMCont58W.W2
}
}
fn main() {}
