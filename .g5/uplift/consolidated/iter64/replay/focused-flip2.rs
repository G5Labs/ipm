#[path="../../../../Verus_verification/consolidated/iter64/failkeep.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(kept(Kind::Mapping, 2, true))) == 1);assert((b(kept(Kind::Mapping, 2, true))) == 0); // dv.mapg
}
}
fn main() {}
