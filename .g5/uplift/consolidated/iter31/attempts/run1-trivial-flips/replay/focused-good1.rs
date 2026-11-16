#[path="../../../../Verus_verification/consolidated/iter31/installed.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
resolution_neighbor();reference_neighbor();directory_neighbor();assert((bi(empty_outcome().msg_installed)) == 0); // repo.msg-installed-req
}
}
fn main() {}
