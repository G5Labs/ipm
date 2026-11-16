#[path="../../../../Verus_verification/consolidated/iter14/model-lane/expected-clear.rs"] mod expected;
use expected::*;
use vstd::prelude::*;
verus! {pub proof fn actual_unix_observations(){lifecycle_neighbors(); assert(b(ordinary_loaded().sentinel)==1); assert(b(nested_loaded().nested_marker)==1);}}
fn main(){}
