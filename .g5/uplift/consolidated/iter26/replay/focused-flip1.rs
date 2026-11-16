#[path="../../../../Verus_verification/consolidated/iter26/sync_summary.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b((begun()).active) }
pub proof fn establish() ensures value()==1 {
reveal_with_fuel(rows_from,5);reveal_with_fuel(report_from,4);staged_store();report_neighbor();selected_neighbor();caller_neighbor();

assert((b((begun()).active)) == 1); // begun-active
}
}
}
verus! { pub proof fn observation_1(){ measured::establish();assert(measured::value()==0); // begun-active
} }
fn main(){}
