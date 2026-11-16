#[path="../../../../Verus_verification/consolidated/iter64/failkeep.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(kept(Kind::File, 3, false))) == 1);assert((b(kept(Kind::File, 3, false))) == 1); // la.filea
}
}
fn main() {}
