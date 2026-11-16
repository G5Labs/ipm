#[path="../../../../Verus_verification/consolidated/iter47/formats.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(strip_trailing_lf(yaml_text(leading())) =~= leading())) == 0) by (compute);assert((b(strip_trailing_lf(yaml_text(leading())) =~= leading())) == 0); // yamlf.stripped.TestELeading
}
}
fn main() {}
