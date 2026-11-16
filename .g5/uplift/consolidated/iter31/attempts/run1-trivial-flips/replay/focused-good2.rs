#[path="../../../../Verus_verification/consolidated/iter31/installed.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
resolution_neighbor();reference_neighbor();directory_neighbor();assert((bi(empty_outcome().msg_none)) == 0); // repo.msg-no-satisfactory
}
}
fn main() {}
