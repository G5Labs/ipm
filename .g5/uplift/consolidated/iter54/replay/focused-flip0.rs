#[path="../../../../Verus_verification/consolidated/iter54/helpcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert(((if help(Name::Structural).warning { 1int } else { 0int })) == 0);assert(((if help(Name::Structural).warning { 1int } else { 0int })) == 1); // h5.warning
}
}
fn main() {}
