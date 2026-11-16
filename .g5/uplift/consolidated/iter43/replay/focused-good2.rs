#[path="../../../../Verus_verification/consolidated/iter43/schooks.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((count(observe(Config::Composite { primary_ipm: true }, Op::Uninstall, -1).events, 30)) == 1) by (compute);assert((count(observe(Config::Composite { primary_ipm: true }, Op::Uninstall, -1).events, 30)) == 1); // p6.B.Unconfigure
}
}
fn main() {}
