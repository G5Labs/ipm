#![allow(non_snake_case)]
// Iteration53: `generate` manifest authoring (template, interactive answers, -export filter, resource grouping).
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter53/bindings.json):
//   Main.cls::GenerateModuleXML                 prompt order; passes author answers as (person, org, license, copyright, notes)
//   Storage/ModuleTemplate.cls::SetAuthorProps  parameters (person, org, copy, license, notes)
//   Storage/ModuleTemplate.cls::GenerateTemplate, SetTemplateProps   fixed template values
//   Storage/ModuleTemplate.cls::ExportResources, ExportOneResource, Expfilename
//                                               classes: prefix mask and prefix ignore; routines: substring mask only
//   Storage/ModuleTemplate.cls::NewTemplate, ReadResources, ReadItems, GroupClasses, SharedPackage, AddPackages,
//                                AddDependant   resources and dependencies written to module.xml
// String predicates (prefix, substring) over the fixture names are supplied as facts per item; the
// premise is that ObjectScript `[`, `$find` and `$extract` behave as their names say (native-qualified).
use vstd::prelude::*;

verus! {

// Author answers in prompt order, and what the manifest stores.
pub struct Answers { pub license: int, pub copyright: int }
pub struct Author { pub license: int, pub copyright: int }
// SetAuthorProps(pPerson, pOrg, pCopy, pLicense, pNotes) called with (person, org, license, copyright, notes).
pub open spec fn set_author_props(p_copy: int, p_license: int) -> Author { Author { license: p_license, copyright: p_copy } }
pub open spec fn stored_author(a: Answers) -> Author { set_author_props(a.license, a.copyright) }

// Export filter.
pub struct Item { pub is_class: bool, pub prefix_mask: bool, pub contains_mask: bool, pub prefix_ignore: bool }
pub open spec fn exported(i: Item) -> bool {
    if i.is_class { i.prefix_mask && !i.prefix_ignore } else { i.prefix_mask }
}

// Grouping: classes in one directory with the same top-level package give one resource for their shared package.
pub open spec fn shared_resource(first_top: int, second_top: int) -> bool { first_top == second_top }

pub proof fn author_swapped(a: Answers)
    requires a.license != a.copyright
    ensures stored_author(a).license == a.copyright, stored_author(a).copyright == a.license
{}

pub proof fn export_neighbors()
    ensures
        exported(Item { is_class: true, prefix_mask: true, contains_mask: true, prefix_ignore: false }),
        !exported(Item { is_class: true, prefix_mask: true, contains_mask: true, prefix_ignore: true }),
        exported(Item { is_class: false, prefix_mask: true, contains_mask: true, prefix_ignore: true }),
        exported(Item { is_class: false, prefix_mask: false, contains_mask: true, prefix_ignore: false }),
        !exported(Item { is_class: true, prefix_mask: false, contains_mask: true, prefix_ignore: false }),
{}

pub proof fn grouping_neighbors()
    ensures shared_resource(1, 1), !shared_resource(1, 2)
{}

// Fixture codes: license answer 1 (Apache-2.0), copyright answer 2 (2024).
pub open spec fn fixture_author() -> Author { stored_author(Answers { license: 1, copyright: 2 }) }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub open spec fn keep() -> Item { Item { is_class: true, prefix_mask: true, contains_mask: true, prefix_ignore: false } }
pub open spec fn skip_cls() -> Item { Item { is_class: true, prefix_mask: true, contains_mask: true, prefix_ignore: true } }
pub open spec fn rout() -> Item { Item { is_class: false, prefix_mask: true, contains_mask: true, prefix_ignore: false } }
pub open spec fn skip_rout() -> Item { Item { is_class: false, prefix_mask: true, contains_mask: true, prefix_ignore: true } }
pub open spec fn zz() -> Item { Item { is_class: false, prefix_mask: false, contains_mask: true, prefix_ignore: false } }

} // verus!

fn main() {}
