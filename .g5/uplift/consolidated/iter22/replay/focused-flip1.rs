#[path="../../../../Verus_verification/consolidated/iter22/reinstall.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
caller_neighbor();catalog_neighbor();reinstall_neighbor();deletion_neighbor();history_neighbor();reveal_with_fuel(search_from,4);assert(one()!=two()) by(compute);assert(alpha().len()>0) by(compute);assert(beta().len()>0) by(compute);assert((b(supplier(default_result().world)==beta())) == 1); // re-alpha-current-beta
}
}
fn main() {}
