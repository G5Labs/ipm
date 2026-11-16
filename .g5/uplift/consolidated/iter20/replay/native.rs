#[path="../../../../Verus_verification/consolidated/iter20/ownership.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {
normal_neighbors();extension_neighbor();children_writer_neighbor();reveal_with_fuel(import_refs,3);reveal_with_fuel(find_unique,3);reveal_with_fuel(find_child,3);reveal_with_fuel(child_contains,3);reveal_with_fuel(ref_pass,3);reveal_with_fuel(naming_pass,3);reveal_with_fuel(filter,3);reveal_with_fuel(drain,3);reveal_with_fuel(reference_count,3);assert(loose_lower().len()>0) by (compute);assert(named_lower().len()>0) by (compute);assert(loose_upper().len()>0) by (compute);assert(named_upper().len()>0) by (compute);assert(loose_pkg().len()>0) by (compute);assert(named_pkg().len()>0) by (compute);assert(loose_upper()!=loose_pkg()) by (compute);assert(named_upper()!=named_pkg()) by (compute);assert(home(free(),Class::Loose,Extension::Lower).resource==seq![]);assert(home(free(),Class::Loose,Extension::Upper).resource==seq![]);assert(home(free(),Class::Named,Extension::Lower).resource==seq![]);assert(home(free(),Class::Named,Extension::Upper).resource==seq![]);assert(home(claimed(),Class::Named,Extension::Lower).resource==seq![]);assert(home(claimed(),Class::Named,Extension::Upper).resource==seq![]);
assert((b(visible(free(),Class::Loose))) == 1); // free-visible-loose
assert((b(visible(claimed(),Class::Loose))) == 0); // claimed-visible-loose
assert((b(visible(naming(),Class::Named))) == 0); // naming-visible-named
assert((b(inventory(free()).contains(Class::Loose))) == 1); // free-inventory-loose
assert((b(has_ref(free(),Class::Loose,Extension::Lower))) == 0); // free-loose-reference
assert((b(home(free(),Class::Loose,Extension::Lower).module.is_some())) == 0); // free-loose-home
assert((b(home_name(free(),Class::Loose,Extension::Lower)==seq![])) == 1); // free-loose-home-match
assert((b(home(free(),Class::Loose,Extension::Lower).ref_id.is_some())) == 0); // free-loose-home-reference
assert((b(home(free(),Class::Loose,Extension::Lower).resource==identifier(Class::Loose,Extension::Lower))) == 0); // free-loose-lower-resource-full
assert((b(home_name(free(),Class::Loose,Extension::Upper)==seq![])) == 1); // free-loose-upper-home-match
assert((b(home(free(),Class::Loose,Extension::Upper).resource==package(Class::Loose))) == 0); // free-loose-upper-resource-package
assert((b(home_name(free(),Class::Loose,Extension::Lower)==seq![])) == 1); // free-loose-home-name-reader
assert((b(free().code.contains_key(Class::Loose))) == 1); // free-loose-code
assert((payload(free(),Class::Loose)) == 1); // free-loose-payload
assert((b(inventory(free()).contains(Class::Named))) == 1); // free-inventory-named
assert((b(has_ref(free(),Class::Named,Extension::Lower))) == 0); // free-named-reference
assert((b(home(free(),Class::Named,Extension::Lower).module.is_some())) == 0); // free-named-home
assert((b(home_name(free(),Class::Named,Extension::Lower)==seq![])) == 1); // free-named-home-match
assert((b(home(free(),Class::Named,Extension::Lower).ref_id.is_some())) == 0); // free-named-home-reference
assert((b(home(free(),Class::Named,Extension::Lower).resource==identifier(Class::Named,Extension::Lower))) == 0); // free-named-lower-resource-full
assert((b(home_name(free(),Class::Named,Extension::Upper)==seq![])) == 1); // free-named-upper-home-match
assert((b(home(free(),Class::Named,Extension::Upper).resource==package(Class::Named))) == 0); // free-named-upper-resource-package
assert((b(home_name(free(),Class::Named,Extension::Lower)==seq![])) == 1); // free-named-home-name-reader
assert((b(free().code.contains_key(Class::Named))) == 1); // free-named-code
assert((payload(free(),Class::Named)) == 1); // free-named-payload
assert((b(visible(free(),Class::Named))) == 1); // free-visible-named
assert((b(inventory(claimed()).contains(Class::Loose))) == 1); // claimed-inventory-loose
assert((b(has_ref(claimed(),Class::Loose,Extension::Lower))) == 1); // claimed-loose-reference
assert((b(home(claimed(),Class::Loose,Extension::Lower).module.is_some())) == 1); // claimed-loose-home
assert((b(home_name(claimed(),Class::Loose,Extension::Lower)==claim_module())) == 1); // claimed-loose-home-match
assert((b(home(claimed(),Class::Loose,Extension::Lower).ref_id.is_some())) == 1); // claimed-loose-home-reference
assert((b(home(claimed(),Class::Loose,Extension::Lower).resource==identifier(Class::Loose,Extension::Lower))) == 1); // claimed-loose-lower-resource-full
assert((b(home_name(claimed(),Class::Loose,Extension::Upper)==claim_module())) == 1); // claimed-loose-upper-home-match
assert((b(home(claimed(),Class::Loose,Extension::Upper).resource==package(Class::Loose))) == 0); // claimed-loose-upper-resource-package
assert((b(home_name(claimed(),Class::Loose,Extension::Lower)==claim_module())) == 1); // claimed-loose-home-name-reader
assert((b(claimed().code.contains_key(Class::Loose))) == 1); // claimed-loose-code
assert((payload(claimed(),Class::Loose)) == 1); // claimed-loose-payload
assert((b(exact_ref_owner(claimed(),Class::Loose,Extension::Lower).is_some())) == 1); // claimed-loose-reference-owner
assert((b(exact_ref_owner(claimed(),Class::Loose,Extension::Lower)==Some(claim_module()))) == 1); // claimed-loose-reference-owner-match
assert((b(match lookup_ref(claimed(),identifier(Class::Loose,Extension::Lower)){Some(id)=>child_contains(claimed().refs[id as int].children,identifier(Class::Loose,Extension::Lower),0),None=>false})) == 1); // claimed-loose-child-match-count
assert((b(inventory(claimed()).contains(Class::Named))) == 1); // claimed-inventory-named
assert((b(has_ref(claimed(),Class::Named,Extension::Lower))) == 0); // claimed-named-reference
assert((b(home(claimed(),Class::Named,Extension::Lower).module.is_some())) == 0); // claimed-named-home
assert((b(home_name(claimed(),Class::Named,Extension::Lower)==seq![])) == 1); // claimed-named-home-match
assert((b(home(claimed(),Class::Named,Extension::Lower).ref_id.is_some())) == 0); // claimed-named-home-reference
assert((b(home(claimed(),Class::Named,Extension::Lower).resource==identifier(Class::Named,Extension::Lower))) == 0); // claimed-named-lower-resource-full
assert((b(home_name(claimed(),Class::Named,Extension::Upper)==seq![])) == 1); // claimed-named-upper-home-match
assert((b(home(claimed(),Class::Named,Extension::Upper).resource==package(Class::Named))) == 0); // claimed-named-upper-resource-package
assert((b(home_name(claimed(),Class::Named,Extension::Lower)==seq![])) == 1); // claimed-named-home-name-reader
assert((b(claimed().code.contains_key(Class::Named))) == 1); // claimed-named-code
assert((payload(claimed(),Class::Named)) == 1); // claimed-named-payload
assert((b(visible(claimed(),Class::Named))) == 1); // claimed-visible-named
assert((b(inventory(naming()).contains(Class::Loose))) == 1); // naming-inventory-loose
assert((b(has_ref(naming(),Class::Loose,Extension::Lower))) == 1); // naming-loose-reference
assert((b(home(naming(),Class::Loose,Extension::Lower).module.is_some())) == 1); // naming-loose-home
assert((b(home_name(naming(),Class::Loose,Extension::Lower)==claim_module())) == 1); // naming-loose-home-match
assert((b(home(naming(),Class::Loose,Extension::Lower).ref_id.is_some())) == 1); // naming-loose-home-reference
assert((b(home(naming(),Class::Loose,Extension::Lower).resource==identifier(Class::Loose,Extension::Lower))) == 1); // naming-loose-lower-resource-full
assert((b(home_name(naming(),Class::Loose,Extension::Upper)==claim_module())) == 1); // naming-loose-upper-home-match
assert((b(home(naming(),Class::Loose,Extension::Upper).resource==package(Class::Loose))) == 0); // naming-loose-upper-resource-package
assert((b(home_name(naming(),Class::Loose,Extension::Lower)==claim_module())) == 1); // naming-loose-home-name-reader
assert((b(naming().code.contains_key(Class::Loose))) == 1); // naming-loose-code
assert((payload(naming(),Class::Loose)) == 1); // naming-loose-payload
assert((b(visible(naming(),Class::Loose))) == 0); // naming-visible-loose
assert((b(exact_ref_owner(naming(),Class::Loose,Extension::Lower).is_some())) == 1); // naming-loose-reference-owner
assert((b(exact_ref_owner(naming(),Class::Loose,Extension::Lower)==Some(claim_module()))) == 1); // naming-loose-reference-owner-match
assert((b(match lookup_ref(naming(),identifier(Class::Loose,Extension::Lower)){Some(id)=>child_contains(naming().refs[id as int].children,identifier(Class::Loose,Extension::Lower),0),None=>false})) == 1); // naming-loose-child-match-count
assert((b(inventory(naming()).contains(Class::Named))) == 1); // naming-inventory-named
assert((b(has_ref(naming(),Class::Named,Extension::Lower))) == 0); // naming-named-reference
assert((b(home(naming(),Class::Named,Extension::Lower).module.is_some())) == 1); // naming-named-home
assert((b(home_name(naming(),Class::Named,Extension::Lower)==naming_module())) == 1); // naming-named-home-match
assert((b(home(naming(),Class::Named,Extension::Lower).ref_id.is_some())) == 0); // naming-named-home-reference
assert((b(home(naming(),Class::Named,Extension::Lower).resource==identifier(Class::Named,Extension::Lower))) == 1); // naming-named-lower-resource-full
assert((b(home_name(naming(),Class::Named,Extension::Upper)==naming_module())) == 1); // naming-named-upper-home-match
assert((b(home(naming(),Class::Named,Extension::Upper).resource==package(Class::Named))) == 1); // naming-named-upper-resource-package
assert((b(home_name(naming(),Class::Named,Extension::Lower)==naming_module())) == 1); // naming-named-home-name-reader
assert((b(naming().code.contains_key(Class::Named))) == 1); // naming-named-code
assert((payload(naming(),Class::Named)) == 1); // naming-named-payload
assert((b(inventory(exact()).contains(Class::Loose))) == 1); // exact-inventory-loose
assert((b(has_ref(exact(),Class::Loose,Extension::Lower))) == 1); // exact-loose-reference
assert((b(home(exact(),Class::Loose,Extension::Lower).module.is_some())) == 1); // exact-loose-home
assert((b(home_name(exact(),Class::Loose,Extension::Lower)==claim_module())) == 1); // exact-loose-home-match
assert((b(home(exact(),Class::Loose,Extension::Lower).ref_id.is_some())) == 1); // exact-loose-home-reference
assert((b(home(exact(),Class::Loose,Extension::Lower).resource==identifier(Class::Loose,Extension::Lower))) == 1); // exact-loose-lower-resource-full
assert((b(home_name(exact(),Class::Loose,Extension::Upper)==claim_module())) == 1); // exact-loose-upper-home-match
assert((b(home(exact(),Class::Loose,Extension::Upper).resource==package(Class::Loose))) == 0); // exact-loose-upper-resource-package
assert((b(home_name(exact(),Class::Loose,Extension::Lower)==claim_module())) == 1); // exact-loose-home-name-reader
assert((b(exact().code.contains_key(Class::Loose))) == 1); // exact-loose-code
assert((payload(exact(),Class::Loose)) == 1); // exact-loose-payload
assert((b(visible(exact(),Class::Loose))) == 0); // exact-visible-loose
assert((b(exact_ref_owner(exact(),Class::Loose,Extension::Lower).is_some())) == 1); // exact-loose-reference-owner
assert((b(exact_ref_owner(exact(),Class::Loose,Extension::Lower)==Some(claim_module()))) == 1); // exact-loose-reference-owner-match
assert((b(match lookup_ref(exact(),identifier(Class::Loose,Extension::Lower)){Some(id)=>child_contains(exact().refs[id as int].children,identifier(Class::Loose,Extension::Lower),0),None=>false})) == 1); // exact-loose-child-match-count
assert((b(inventory(exact()).contains(Class::Named))) == 1); // exact-inventory-named
assert((b(has_ref(exact(),Class::Named,Extension::Lower))) == 1); // exact-named-reference
assert((b(home(exact(),Class::Named,Extension::Lower).module.is_some())) == 1); // exact-named-home
assert((b(home_name(exact(),Class::Named,Extension::Lower)==exact_module())) == 1); // exact-named-home-match
assert((b(home(exact(),Class::Named,Extension::Lower).ref_id.is_some())) == 1); // exact-named-home-reference
assert((b(home(exact(),Class::Named,Extension::Lower).resource==identifier(Class::Named,Extension::Lower))) == 1); // exact-named-lower-resource-full
assert((b(home_name(exact(),Class::Named,Extension::Upper)==exact_module())) == 1); // exact-named-upper-home-match
assert((b(home(exact(),Class::Named,Extension::Upper).resource==package(Class::Named))) == 0); // exact-named-upper-resource-package
assert((b(home_name(exact(),Class::Named,Extension::Lower)==exact_module())) == 1); // exact-named-home-name-reader
assert((b(exact().code.contains_key(Class::Named))) == 1); // exact-named-code
assert((payload(exact(),Class::Named)) == 1); // exact-named-payload
assert((b(visible(exact(),Class::Named))) == 0); // exact-visible-named
assert((b(exact_ref_owner(exact(),Class::Named,Extension::Lower).is_some())) == 1); // exact-named-reference-owner
assert((b(exact_ref_owner(exact(),Class::Named,Extension::Lower)==Some(exact_module()))) == 1); // exact-named-reference-owner-match
assert((b(match lookup_ref(exact(),identifier(Class::Named,Extension::Lower)){Some(id)=>child_contains(exact().refs[id as int].children,identifier(Class::Named,Extension::Lower),0),None=>false})) == 1); // exact-named-child-match-count
assert((b(naming().modules.contains(naming_module()))) == 1); // naming-module
assert((reference_count(naming(),naming_module(),0)) == 0); // naming-resource-count
}
}
fn main() {}
