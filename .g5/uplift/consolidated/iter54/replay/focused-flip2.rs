#[path="../../../../Verus_verification/consolidated/iter54/helpcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert(((if intro_current_registry(1) { 1int } else { 0int })) == 1);assert(((if intro_current_registry(1) { 1int } else { 0int })) == 0); // i3.currentregistry
}
}
fn main() {}
