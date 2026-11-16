#[path="../../../../Verus_verification/consolidated/iter26/sync_summary.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b(has_row(a1(),zeta(),1)) }
pub proof fn establish() ensures value()==1 {
reveal_with_fuel(rows_from,5);reveal_with_fuel(report_from,4);staged_store();report_neighbor();selected_neighbor();caller_neighbor();

assert((b(has_row(a1(),zeta(),1))) == 1); // a1-zeta-class1-exists
}
}
}
verus! { pub proof fn observation_2(){ measured::establish();assert(measured::value()==1); // a1-zeta-class1-exists
} }
fn main(){}
