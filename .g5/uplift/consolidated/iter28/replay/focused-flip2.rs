#[path="../../../../Verus_verification/consolidated/iter28/script.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b(ok(named(Case::TxtUnknown,true))) }
pub proof fn establish() ensures value()==1 {
assert(first()!=last() && first()!=unknown() && last()!=unknown() && fault()!=first() && fault()!=last() && fault()!=unknown()) by(compute);text_preparation();json_preparation();shape_neighbor();execution_neighbor();dry_execution_neighbor();runtime_neighbor();named_neighbors();reveal_with_fuel(execute,4);output_neighbors();

assert((b(ok(named(Case::TxtUnknown,true)))) == 1); // txt-dry-unknown-ok
}
}
}
verus! { pub proof fn observation_2(){ measured::establish();assert(measured::value()==0); // txt-dry-unknown-ok
} }
fn main(){}
