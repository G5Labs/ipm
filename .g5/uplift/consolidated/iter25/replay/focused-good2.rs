#[path="../../../../Verus_verification/consolidated/iter25/developer_sources.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b(ok(explicit_developer())) }
pub proof fn establish() ensures value()==1 {
lifecycle_staged();constructor_neighbor();layout_neighbor();reader_neighbor();constants_neighbor();

assert((b(ok(explicit_developer()))) == 1); // reinstall-explicit-dev
}
}
}
verus! { pub proof fn observation_2(){ measured::establish();assert(measured::value()==1); // reinstall-explicit-dev
} }
fn main(){}
