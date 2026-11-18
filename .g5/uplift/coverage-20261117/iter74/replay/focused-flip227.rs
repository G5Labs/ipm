#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_227() {
assert((nph(twin_while_running(h::Action::Install, n_h74a()))) == 0) by (compute_only);assert((nph(twin_while_running(h::Action::Install, n_h74a()))) == 1); // mid.install.permph
}
}
fn main() {}
