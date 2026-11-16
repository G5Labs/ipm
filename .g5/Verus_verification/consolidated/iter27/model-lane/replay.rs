use vstd::prelude::*;
#[path="../purge.rs"] pub mod model;
use model::*;
verus!{pub proof fn observed_group(){
assert(data_path(top_name())==top_key()) by(compute);assert(data_path(dotted_name())==nested_key()) by(compute);lifecycle_neighbors();
// hook-purge-hook-purge

assert((observed_callback(removed(Case::Hook,Flag::Purge)).purge as int)==0);
// hook-config-hook-purge

assert((observed_callback(removed(Case::Hook,Flag::Configure)).purge as int)==1);
// hook-purge-hook-top

assert((observed_callback(removed(Case::Hook,Flag::Purge)).top as int)==1);
// plain-default-before-count

assert((count(seeded(Case::Plain)))==1);
// plain-default-before-value

assert((value(seeded(Case::Plain)))==271);
// plain-default-after-count

assert((count(after(Case::Plain,Flag::Default)))==1);
// plain-default-after-value

assert((value(after(Case::Plain,Flag::Default)))==271);
// plain-default-module-gone

assert((b((removed(Case::Plain,Flag::Default)).module.is_none()))==1);
// plain-default-class-gone

assert((b(!(removed(Case::Plain,Flag::Default)).class))==1);
// plain-purge-before-count

assert((count(seeded(Case::Plain)))==1);
// plain-purge-before-value

assert((value(seeded(Case::Plain)))==271);
// plain-purge-after-count

assert((count(after(Case::Plain,Flag::Purge)))==1);
// plain-purge-after-value

assert((value(after(Case::Plain,Flag::Purge)))==271);
// plain-purge-module-gone

assert((b((removed(Case::Plain,Flag::Purge)).module.is_none()))==1);
// plain-purge-class-gone

assert((b(!(removed(Case::Plain,Flag::Purge)).class))==1);
// plain-config-before-count

assert((count(seeded(Case::Plain)))==1);
// plain-config-before-value

assert((value(seeded(Case::Plain)))==271);
// plain-config-after-count

assert((count(after(Case::Plain,Flag::Configure)))==1);
// plain-config-after-value

assert((value(after(Case::Plain,Flag::Configure)))==271);
// plain-config-module-gone

assert((b((removed(Case::Plain,Flag::Configure)).module.is_none()))==1);
// plain-config-class-gone

assert((b(!(removed(Case::Plain,Flag::Configure)).class))==1);
// hook-default-before-count

assert((count(seeded(Case::Hook)))==1);
// hook-default-before-value

assert((value(seeded(Case::Hook)))==271);
// hook-default-after-count

assert((count(after(Case::Hook,Flag::Default)))==1);
// hook-default-after-value

assert((value(after(Case::Hook,Flag::Default)))==271);
// hook-default-module-gone

assert((b((removed(Case::Hook,Flag::Default)).module.is_none()))==1);
// hook-default-class-gone

assert((b(!(removed(Case::Hook,Flag::Default)).class))==1);
// hook-default-hook-called

assert((observed_callback(removed(Case::Hook,Flag::Default)).called as int)==1);
// hook-default-hook-purge

assert((observed_callback(removed(Case::Hook,Flag::Default)).purge as int)==0);
// hook-default-hook-top

assert((observed_callback(removed(Case::Hook,Flag::Default)).top as int)==0);
// hook-default-hook-nested

assert((observed_callback(removed(Case::Hook,Flag::Default)).nested as int)==0);
// hook-default-hook-after

assert((observed_callback(removed(Case::Hook,Flag::Default)).after as int)==1);
// hook-purge-before-count

assert((count(seeded(Case::Hook)))==1);
// hook-purge-before-value

assert((value(seeded(Case::Hook)))==271);
// hook-purge-after-count

assert((count(after(Case::Hook,Flag::Purge)))==1);
// hook-purge-after-value

assert((value(after(Case::Hook,Flag::Purge)))==271);
// hook-purge-module-gone

assert((b((removed(Case::Hook,Flag::Purge)).module.is_none()))==1);
// hook-purge-class-gone

assert((b(!(removed(Case::Hook,Flag::Purge)).class))==1);
// hook-purge-hook-called

assert((observed_callback(removed(Case::Hook,Flag::Purge)).called as int)==1);
// hook-purge-hook-nested

assert((observed_callback(removed(Case::Hook,Flag::Purge)).nested as int)==0);
// hook-purge-hook-after

assert((observed_callback(removed(Case::Hook,Flag::Purge)).after as int)==1);
// hook-config-before-count

assert((count(seeded(Case::Hook)))==1);
// hook-config-before-value

assert((value(seeded(Case::Hook)))==271);
// hook-config-after-count

assert((count(after(Case::Hook,Flag::Configure)))==0);
// hook-config-after-value

assert((value(after(Case::Hook,Flag::Configure)))==0);
// hook-config-module-gone

assert((b((removed(Case::Hook,Flag::Configure)).module.is_none()))==1);
// hook-config-class-gone

assert((b(!(removed(Case::Hook,Flag::Configure)).class))==1);
// hook-config-hook-called

assert((observed_callback(removed(Case::Hook,Flag::Configure)).called as int)==1);
// hook-config-hook-top

assert((observed_callback(removed(Case::Hook,Flag::Configure)).top as int)==0);
// hook-config-hook-nested

assert((observed_callback(removed(Case::Hook,Flag::Configure)).nested as int)==1);
// hook-config-hook-after

assert((observed_callback(removed(Case::Hook,Flag::Configure)).after as int)==1);
// calibration-before-count

assert((count(sdk_before()))==1);
// calibration-before-value

assert((value(sdk_before()))==271);
// calibration-after-count

assert((count(sdk_after()))==0);
// calibration-after-value

assert((value(sdk_after()))==0);
// calibration-class-present

assert((b(sdk_after().class))==1);
}}
fn main(){}
