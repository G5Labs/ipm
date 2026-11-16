#[path="../../../../Verus_verification/consolidated/iter58/dfiitems.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert(((if loaded(Item { wildcard: true, ext: 1 }) { 1int } else { 0int })) == 0);assert(((if loaded(Item { wildcard: true, ext: 1 }) { 1int } else { 0int })) == 0); // d1.item.IPMCont58W.W2
assert(((if present_after_uninstall(Item { wildcard: true, ext: 0 }) { 1int } else { 0int })) == 1);assert(((if present_after_uninstall(Item { wildcard: true, ext: 0 }) { 1int } else { 0int })) == 1); // u1.item.IPMCont58W.W1
assert(((if loaded(Item { wildcard: true, ext: 2 }) { 1int } else { 0int })) == 0);assert(((if loaded(Item { wildcard: true, ext: 2 }) { 1int } else { 0int })) == 0); // d1.item.IPMCont58W.W3
assert(((if loaded(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 1);assert(((if loaded(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 1); // d1.item.IPMCont58.DashA
assert(((if present_after_uninstall(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 0);assert(((if present_after_uninstall(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 0); // u1.item.IPMCont58.DashA
assert(((if loaded(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 1);assert(((if loaded(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 1); // d1.item.IPMCont58.DashB
assert(((if present_after_uninstall(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 0);assert(((if present_after_uninstall(Item { wildcard: false, ext: 0 }) { 1int } else { 0int })) == 0); // u1.item.IPMCont58.DashB
assert(((if loaded(Item { wildcard: true, ext: 0 }) { 1int } else { 0int })) == 1);assert(((if loaded(Item { wildcard: true, ext: 0 }) { 1int } else { 0int })) == 1); // d1.item.IPMCont58W.W1
assert(((if present_after_uninstall(Item { wildcard: true, ext: 1 }) { 1int } else { 0int })) == 0);assert(((if present_after_uninstall(Item { wildcard: true, ext: 1 }) { 1int } else { 0int })) == 0); // u1.item.IPMCont58W.W2
assert(((if present_after_uninstall(Item { wildcard: true, ext: 2 }) { 1int } else { 0int })) == 0);assert(((if present_after_uninstall(Item { wildcard: true, ext: 2 }) { 1int } else { 0int })) == 0); // u1.item.IPMCont58W.W3
assert(((if loaded(Item { wildcard: true, ext: 3 }) { 1int } else { 0int })) == 1);assert(((if loaded(Item { wildcard: true, ext: 3 }) { 1int } else { 0int })) == 1); // d1.item.IPMCont58W.W4
assert(((if present_after_uninstall(Item { wildcard: true, ext: 3 }) { 1int } else { 0int })) == 1);assert(((if present_after_uninstall(Item { wildcard: true, ext: 3 }) { 1int } else { 0int })) == 1); // u1.item.IPMCont58W.W4
}
}
fn main() {}
