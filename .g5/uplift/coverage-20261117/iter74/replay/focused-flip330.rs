#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_330() {
t1_facts();assert((cmd_rows(Cmd::Phase(Ph::Test), w_single(), 2, 3, t1_scoped()).len()) == 0); // t1.rows
}
}
fn main() {}
