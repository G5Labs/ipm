#[path="../../../../Verus_verification/consolidated/iter43/schooks.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((total(observe(Config::Direct, Op::Load, -1))) == 0) by (compute);assert((total(observe(Config::Direct, Op::Load, -1))) == 1); // d1.total
}
}
fn main() {}
