#[path="../../../../Verus_verification/consolidated/iter58/dfiitems.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert(((if loaded(Item { wildcard: true, ext: 2 }) { 1int } else { 0int })) == 0);assert(((if loaded(Item { wildcard: true, ext: 2 }) { 1int } else { 0int })) == 1); // d1.item.IPMCont58W.W3
}
}
fn main() {}
