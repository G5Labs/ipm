#![allow(non_snake_case)]
// Iteration46: the `show <module>` summary.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter46/bindings.json):
//   General/ModuleSummary.cls::GetLines              fields, usage, footprint, dependency, dependent and requirement blocks
//   ::GetSystemRequirements                           one entry per set requirement; Health only when true
//   ::GetSection, ::GetShortName                      scoped and test resources hidden; default processors -> Code
//   ::AddList, ::AddTable, ::AddBlock                 empty lists and empty blocks print nothing
//   ::SplitLines, ::Join                              after-install message lines trimmed, blanks dropped
//
// Inputs abstract the stored manifest; outputs are the observed presence facts. Premises: the stored
// module record as loaded (XML import), SQL for dependents, and terminal capture.
use vstd::prelude::*;

verus! {

pub struct Req { pub version: bool, pub interop: bool, pub health: Option<bool> }
pub struct M {
    pub desc: bool, pub after_lines: nat, pub defaults: nat, pub parameters: nat,
    pub code: nat, pub scoped: nat, pub files: nat, pub deps: nat, pub dependents: nat,
    pub req: Option<Req>, pub keywords: bool,
}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub open spec fn health_entry(r: Req) -> bool { r.health == Some(true) }
pub open spec fn req_entries(r: Option<Req>) -> nat {
    match r {
        None => 0,
        Some(x) => (if x.version { 1nat } else { 0nat }) + (if x.interop { 1nat } else { 0nat }) + (if health_entry(x) { 1nat } else { 0nat }),
    }
}

// Blocks: usage (after-install lines or <Default> entries), footprint (shown resources), depends on,
// required by, system requirements. <Parameter> settings never enter the Defaults list.
pub open spec fn defaults_shown(m: M) -> nat { m.defaults }
pub open spec fn usage(m: M) -> bool { m.after_lines > 0 || defaults_shown(m) > 0 }
pub open spec fn shown_code(m: M) -> nat { m.code }
pub open spec fn footprint(m: M) -> bool { shown_code(m) > 0 || m.files > 0 }
pub open spec fn sysreq(m: M) -> bool { m.req.is_some() }
pub open spec fn blocks(m: M) -> int { b(usage(m)) + b(footprint(m)) + b(m.deps > 0) + b(m.dependents > 0) + b(sysreq(m)) }
pub open spec fn keywords_shown(m: M) -> bool { false }

pub proof fn health_false_hidden(m: M, r: Req)
    requires m.req == Some(r), r.health == Some(false), !r.version, !r.interop
    ensures !sysreq(m)
{}
pub proof fn parameter_not_listed(m: M)
    requires m.defaults == 0, m.after_lines == 0, m.parameters > 0
    ensures !usage(m)
{}
pub proof fn empty_blocks_omitted(m: M)
    requires !m.desc, m.after_lines == 0, m.defaults == 0, m.deps == 0, m.dependents == 0, m.req.is_none(), m.code > 0, m.files == 0
    ensures blocks(m) == 1
{}

pub proof fn scoped_hidden(m: M)
    requires m.scoped > 0
    ensures shown_code(m) == m.code
{}

// Fixture modules.
pub open spec fn a() -> M { M { desc: true, after_lines: 2, defaults: 1, parameters: 1, code: 2, scoped: 1, files: 1, deps: 1, dependents: 0,
    req: Some(Req { version: true, interop: true, health: Some(false) }), keywords: true } }
pub open spec fn bmod() -> M { M { desc: false, after_lines: 0, defaults: 0, parameters: 0, code: 1, scoped: 0, files: 0, deps: 0, dependents: 1, req: None, keywords: false } }
pub open spec fn d() -> M { M { desc: false, after_lines: 0, defaults: 0, parameters: 0, code: 1, scoped: 0, files: 0, deps: 0, dependents: 0,
    req: Some(Req { version: false, interop: false, health: Some(false) }), keywords: false } }
pub open spec fn h() -> M { M { desc: false, after_lines: 0, defaults: 0, parameters: 0, code: 1, scoped: 0, files: 0, deps: 0, dependents: 0, req: None, keywords: false } }

pub proof fn schedule()
    ensures blocks(a()) == 4, blocks(bmod()) == 2, blocks(d()) == 1, blocks(h()) == 1,
        !sysreq(d()), sysreq(a()), req_entries(a().req) == 2, usage(a()), !keywords_shown(a()), shown_code(a()) == 2
{}

} // verus!

fn main() {}
