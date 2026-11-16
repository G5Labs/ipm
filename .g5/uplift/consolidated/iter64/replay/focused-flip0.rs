#[path="../../../../Verus_verification/consolidated/iter64/failkeep.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(kept(Kind::Cpf, 2, false))) == 1);assert((b(kept(Kind::Cpf, 2, false))) == 0); // ec.cpf
}
}
fn main() {}
