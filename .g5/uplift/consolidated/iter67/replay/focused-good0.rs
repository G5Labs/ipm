#[path="../../../../Verus_verification/consolidated/iter67/composite.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(read_only(cfg(true, true, false, true)))) == 0);assert((b(read_only(cfg(true, true, false, true)))) == 0); // r1.ro
}
}
fn main() {}
