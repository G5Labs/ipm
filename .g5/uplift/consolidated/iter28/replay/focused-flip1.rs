#[path="../../../../Verus_verification/consolidated/iter28/script.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b(ok(named(Case::TxtUnknown,false))) }
pub proof fn establish() ensures value()==0 {
assert(first()!=last() && first()!=unknown() && last()!=unknown() && fault()!=first() && fault()!=last() && fault()!=unknown()) by(compute);text_preparation();json_preparation();shape_neighbor();execution_neighbor();dry_execution_neighbor();runtime_neighbor();named_neighbors();reveal_with_fuel(execute,4);output_neighbors();

assert((b(ok(named(Case::TxtUnknown,false)))) == 0); // txt-unknown-ok
}
}
}
verus! { pub proof fn observation_1(){ measured::establish();assert(measured::value()==1); // txt-unknown-ok
} }
fn main(){}
