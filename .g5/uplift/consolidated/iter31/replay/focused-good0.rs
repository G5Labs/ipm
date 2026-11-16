#[path="../../../../Verus_verification/consolidated/iter31/installed.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
resolution_neighbor();reference_neighbor();directory_neighbor();assert((code(s2_p().world.b)) == 170); // s2-p.b
}
}
fn main() {}
