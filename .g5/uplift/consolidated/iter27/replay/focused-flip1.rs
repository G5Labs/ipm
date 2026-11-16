#[path="../../../../Verus_verification/consolidated/iter27/purge.rs"] mod model;
use model::*;
use vstd::prelude::*;
mod measured {
use super::*;
verus! {
pub closed spec fn value()->int { observed_callback(removed(Case::Hook,Flag::Configure)).purge as int }
pub proof fn establish() ensures value()==1 {
assert(data_path(top_name())==top_key()) by(compute);assert(data_path(dotted_name())==nested_key()) by(compute);lifecycle_neighbors();

assert((observed_callback(removed(Case::Hook,Flag::Configure)).purge as int) == 1); // hook-config-hook-purge
}
}
}
verus! { pub proof fn observation_1(){ measured::establish();assert(measured::value()==0); // hook-config-hook-purge
} }
fn main(){}
