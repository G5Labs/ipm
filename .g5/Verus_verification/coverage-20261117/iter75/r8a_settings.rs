// Iteration75 neighbours for the TestReportFormat setter (settings.report-format #1) and the test resource
// target validation (build.test.selection #1).
//
// Deciding computations are inherited and imported unchanged; both are faithful to pin 571e04a1:
// - iter3 public_repairs.rs `format_valid`/`set_format` and playbook settings.rs `valid_trf`/`config_set`:
//   Repo/UniversalSettings.cls:225-233 SetTestReportFormat refuses a non-empty value whose
//   $zconvert(val,"l") is not in Test/Abstract.cls:7 VALIDFORMATS "json,yaml,toon", and otherwise stores val
//   as supplied through SetValue. UpdateOne (UniversalSettings.cls:123-125) routes `config set` there.
// - iter3 public_repairs.rs `valid_test_target`: ResourceProcessor/Test.cls:615-621 %OnValidateObject
//   refuses a resource with neither or both of Package and Class.
#[path = "../../consolidated/iter3/public_repairs.rs"]
mod pr;
#[path = "../../../../playbook-uplift/models/verus/settings.rs"]
mod pb;

use vstd::prelude::*;

verus! {

// "YAML" and "ToOn" are accepted whatever their case and stored with the supplied spelling; "yaml" too.
pub proof fn yaml_and_toon_accepted()
    ensures ({
        let first = pr::set_value(pr::Setting { current: seq![], first: None }, seq![1int], false);
        let yaml_upper = seq![89int, 65, 77, 76];
        let yaml = seq![121int, 97, 109, 108];
        let toon_mixed = seq![84int, 111, 79, 110];
        &&& pr::set_format(first, yaml_upper, true).is_some()
        &&& pr::set_format(first, yaml_upper, true).unwrap().current == yaml_upper
        &&& pr::set_format(first, yaml, true).unwrap().current == yaml
        &&& pr::set_format(first, toon_mixed, true).is_some()
        &&& pr::set_format(first, toon_mixed, true).unwrap().current == toon_mixed
        // A near miss is refused.
        &&& pr::set_format(first, seq![121int, 97, 109], true).is_none()
    })
{
    reveal_with_fuel(pr::lower, 6);
    assert(pr::lower(seq![89int, 65, 77, 76]) =~= seq![121int, 97, 109, 108]);
    assert(pr::lower(seq![121int, 97, 109, 108]) =~= seq![121int, 97, 109, 108]);
    assert(pr::lower(seq![84int, 111, 79, 110]) =~= seq![116int, 111, 111, 110]);
    let near = seq![121int, 97, 109];
    assert(pr::lower(near) =~= near);
    assert(pr::lower(near).len() == 3);
}

// Through the playbook `config set` path: yaml (11) and toon (12) are stored; an unknown value is not.
pub proof fn config_set_stores_yaml_and_toon()
    ensures
        pb::config_get(pb::config_set(pb::cfg0(), 0, pb::k_trf(), 11).0, 0, pb::k_trf()) == Some(11nat),
        pb::config_get(pb::config_set(pb::cfg0(), 0, pb::k_trf(), 12).0, 0, pb::k_trf()) == Some(12nat),
        pb::config_get(pb::config_set(pb::cfg0(), 0, pb::k_trf(), 10).0, 0, pb::k_trf()) == Some(10nat),
        pb::config_get(pb::config_set(pb::cfg0(), 0, pb::k_trf(), 99).0, 0, pb::k_trf()) == Some(0nat),
{}

// A test resource with only Package is valid, like one with only Class; both or neither are refused.
pub proof fn package_only_test_target_valid()
    ensures
        pr::valid_test_target(seq![], seq![65int]),
        pr::valid_test_target(seq![65int], seq![]),
        !pr::valid_test_target(seq![65int], seq![66int]),
        !pr::valid_test_target(seq![], seq![]),
{}

} // verus!

fn main() {}
