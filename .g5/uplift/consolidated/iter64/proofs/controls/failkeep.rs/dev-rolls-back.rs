#![allow(non_snake_case)]
// Iteration64: what survives a failed ordinary load, by resource kind, transaction mode and failure point.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter64/bindings.json): Utils/Module.cls::LoadNewModule
// (transaction around a normal load; developer mode commits the manifest early), Lifecycle/Base.cls::%Reload
// (declared mappings, ^Sources), ResourceProcessor/CPF.cls (merge before Initialize), ResourceProcessor/FileCopy.cls
// and LocalizationExport.cls (Activate-time file writes), ResourceProcessor/WebApplication.cls (Activate).
// Premises (native-qualified): Config.Map* and Security.Applications changes made inside the load transaction are
// undone by its rollback; `iris merge` and file writes are not; the compiler compiles the valid class before the
// invalid one fails.
use vstd::prelude::*;

verus! {

pub enum Kind { Record, Class, File, WebApp, Mapping, Cpf, Msg, LocDoc, LocExport, Sources }
// Failure point: 0 = success, 2 = Compile fails, 3 = after Activate fails.
pub open spec fn phase(k: Kind) -> int {
    match k { Kind::Cpf => 0, Kind::Mapping => 1, Kind::Sources => 1, Kind::Record => 1, Kind::LocDoc => 1,
              Kind::Class => 2, Kind::Msg => 2, Kind::File => 3, Kind::WebApp => 3, Kind::LocExport => 3 }
}
pub open spec fn external(k: Kind) -> bool { match k { Kind::Cpf => true, Kind::File => true, Kind::LocExport => true, _ => false } }
pub open spec fn happened(k: Kind, fail: int, dev: bool) -> bool {
    (match k { Kind::Sources => dev, _ => true }) && (fail == 0 || phase(k) <= fail)
}
pub open spec fn kept(k: Kind, fail: int, dev: bool) -> bool { happened(k, fail, dev) && (fail == 0 || external(k)) }
pub open spec fn hist_committed(fail: int, dev: bool) -> bool { fail == 0 || dev }

pub proof fn normal_compile_failure_keeps_only_cpf()
    ensures kept(Kind::Cpf, 2, false), !kept(Kind::Mapping, 2, false), !kept(Kind::Record, 2, false),
            !kept(Kind::File, 2, false), !kept(Kind::Class, 2, false), !hist_committed(2, false)
{}
pub proof fn late_failure_keeps_external_effects()
    ensures kept(Kind::File, 3, false), kept(Kind::LocExport, 3, false), kept(Kind::Cpf, 3, false),
            !kept(Kind::WebApp, 3, false), !kept(Kind::Mapping, 3, false), !kept(Kind::Msg, 3, false)
{}
pub proof fn developer_failure_keeps_database_effects()
    ensures kept(Kind::Record, 2, true), kept(Kind::Mapping, 2, true), kept(Kind::Sources, 2, true), kept(Kind::Msg, 2, true),
            kept(Kind::Class, 2, true), !kept(Kind::WebApp, 2, true), !kept(Kind::File, 2, true), hist_committed(2, true)
{}
pub proof fn success_keeps_all_but_nondev_sources()
    ensures kept(Kind::WebApp, 0, false), kept(Kind::Mapping, 0, false), !kept(Kind::Sources, 0, false)
{}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

} // verus!

fn main() {}
