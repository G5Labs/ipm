#[path="../../../../Verus_verification/consolidated/iter35/listing.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((vcode(mods(), cut(), 1)) == 9) by (compute);assert((vcode(mods(), cut(), 1)) == 9); // t2.version.i35a
}
}
fn main() {}
