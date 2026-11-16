#[path="../../../../Verus_verification/consolidated/iter60/legacymsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert(((if survives_uninstall(Spec::Single(1), 1) { 1int } else { -1int })) == -1) by (compute);assert(((if survives_uninstall(Spec::Single(1), 1) { 1int } else { -1int })) == -1); // u2.inc1.one
}
}
fn main() {}
