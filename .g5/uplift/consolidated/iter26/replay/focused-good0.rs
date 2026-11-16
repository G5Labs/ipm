#[path="../../../../Verus_verification/consolidated/iter26/sync_summary.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b((inactive()).active) }
pub proof fn establish() ensures value()==0 {
reveal_with_fuel(rows_from,5);reveal_with_fuel(report_from,4);staged_store();report_neighbor();selected_neighbor();caller_neighbor();

assert((b((inactive()).active)) == 0); // inactive-active
}
}
}
verus! { pub proof fn observation_0(){ measured::establish();assert(measured::value()==0); // inactive-active
} }
fn main(){}
