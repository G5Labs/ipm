#[path="../../../../Verus_verification/consolidated/iter25/developer_sources.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { b(ok(refused())) }
pub proof fn establish() ensures value()==0 {
lifecycle_staged();constructor_neighbor();layout_neighbor();reader_neighbor();constants_neighbor();

assert((b(ok(refused()))) == 0); // reinstall-normal-after-dev
}
}
}
verus! { pub proof fn observation_1(){ measured::establish();assert(measured::value()==0); // reinstall-normal-after-dev
} }
fn main(){}
