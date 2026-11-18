// Iteration75 (repair group R8B): a phase-scoped requirement is classified for resolution only when its phase is
// in the command's phases. IPM 571e04a1.
//
// dep_resolve.rs::active and ::classify are inherited unchanged and faithful:
//   Storage/Module.cls:834-855 HasScope: an empty Scope always counts; otherwise the scope (or any item of a
//   scope list) must be in pPhases, compared in lower case. dep_resolve.rs represents one scope token.
//   Storage/Module.cls:955 (BuildDependencyGraph) and :1256 skip a requirement when HasScope is false.
#[path="../../../../playbook-uplift/models/verus/dep_resolve.rs"]
pub mod dep;
use vstd::prelude::*;

verus! {

use dep::{Req, Item, active, classify, items_from};

pub open spec fn test_phase() -> nat { 9 }
pub open spec fn load_phases() -> Set<nat> { set![1nat, 2, 3] }
pub open spec fn test_phases() -> Set<nat> { set![1nat, 2, 3, 9] }
pub open spec fn scoped() -> Req { Req { name: 5, range: ISet::<nat>::full(), exact: true, scope: Some(test_phase()) } }
pub open spec fn unscoped() -> Req { Req { name: 6, range: ISet::<nat>::full(), exact: true, scope: None } }

// deps#4: the same manifest yields a resolution item for the test-scoped requirement under `test` and none under
// `load`; the unscoped requirement is classified in both.
pub proof fn scoped_requirement_neighbors() {
    reveal_with_fuel(items_from, 3);
    assert(!active(load_phases(), scoped()));
    assert(active(test_phases(), scoped()));
    let reqs = seq![scoped(), unscoped()];
    let under_load = classify(load_phases(), reqs, seq![0nat]);
    let under_test = classify(test_phases(), reqs, seq![0nat]);
    assert(under_load.len() == 1 && under_load[0].idx == 1);
    assert(under_test.len() == 2 && under_test[0].idx == 0 && under_test[1].idx == 1);
}

}
fn main() {}
