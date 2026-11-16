#[path="../../../../Verus_verification/consolidated/iter25/developer_sources.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b(ok(developer())) }
pub proof fn establish() ensures value()==1 {
lifecycle_staged();constructor_neighbor();layout_neighbor();reader_neighbor();constants_neighbor();
assert((b(ok(developer()))) == 1);
}
}
}
verus! { pub proof fn observation_0(){ measured::establish();assert(measured::value()==0); } }
fn main(){}
