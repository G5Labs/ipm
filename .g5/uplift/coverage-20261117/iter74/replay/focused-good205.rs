#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_205() {
assert((b(ver_is(rec1(cli_install(i6())), v100().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_install(i6())), v100().text))) == 1); // i6.r1.ver
}
}
fn main() {}
