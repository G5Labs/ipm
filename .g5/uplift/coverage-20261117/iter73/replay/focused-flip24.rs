#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_24() {
assert((b(m8_disk().backups == seq![1int])) == 1) by (compute_only);assert((b(m8_disk().backups == seq![1int])) == 0); // m8.backup.old
}
}
fn main() {}
