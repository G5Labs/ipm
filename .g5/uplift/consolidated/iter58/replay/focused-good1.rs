#[path="../../../../Verus_verification/consolidated/iter58/dfiitems.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert(((if present_after_uninstall(Item { wildcard: true, ext: 0 }) { 1int } else { 0int })) == 1);assert(((if present_after_uninstall(Item { wildcard: true, ext: 0 }) { 1int } else { 0int })) == 1); // u1.item.IPMCont58W.W1
}
}
fn main() {}
