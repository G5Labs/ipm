use vstd::prelude::*;
#[path="../script.rs"] pub mod model;
use model::*;
verus!{pub proof fn observed_group(){
assert(first()!=last() && first()!=unknown() && last()!=unknown() && fault()!=first() && fault()!=last() && fault()!=unknown()) by(compute);text_preparation();json_preparation();shape_neighbor();execution_neighbor();dry_execution_neighbor();runtime_neighbor();named_neighbors();reveal_with_fuel(execute,4);
// txt-valid-ok

assert((b(ok(named(Case::TxtValid,false))))==1);
// txt-unknown-ok

assert((b(ok(named(Case::TxtUnknown,false))))==0);
// txt-dry-unknown-ok

assert((b(ok(named(Case::TxtUnknown,true))))==1);
// txt-valid-value

assert((setting_value((named(Case::TxtValid,false)).world) as int)==202);
// txt-valid-raw-value

assert((raw_value((named(Case::TxtValid,false)).world) as int)==202);
// txt-valid-runtime-calls

assert(((named(Case::TxtValid,false)).world.calls as int)==0);
// txt-unknown-value

assert((setting_value((named(Case::TxtUnknown,false)).world) as int)==101);
// txt-unknown-raw-value

assert((raw_value((named(Case::TxtUnknown,false)).world) as int)==101);
// txt-unknown-runtime-calls

assert(((named(Case::TxtUnknown,false)).world.calls as int)==0);
// txt-dry-unknown-value

assert((setting_value((named(Case::TxtUnknown,true)).world) as int)==7);
// txt-dry-unknown-raw-value

assert((raw_value((named(Case::TxtUnknown,true)).world) as int)==7);
// txt-dry-unknown-runtime-calls

assert(((named(Case::TxtUnknown,true)).world.calls as int)==0);
// txt-runtime-ok

assert((b(ok(named(Case::TxtRuntime,false))))==0);
// txt-runtime-value

assert((setting_value((named(Case::TxtRuntime,false)).world) as int)==101);
// txt-runtime-raw-value

assert((raw_value((named(Case::TxtRuntime,false)).world) as int)==101);
// txt-runtime-runtime-calls

assert(((named(Case::TxtRuntime,false)).world.calls as int)==1);
// json-valid-ok

assert((b(ok(named(Case::JsonValid,false))))==1);
// json-valid-value

assert((setting_value((named(Case::JsonValid,false)).world) as int)==202);
// json-valid-raw-value

assert((raw_value((named(Case::JsonValid,false)).world) as int)==202);
// json-valid-runtime-calls

assert(((named(Case::JsonValid,false)).world.calls as int)==0);
// json-unknown-ok

assert((b(ok(named(Case::JsonUnknown,false))))==0);
// json-unknown-value

assert((setting_value((named(Case::JsonUnknown,false)).world) as int)==7);
// json-unknown-raw-value

assert((raw_value((named(Case::JsonUnknown,false)).world) as int)==7);
// json-unknown-runtime-calls

assert(((named(Case::JsonUnknown,false)).world.calls as int)==0);
// json-runtime-ok

assert((b(ok(named(Case::JsonRuntime,false))))==0);
// json-runtime-value

assert((setting_value((named(Case::JsonRuntime,false)).world) as int)==101);
// json-runtime-raw-value

assert((raw_value((named(Case::JsonRuntime,false)).world) as int)==101);
// json-runtime-runtime-calls

assert(((named(Case::JsonRuntime,false)).world.calls as int)==1);
// json-dry-valid-ok

assert((b(ok(named(Case::JsonValid,true))))==1);
// json-dry-valid-value

assert((setting_value((named(Case::JsonValid,true)).world) as int)==7);
// json-dry-valid-raw-value

assert((raw_value((named(Case::JsonValid,true)).world) as int)==7);
// json-dry-valid-runtime-calls

assert(((named(Case::JsonValid,true)).world.calls as int)==0);
// json-dry-unknown-ok

assert((b(ok(named(Case::JsonUnknown,true))))==0);
// json-dry-unknown-value

assert((setting_value((named(Case::JsonUnknown,true)).world) as int)==7);
// json-dry-unknown-raw-value

assert((raw_value((named(Case::JsonUnknown,true)).world) as int)==7);
// json-dry-unknown-runtime-calls

assert(((named(Case::JsonUnknown,true)).world.calls as int)==0);
// json-array-ok

assert((b(ok(named(Case::JsonArray,false))))==0);
// json-array-value

assert((setting_value((named(Case::JsonArray,false)).world) as int)==7);
// json-array-raw-value

assert((raw_value((named(Case::JsonArray,false)).world) as int)==7);
// json-array-runtime-calls

assert(((named(Case::JsonArray,false)).world.calls as int)==0);
// json-wrongcommands-ok

assert((b(ok(named(Case::JsonWrong,false))))==0);
// json-wrongcommands-value

assert((setting_value((named(Case::JsonWrong,false)).world) as int)==7);
// json-wrongcommands-raw-value

assert((raw_value((named(Case::JsonWrong,false)).world) as int)==7);
// json-wrongcommands-runtime-calls

assert(((named(Case::JsonWrong,false)).world.calls as int)==0);
// missing-file-ok

assert((b(ok(named(Case::Missing,false))))==0);
// missing-file-value

assert((setting_value((named(Case::Missing,false)).world) as int)==7);
// missing-file-raw-value

assert((raw_value((named(Case::Missing,false)).world) as int)==7);
// missing-file-runtime-calls

assert(((named(Case::Missing,false)).world.calls as int)==0);
// unsupported-extension-ok

assert((b(ok(named(Case::Unsupported,false))))==0);
// unsupported-extension-value

assert((setting_value((named(Case::Unsupported,false)).world) as int)==7);
// unsupported-extension-raw-value

assert((raw_value((named(Case::Unsupported,false)).world) as int)==7);
// unsupported-extension-runtime-calls

assert(((named(Case::Unsupported,false)).world.calls as int)==0);
// txt-unknown-last-listed

assert((b(last_listed(named(Case::TxtUnknown,false))))==0);
// txt-dry-unknown-last-listed

assert((b(last_listed(named(Case::TxtUnknown,true))))==1);
// json-unknown-first-listed

assert((b(first_listed(named(Case::JsonUnknown,false))))==0);
// json-runtime-last-listed

assert((b(last_listed(named(Case::JsonRuntime,false))))==0);
// json-dry-valid-last-listed

assert((b(last_listed(named(Case::JsonValid,true))))==1);
}}
fn main(){}
