#[path="../../../../Verus_verification/consolidated/iter43/schooks.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(observe(Config::Composite { primary_ipm: true }, Op::CompileOnly, 6).ok)) == 0) by (compute);assert((b(observe(Config::Composite { primary_ipm: true }, Op::CompileOnly, 6).ok)) == 1); // p5.ok
}
}
fn main() {}
