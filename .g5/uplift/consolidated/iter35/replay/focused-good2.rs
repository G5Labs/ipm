#[path="../../../../Verus_verification/consolidated/iter35/listing.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((flat_pos(mods(), all(), 10, 0)) == 11) by (compute);assert((flat_pos(mods(), all(), 10, 0)) == 11); // l0.pos.i35n
}
}
fn main() {}
