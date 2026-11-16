#[path="../../../../Verus_verification/consolidated/iter55/nsinit.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(!exec(Expr::SetNamespace).namespace_changed)) == 1);assert((b(!exec(Expr::SetNamespace).namespace_changed)) == 1); // e2.after.user
}
}
fn main() {}
