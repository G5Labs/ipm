#[path="../../../../Verus_verification/consolidated/iter46/summary.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(match a().req { Some(r) => health_entry(r), None => false })) == 0);assert((b(match a().req { Some(r) => health_entry(r), None => false })) == 1); // a.sysreq.health
}
}
fn main() {}
