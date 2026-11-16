#[path="../../../../Verus_verification/consolidated/iter20/ownership.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
normal_neighbors();extension_neighbor();children_writer_neighbor();reveal_with_fuel(import_refs,3);reveal_with_fuel(find_unique,3);reveal_with_fuel(find_child,3);reveal_with_fuel(child_contains,3);reveal_with_fuel(ref_pass,3);reveal_with_fuel(naming_pass,3);reveal_with_fuel(filter,3);reveal_with_fuel(drain,3);reveal_with_fuel(reference_count,3);assert(loose_lower().len()>0) by (compute);assert(named_lower().len()>0) by (compute);assert(loose_upper().len()>0) by (compute);assert(named_upper().len()>0) by (compute);assert(loose_pkg().len()>0) by (compute);assert(named_pkg().len()>0) by (compute);assert(loose_upper()!=loose_pkg()) by (compute);assert(named_upper()!=named_pkg()) by (compute);assert(home(free(),Class::Loose,Extension::Lower).resource==seq![]);assert(home(free(),Class::Loose,Extension::Upper).resource==seq![]);assert(home(free(),Class::Named,Extension::Lower).resource==seq![]);assert(home(free(),Class::Named,Extension::Upper).resource==seq![]);assert(home(claimed(),Class::Named,Extension::Lower).resource==seq![]);assert(home(claimed(),Class::Named,Extension::Upper).resource==seq![]);assert((b(visible(claimed(),Class::Loose))) == 0); // claimed-visible-loose
}
}
fn main() {}
