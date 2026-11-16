#[path="../../../../Verus_verification/consolidated/iter35/listing.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((flat_pos(mods(), all(), 9, 2)) == 2) by (compute);assert((flat_pos(mods(), all(), 9, 2)) == 2); // l1.pos.i35pb
}
}
fn main() {}
