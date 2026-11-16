#[path="../../../../Verus_verification/consolidated/iter30/phases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(case(0).ok)) == 1) by (compute);assert((b(case(0).ok)) == 1); // setup.ok
assert((case(0).starts.len() as int) == 6) by (compute);assert((case(0).starts.len() as int) == 6); // setup.starts
assert((b(!case(0).ok)) == 0) by (compute);assert((b(!case(0).ok)) == 0); // setup.failures
assert((delta_compile(0)) == 1) by (compute);assert((delta_compile(0)) == 1); // setup.hook.Compile
assert((delta_activate(0)) == 1) by (compute);assert((delta_activate(0)) == 1); // setup.hook.Activate
assert((delta_custom(0)) == 0) by (compute);assert((delta_custom(0)) == 0); // setup.hook.MyCustom
assert((case(0).world.compiled) == 1) by (compute);assert((case(0).world.compiled) == 1); // setup.value
assert((position(case(0).starts, 1)) == 1) by (compute);assert((position(case(0).starts, 1)) == 1); // setup.start.Initialize
assert((position(case(0).starts, 2)) == 2) by (compute);assert((position(case(0).starts, 2)) == 2); // setup.start.Reload
assert((position(case(0).starts, 4)) == 3) by (compute);assert((position(case(0).starts, 4)) == 3); // setup.start.Validate
assert((position(case(0).starts, 6)) == 4) by (compute);assert((position(case(0).starts, 6)) == 4); // setup.start.Compile
assert((position(case(0).starts, 7)) == 5) by (compute);assert((position(case(0).starts, 7)) == 5); // setup.start.Activate
assert((position(case(0).starts, 14)) == 6) by (compute);assert((position(case(0).starts, 14)) == 6); // setup.start.Configure
assert((position(case(0).starts, 100)) == 0) by (compute);assert((position(case(0).starts, 100)) == 0); // setup.start.MyCustom
assert((position(case(0).starts, 0)) == 0) by (compute);assert((position(case(0).starts, 0)) == 0); // setup.start.Clean
assert((position(case(0).starts, 5)) == 0) by (compute);assert((position(case(0).starts, 5)) == 0); // setup.start.ExportData
assert((position(case(0).starts, 8)) == 0) by (compute);assert((position(case(0).starts, 8)) == 0); // setup.start.Document
assert((position(case(0).starts, 9)) == 0) by (compute);assert((position(case(0).starts, 9)) == 0); // setup.start.MakeDeployed
assert((position(case(0).starts, 10)) == 0) by (compute);assert((position(case(0).starts, 10)) == 0); // setup.start.Test
assert((position(case(0).starts, 11)) == 0) by (compute);assert((position(case(0).starts, 11)) == 0); // setup.start.Package
assert((b(case(1).ok)) == 1) by (compute);assert((b(case(1).ok)) == 1); // c1.ok
assert((case(1).starts.len() as int) == 1) by (compute);assert((case(1).starts.len() as int) == 1); // c1.starts
assert((b(!case(1).ok)) == 0) by (compute);assert((b(!case(1).ok)) == 0); // c1.failures
assert((delta_compile(1)) == 1) by (compute);assert((delta_compile(1)) == 1); // c1.hook.Compile
assert((delta_activate(1)) == 0) by (compute);assert((delta_activate(1)) == 0); // c1.hook.Activate
assert((delta_custom(1)) == 0) by (compute);assert((delta_custom(1)) == 0); // c1.hook.MyCustom
assert((case(1).world.compiled) == 1) by (compute);assert((case(1).world.compiled) == 1); // c1.value
assert((position(case(1).starts, 1)) == 0) by (compute);assert((position(case(1).starts, 1)) == 0); // c1.start.Initialize
assert((position(case(1).starts, 2)) == 0) by (compute);assert((position(case(1).starts, 2)) == 0); // c1.start.Reload
assert((position(case(1).starts, 4)) == 0) by (compute);assert((position(case(1).starts, 4)) == 0); // c1.start.Validate
assert((position(case(1).starts, 6)) == 1) by (compute);assert((position(case(1).starts, 6)) == 1); // c1.start.Compile
assert((position(case(1).starts, 7)) == 0) by (compute);assert((position(case(1).starts, 7)) == 0); // c1.start.Activate
assert((position(case(1).starts, 14)) == 0) by (compute);assert((position(case(1).starts, 14)) == 0); // c1.start.Configure
assert((position(case(1).starts, 100)) == 0) by (compute);assert((position(case(1).starts, 100)) == 0); // c1.start.MyCustom
assert((position(case(1).starts, 0)) == 0) by (compute);assert((position(case(1).starts, 0)) == 0); // c1.start.Clean
assert((position(case(1).starts, 5)) == 0) by (compute);assert((position(case(1).starts, 5)) == 0); // c1.start.ExportData
assert((position(case(1).starts, 8)) == 0) by (compute);assert((position(case(1).starts, 8)) == 0); // c1.start.Document
assert((position(case(1).starts, 9)) == 0) by (compute);assert((position(case(1).starts, 9)) == 0); // c1.start.MakeDeployed
assert((position(case(1).starts, 10)) == 0) by (compute);assert((position(case(1).starts, 10)) == 0); // c1.start.Test
assert((position(case(1).starts, 11)) == 0) by (compute);assert((position(case(1).starts, 11)) == 0); // c1.start.Package
assert((b(case(2).ok)) == 1) by (compute);assert((b(case(2).ok)) == 1); // c2.ok
assert((case(2).starts.len() as int) == 4) by (compute);assert((case(2).starts.len() as int) == 4); // c2.starts
assert((b(!case(2).ok)) == 0) by (compute);assert((b(!case(2).ok)) == 0); // c2.failures
assert((delta_compile(2)) == 1) by (compute);assert((delta_compile(2)) == 1); // c2.hook.Compile
assert((delta_activate(2)) == 0) by (compute);assert((delta_activate(2)) == 0); // c2.hook.Activate
assert((delta_custom(2)) == 0) by (compute);assert((delta_custom(2)) == 0); // c2.hook.MyCustom
assert((case(2).world.compiled) == 12) by (compute);assert((case(2).world.compiled) == 12); // c2.value
assert((position(case(2).starts, 1)) == 1) by (compute);assert((position(case(2).starts, 1)) == 1); // c2.start.Initialize
assert((position(case(2).starts, 2)) == 2) by (compute);assert((position(case(2).starts, 2)) == 2); // c2.start.Reload
assert((position(case(2).starts, 4)) == 3) by (compute);assert((position(case(2).starts, 4)) == 3); // c2.start.Validate
assert((position(case(2).starts, 6)) == 4) by (compute);assert((position(case(2).starts, 6)) == 4); // c2.start.Compile
assert((position(case(2).starts, 7)) == 0) by (compute);assert((position(case(2).starts, 7)) == 0); // c2.start.Activate
assert((position(case(2).starts, 14)) == 0) by (compute);assert((position(case(2).starts, 14)) == 0); // c2.start.Configure
assert((position(case(2).starts, 100)) == 0) by (compute);assert((position(case(2).starts, 100)) == 0); // c2.start.MyCustom
assert((position(case(2).starts, 0)) == 0) by (compute);assert((position(case(2).starts, 0)) == 0); // c2.start.Clean
assert((position(case(2).starts, 5)) == 0) by (compute);assert((position(case(2).starts, 5)) == 0); // c2.start.ExportData
assert((position(case(2).starts, 8)) == 0) by (compute);assert((position(case(2).starts, 8)) == 0); // c2.start.Document
assert((position(case(2).starts, 9)) == 0) by (compute);assert((position(case(2).starts, 9)) == 0); // c2.start.MakeDeployed
assert((position(case(2).starts, 10)) == 0) by (compute);assert((position(case(2).starts, 10)) == 0); // c2.start.Test
assert((position(case(2).starts, 11)) == 0) by (compute);assert((position(case(2).starts, 11)) == 0); // c2.start.Package
assert((b(case(3).ok)) == 0) by (compute);assert((b(case(3).ok)) == 0); // c3.ok
assert((case(3).starts.len() as int) == 1) by (compute);assert((case(3).starts.len() as int) == 1); // c3.starts
assert((b(!case(3).ok)) == 1) by (compute);assert((b(!case(3).ok)) == 1); // c3.failures
assert((delta_compile(3)) == 0) by (compute);assert((delta_compile(3)) == 0); // c3.hook.Compile
assert((delta_activate(3)) == 0) by (compute);assert((delta_activate(3)) == 0); // c3.hook.Activate
assert((delta_custom(3)) == 0) by (compute);assert((delta_custom(3)) == 0); // c3.hook.MyCustom
assert((case(3).world.compiled) == 12) by (compute);assert((case(3).world.compiled) == 12); // c3.value
assert((position(case(3).starts, 1)) == 0) by (compute);assert((position(case(3).starts, 1)) == 0); // c3.start.Initialize
assert((position(case(3).starts, 2)) == 0) by (compute);assert((position(case(3).starts, 2)) == 0); // c3.start.Reload
assert((position(case(3).starts, 4)) == 0) by (compute);assert((position(case(3).starts, 4)) == 0); // c3.start.Validate
assert((position(case(3).starts, 6)) == 0) by (compute);assert((position(case(3).starts, 6)) == 0); // c3.start.Compile
assert((position(case(3).starts, 7)) == 0) by (compute);assert((position(case(3).starts, 7)) == 0); // c3.start.Activate
assert((position(case(3).starts, 14)) == 0) by (compute);assert((position(case(3).starts, 14)) == 0); // c3.start.Configure
assert((position(case(3).starts, 100)) == 0) by (compute);assert((position(case(3).starts, 100)) == 0); // c3.start.MyCustom
assert((position(case(3).starts, 0)) == 0) by (compute);assert((position(case(3).starts, 0)) == 0); // c3.start.Clean
assert((position(case(3).starts, 5)) == 0) by (compute);assert((position(case(3).starts, 5)) == 0); // c3.start.ExportData
assert((position(case(3).starts, 8)) == 0) by (compute);assert((position(case(3).starts, 8)) == 0); // c3.start.Document
assert((position(case(3).starts, 9)) == 0) by (compute);assert((position(case(3).starts, 9)) == 0); // c3.start.MakeDeployed
assert((position(case(3).starts, 10)) == 0) by (compute);assert((position(case(3).starts, 10)) == 0); // c3.start.Test
assert((position(case(3).starts, 11)) == 0) by (compute);assert((position(case(3).starts, 11)) == 0); // c3.start.Package
assert((position(case(3).starts, INVALID())) == 1) by (compute);assert((position(case(3).starts, INVALID())) == 1); // c3.start.Compile+Activate
assert((b(case(4).ok)) == 1) by (compute);assert((b(case(4).ok)) == 1); // c4.ok
assert((case(4).starts.len() as int) == 2) by (compute);assert((case(4).starts.len() as int) == 2); // c4.starts
assert((b(!case(4).ok)) == 0) by (compute);assert((b(!case(4).ok)) == 0); // c4.failures
assert((delta_compile(4)) == 0) by (compute);assert((delta_compile(4)) == 0); // c4.hook.Compile
assert((delta_activate(4)) == 0) by (compute);assert((delta_activate(4)) == 0); // c4.hook.Activate
assert((delta_custom(4)) == 0) by (compute);assert((delta_custom(4)) == 0); // c4.hook.MyCustom
assert((case(4).world.compiled) == 12) by (compute);assert((case(4).world.compiled) == 12); // c4.value
assert((position(case(4).starts, 1)) == 1) by (compute);assert((position(case(4).starts, 1)) == 1); // c4.start.Initialize
assert((position(case(4).starts, 2)) == 2) by (compute);assert((position(case(4).starts, 2)) == 2); // c4.start.Reload
assert((position(case(4).starts, 4)) == 0) by (compute);assert((position(case(4).starts, 4)) == 0); // c4.start.Validate
assert((position(case(4).starts, 6)) == 0) by (compute);assert((position(case(4).starts, 6)) == 0); // c4.start.Compile
assert((position(case(4).starts, 7)) == 0) by (compute);assert((position(case(4).starts, 7)) == 0); // c4.start.Activate
assert((position(case(4).starts, 14)) == 0) by (compute);assert((position(case(4).starts, 14)) == 0); // c4.start.Configure
assert((position(case(4).starts, 100)) == 0) by (compute);assert((position(case(4).starts, 100)) == 0); // c4.start.MyCustom
assert((position(case(4).starts, 0)) == 0) by (compute);assert((position(case(4).starts, 0)) == 0); // c4.start.Clean
assert((position(case(4).starts, 5)) == 0) by (compute);assert((position(case(4).starts, 5)) == 0); // c4.start.ExportData
assert((position(case(4).starts, 8)) == 0) by (compute);assert((position(case(4).starts, 8)) == 0); // c4.start.Document
assert((position(case(4).starts, 9)) == 0) by (compute);assert((position(case(4).starts, 9)) == 0); // c4.start.MakeDeployed
assert((position(case(4).starts, 10)) == 0) by (compute);assert((position(case(4).starts, 10)) == 0); // c4.start.Test
assert((position(case(4).starts, 11)) == 0) by (compute);assert((position(case(4).starts, 11)) == 0); // c4.start.Package
assert((b(case(5).ok)) == 1) by (compute);assert((b(case(5).ok)) == 1); // c5.ok
assert((case(5).starts.len() as int) == 1) by (compute);assert((case(5).starts.len() as int) == 1); // c5.starts
assert((b(!case(5).ok)) == 0) by (compute);assert((b(!case(5).ok)) == 0); // c5.failures
assert((delta_compile(5)) == 0) by (compute);assert((delta_compile(5)) == 0); // c5.hook.Compile
assert((delta_activate(5)) == 0) by (compute);assert((delta_activate(5)) == 0); // c5.hook.Activate
assert((delta_custom(5)) == 1) by (compute);assert((delta_custom(5)) == 1); // c5.hook.MyCustom
assert((case(5).world.compiled) == 12) by (compute);assert((case(5).world.compiled) == 12); // c5.value
assert((position(case(5).starts, 1)) == 0) by (compute);assert((position(case(5).starts, 1)) == 0); // c5.start.Initialize
assert((position(case(5).starts, 2)) == 0) by (compute);assert((position(case(5).starts, 2)) == 0); // c5.start.Reload
assert((position(case(5).starts, 4)) == 0) by (compute);assert((position(case(5).starts, 4)) == 0); // c5.start.Validate
assert((position(case(5).starts, 6)) == 0) by (compute);assert((position(case(5).starts, 6)) == 0); // c5.start.Compile
assert((position(case(5).starts, 7)) == 0) by (compute);assert((position(case(5).starts, 7)) == 0); // c5.start.Activate
assert((position(case(5).starts, 14)) == 0) by (compute);assert((position(case(5).starts, 14)) == 0); // c5.start.Configure
assert((position(case(5).starts, 100)) == 1) by (compute);assert((position(case(5).starts, 100)) == 1); // c5.start.MyCustom
assert((position(case(5).starts, 0)) == 0) by (compute);assert((position(case(5).starts, 0)) == 0); // c5.start.Clean
assert((position(case(5).starts, 5)) == 0) by (compute);assert((position(case(5).starts, 5)) == 0); // c5.start.ExportData
assert((position(case(5).starts, 8)) == 0) by (compute);assert((position(case(5).starts, 8)) == 0); // c5.start.Document
assert((position(case(5).starts, 9)) == 0) by (compute);assert((position(case(5).starts, 9)) == 0); // c5.start.MakeDeployed
assert((position(case(5).starts, 10)) == 0) by (compute);assert((position(case(5).starts, 10)) == 0); // c5.start.Test
assert((position(case(5).starts, 11)) == 0) by (compute);assert((position(case(5).starts, 11)) == 0); // c5.start.Package
assert((b(case(6).ok)) == 1) by (compute);assert((b(case(6).ok)) == 1); // c6.ok
assert((case(6).starts.len() as int) == 4) by (compute);assert((case(6).starts.len() as int) == 4); // c6.starts
assert((b(!case(6).ok)) == 0) by (compute);assert((b(!case(6).ok)) == 0); // c6.failures
assert((delta_compile(6)) == 1) by (compute);assert((delta_compile(6)) == 1); // c6.hook.Compile
assert((delta_activate(6)) == 0) by (compute);assert((delta_activate(6)) == 0); // c6.hook.Activate
assert((delta_custom(6)) == 0) by (compute);assert((delta_custom(6)) == 0); // c6.hook.MyCustom
assert((case(6).world.compiled) == 16) by (compute);assert((case(6).world.compiled) == 16); // c6.value
assert((position(case(6).starts, 1)) == 1) by (compute);assert((position(case(6).starts, 1)) == 1); // c6.start.Initialize
assert((position(case(6).starts, 2)) == 2) by (compute);assert((position(case(6).starts, 2)) == 2); // c6.start.Reload
assert((position(case(6).starts, 4)) == 3) by (compute);assert((position(case(6).starts, 4)) == 3); // c6.start.Validate
assert((position(case(6).starts, 6)) == 4) by (compute);assert((position(case(6).starts, 6)) == 4); // c6.start.Compile
assert((position(case(6).starts, 7)) == 0) by (compute);assert((position(case(6).starts, 7)) == 0); // c6.start.Activate
assert((position(case(6).starts, 14)) == 0) by (compute);assert((position(case(6).starts, 14)) == 0); // c6.start.Configure
assert((position(case(6).starts, 100)) == 0) by (compute);assert((position(case(6).starts, 100)) == 0); // c6.start.MyCustom
assert((position(case(6).starts, 0)) == 0) by (compute);assert((position(case(6).starts, 0)) == 0); // c6.start.Clean
assert((position(case(6).starts, 5)) == 0) by (compute);assert((position(case(6).starts, 5)) == 0); // c6.start.ExportData
assert((position(case(6).starts, 8)) == 0) by (compute);assert((position(case(6).starts, 8)) == 0); // c6.start.Document
assert((position(case(6).starts, 9)) == 0) by (compute);assert((position(case(6).starts, 9)) == 0); // c6.start.MakeDeployed
assert((position(case(6).starts, 10)) == 0) by (compute);assert((position(case(6).starts, 10)) == 0); // c6.start.Test
assert((position(case(6).starts, 11)) == 0) by (compute);assert((position(case(6).starts, 11)) == 0); // c6.start.Package
assert((b(case(7).ok)) == 0) by (compute);assert((b(case(7).ok)) == 0); // c7.ok
assert((case(7).starts.len() as int) == 1) by (compute);assert((case(7).starts.len() as int) == 1); // c7.starts
assert((b(!case(7).ok)) == 1) by (compute);assert((b(!case(7).ok)) == 1); // c7.failures
assert((delta_compile(7)) == 0) by (compute);assert((delta_compile(7)) == 0); // c7.hook.Compile
assert((delta_activate(7)) == 0) by (compute);assert((delta_activate(7)) == 0); // c7.hook.Activate
assert((delta_custom(7)) == 0) by (compute);assert((delta_custom(7)) == 0); // c7.hook.MyCustom
assert((case(7).world.compiled) == 16) by (compute);assert((case(7).world.compiled) == 16); // c7.value
assert((position(case(7).starts, 1)) == 0) by (compute);assert((position(case(7).starts, 1)) == 0); // c7.start.Initialize
assert((position(case(7).starts, 2)) == 0) by (compute);assert((position(case(7).starts, 2)) == 0); // c7.start.Reload
assert((position(case(7).starts, 4)) == 0) by (compute);assert((position(case(7).starts, 4)) == 0); // c7.start.Validate
assert((position(case(7).starts, 6)) == 0) by (compute);assert((position(case(7).starts, 6)) == 0); // c7.start.Compile
assert((position(case(7).starts, 7)) == 0) by (compute);assert((position(case(7).starts, 7)) == 0); // c7.start.Activate
assert((position(case(7).starts, 14)) == 0) by (compute);assert((position(case(7).starts, 14)) == 0); // c7.start.Configure
assert((position(case(7).starts, 100)) == 0) by (compute);assert((position(case(7).starts, 100)) == 0); // c7.start.MyCustom
assert((position(case(7).starts, 0)) == 0) by (compute);assert((position(case(7).starts, 0)) == 0); // c7.start.Clean
assert((position(case(7).starts, 5)) == 0) by (compute);assert((position(case(7).starts, 5)) == 0); // c7.start.ExportData
assert((position(case(7).starts, 8)) == 0) by (compute);assert((position(case(7).starts, 8)) == 0); // c7.start.Document
assert((position(case(7).starts, 9)) == 0) by (compute);assert((position(case(7).starts, 9)) == 0); // c7.start.MakeDeployed
assert((position(case(7).starts, 10)) == 0) by (compute);assert((position(case(7).starts, 10)) == 0); // c7.start.Test
assert((position(case(7).starts, 11)) == 0) by (compute);assert((position(case(7).starts, 11)) == 0); // c7.start.Package
assert((position(case(7).starts, INVALID())) == 1) by (compute);assert((position(case(7).starts, INVALID())) == 1); // c7.start.Bogus
assert((b(case(8).ok)) == 1) by (compute);assert((b(case(8).ok)) == 1); // c8.ok
assert((case(8).starts.len() as int) == 6) by (compute);assert((case(8).starts.len() as int) == 6); // c8.starts
assert((b(!case(8).ok)) == 0) by (compute);assert((b(!case(8).ok)) == 0); // c8.failures
assert((delta_compile(8)) == 1) by (compute);assert((delta_compile(8)) == 1); // c8.hook.Compile
assert((delta_activate(8)) == 1) by (compute);assert((delta_activate(8)) == 1); // c8.hook.Activate
assert((delta_custom(8)) == 0) by (compute);assert((delta_custom(8)) == 0); // c8.hook.MyCustom
assert((case(8).world.compiled) == 18) by (compute);assert((case(8).world.compiled) == 18); // c8.value
assert((position(case(8).starts, 1)) == 1) by (compute);assert((position(case(8).starts, 1)) == 1); // c8.start.Initialize
assert((position(case(8).starts, 2)) == 2) by (compute);assert((position(case(8).starts, 2)) == 2); // c8.start.Reload
assert((position(case(8).starts, 4)) == 3) by (compute);assert((position(case(8).starts, 4)) == 3); // c8.start.Validate
assert((position(case(8).starts, 6)) == 4) by (compute);assert((position(case(8).starts, 6)) == 4); // c8.start.Compile
assert((position(case(8).starts, 7)) == 5) by (compute);assert((position(case(8).starts, 7)) == 5); // c8.start.Activate
assert((position(case(8).starts, 14)) == 6) by (compute);assert((position(case(8).starts, 14)) == 6); // c8.start.Configure
assert((position(case(8).starts, 100)) == 0) by (compute);assert((position(case(8).starts, 100)) == 0); // c8.start.MyCustom
assert((position(case(8).starts, 0)) == 0) by (compute);assert((position(case(8).starts, 0)) == 0); // c8.start.Clean
assert((position(case(8).starts, 5)) == 0) by (compute);assert((position(case(8).starts, 5)) == 0); // c8.start.ExportData
assert((position(case(8).starts, 8)) == 0) by (compute);assert((position(case(8).starts, 8)) == 0); // c8.start.Document
assert((position(case(8).starts, 9)) == 0) by (compute);assert((position(case(8).starts, 9)) == 0); // c8.start.MakeDeployed
assert((position(case(8).starts, 10)) == 0) by (compute);assert((position(case(8).starts, 10)) == 0); // c8.start.Test
assert((position(case(8).starts, 11)) == 0) by (compute);assert((position(case(8).starts, 11)) == 0); // c8.start.Package
assert((b(case(9).ok)) == 1) by (compute);assert((b(case(9).ok)) == 1); // c9.ok
assert((case(9).starts.len() as int) == 1) by (compute);assert((case(9).starts.len() as int) == 1); // c9.starts
assert((b(!case(9).ok)) == 0) by (compute);assert((b(!case(9).ok)) == 0); // c9.failures
assert((delta_compile(9)) == 1) by (compute);assert((delta_compile(9)) == 1); // c9.hook.Compile
assert((delta_activate(9)) == 0) by (compute);assert((delta_activate(9)) == 0); // c9.hook.Activate
assert((delta_custom(9)) == 0) by (compute);assert((delta_custom(9)) == 0); // c9.hook.MyCustom
assert((case(9).world.compiled) == 18) by (compute);assert((case(9).world.compiled) == 18); // c9.value
assert((position(case(9).starts, 1)) == 0) by (compute);assert((position(case(9).starts, 1)) == 0); // c9.start.Initialize
assert((position(case(9).starts, 2)) == 0) by (compute);assert((position(case(9).starts, 2)) == 0); // c9.start.Reload
assert((position(case(9).starts, 4)) == 0) by (compute);assert((position(case(9).starts, 4)) == 0); // c9.start.Validate
assert((position(case(9).starts, 6)) == 1) by (compute);assert((position(case(9).starts, 6)) == 1); // c9.start.Compile
assert((position(case(9).starts, 7)) == 0) by (compute);assert((position(case(9).starts, 7)) == 0); // c9.start.Activate
assert((position(case(9).starts, 14)) == 0) by (compute);assert((position(case(9).starts, 14)) == 0); // c9.start.Configure
assert((position(case(9).starts, 100)) == 0) by (compute);assert((position(case(9).starts, 100)) == 0); // c9.start.MyCustom
assert((position(case(9).starts, 0)) == 0) by (compute);assert((position(case(9).starts, 0)) == 0); // c9.start.Clean
assert((position(case(9).starts, 5)) == 0) by (compute);assert((position(case(9).starts, 5)) == 0); // c9.start.ExportData
assert((position(case(9).starts, 8)) == 0) by (compute);assert((position(case(9).starts, 8)) == 0); // c9.start.Document
assert((position(case(9).starts, 9)) == 0) by (compute);assert((position(case(9).starts, 9)) == 0); // c9.start.MakeDeployed
assert((position(case(9).starts, 10)) == 0) by (compute);assert((position(case(9).starts, 10)) == 0); // c9.start.Test
assert((position(case(9).starts, 11)) == 0) by (compute);assert((position(case(9).starts, 11)) == 0); // c9.start.Package
assert((b(case(10).ok)) == 0) by (compute);assert((b(case(10).ok)) == 0); // c10.ok
assert((case(10).starts.len() as int) == 1) by (compute);assert((case(10).starts.len() as int) == 1); // c10.starts
assert((b(!case(10).ok)) == 1) by (compute);assert((b(!case(10).ok)) == 1); // c10.failures
assert((delta_compile(10)) == 0) by (compute);assert((delta_compile(10)) == 0); // c10.hook.Compile
assert((delta_activate(10)) == 0) by (compute);assert((delta_activate(10)) == 0); // c10.hook.Activate
assert((delta_custom(10)) == 0) by (compute);assert((delta_custom(10)) == 0); // c10.hook.MyCustom
assert((case(10).world.compiled) == 18) by (compute);assert((case(10).world.compiled) == 18); // c10.value
assert((position(case(10).starts, 1)) == 0) by (compute);assert((position(case(10).starts, 1)) == 0); // c10.start.Initialize
assert((position(case(10).starts, 2)) == 0) by (compute);assert((position(case(10).starts, 2)) == 0); // c10.start.Reload
assert((position(case(10).starts, 4)) == 0) by (compute);assert((position(case(10).starts, 4)) == 0); // c10.start.Validate
assert((position(case(10).starts, 6)) == 0) by (compute);assert((position(case(10).starts, 6)) == 0); // c10.start.Compile
assert((position(case(10).starts, 7)) == 0) by (compute);assert((position(case(10).starts, 7)) == 0); // c10.start.Activate
assert((position(case(10).starts, 14)) == 0) by (compute);assert((position(case(10).starts, 14)) == 0); // c10.start.Configure
assert((position(case(10).starts, 100)) == 0) by (compute);assert((position(case(10).starts, 100)) == 0); // c10.start.MyCustom
assert((position(case(10).starts, 0)) == 0) by (compute);assert((position(case(10).starts, 0)) == 0); // c10.start.Clean
assert((position(case(10).starts, 5)) == 0) by (compute);assert((position(case(10).starts, 5)) == 0); // c10.start.ExportData
assert((position(case(10).starts, 8)) == 0) by (compute);assert((position(case(10).starts, 8)) == 0); // c10.start.Document
assert((position(case(10).starts, 9)) == 0) by (compute);assert((position(case(10).starts, 9)) == 0); // c10.start.MakeDeployed
assert((position(case(10).starts, 10)) == 0) by (compute);assert((position(case(10).starts, 10)) == 0); // c10.start.Test
assert((position(case(10).starts, 11)) == 0) by (compute);assert((position(case(10).starts, 11)) == 0); // c10.start.Package
assert((position(case(10).starts, INVALID())) == 1) by (compute);assert((position(case(10).starts, INVALID())) == 1); // c10.start.Mycustom+Compile
assert((b(case(11).ok)) == 1) by (compute);assert((b(case(11).ok)) == 1); // c11.ok
assert((case(11).starts.len() as int) == 2) by (compute);assert((case(11).starts.len() as int) == 2); // c11.starts
assert((b(!case(11).ok)) == 0) by (compute);assert((b(!case(11).ok)) == 0); // c11.failures
assert((delta_compile(11)) == 0) by (compute);assert((delta_compile(11)) == 0); // c11.hook.Compile
assert((delta_activate(11)) == 1) by (compute);assert((delta_activate(11)) == 1); // c11.hook.Activate
assert((delta_custom(11)) == 0) by (compute);assert((delta_custom(11)) == 0); // c11.hook.MyCustom
assert((case(11).world.compiled) == 18) by (compute);assert((case(11).world.compiled) == 18); // c11.value
assert((position(case(11).starts, 1)) == 0) by (compute);assert((position(case(11).starts, 1)) == 0); // c11.start.Initialize
assert((position(case(11).starts, 2)) == 0) by (compute);assert((position(case(11).starts, 2)) == 0); // c11.start.Reload
assert((position(case(11).starts, 4)) == 0) by (compute);assert((position(case(11).starts, 4)) == 0); // c11.start.Validate
assert((position(case(11).starts, 6)) == 0) by (compute);assert((position(case(11).starts, 6)) == 0); // c11.start.Compile
assert((position(case(11).starts, 7)) == 1) by (compute);assert((position(case(11).starts, 7)) == 1); // c11.start.Activate
assert((position(case(11).starts, 14)) == 2) by (compute);assert((position(case(11).starts, 14)) == 2); // c11.start.Configure
assert((position(case(11).starts, 100)) == 0) by (compute);assert((position(case(11).starts, 100)) == 0); // c11.start.MyCustom
assert((position(case(11).starts, 0)) == 0) by (compute);assert((position(case(11).starts, 0)) == 0); // c11.start.Clean
assert((position(case(11).starts, 5)) == 0) by (compute);assert((position(case(11).starts, 5)) == 0); // c11.start.ExportData
assert((position(case(11).starts, 8)) == 0) by (compute);assert((position(case(11).starts, 8)) == 0); // c11.start.Document
assert((position(case(11).starts, 9)) == 0) by (compute);assert((position(case(11).starts, 9)) == 0); // c11.start.MakeDeployed
assert((position(case(11).starts, 10)) == 0) by (compute);assert((position(case(11).starts, 10)) == 0); // c11.start.Test
assert((position(case(11).starts, 11)) == 0) by (compute);assert((position(case(11).starts, 11)) == 0); // c11.start.Package
}
}
fn main() {}
