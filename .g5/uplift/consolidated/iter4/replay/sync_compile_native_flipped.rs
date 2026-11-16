//! Sync's dependent-recompile step after a resource cannot be resolved.
//! Owner: devtools.sync.compile-after-resolve in features/developer-tools.g5.md.
//! Source: General/Sync/Pipeline.cls::SyncCompile and its two callers in ::Run;
//! General/Sync/Summary.cls::AddWarning.
use vstd::prelude::*;
verus! {

pub struct Child { pub is_cls: bool, pub defined: bool }
pub struct SyncRes {
    pub compilable: bool,
    pub compile_scope: bool,
    pub from_project: bool,
    pub resolve_ok: bool,
    pub children: Seq<Child>,
}

pub open spec fn eligible(r: SyncRes) -> bool { r.compilable && r.compile_scope && !r.from_project }
pub open spec fn warned(r: SyncRes) -> bool { eligible(r) && !r.resolve_ok }

/// A .cls child joins the compile set only if the class is already defined; other children always join.
pub open spec fn kept(cs: Seq<Child>) -> nat
    decreases cs.len()
{
    if cs.len() == 0 { 0 } else {
        (if !cs.last().is_cls || cs.last().defined { 1nat } else { 0nat }) + kept(cs.drop_last())
    }
}

pub open spec fn contributed(r: SyncRes) -> nat {
    if eligible(r) && r.resolve_ok { kept(r.children) } else { 0 }
}

pub open spec fn compile_set_size(rs: Seq<SyncRes>) -> nat
    decreases rs.len()
{
    if rs.len() == 0 { 0 } else { contributed(rs.last()) + compile_set_size(rs.drop_last()) }
}

pub open spec fn warnings(rs: Seq<SyncRes>) -> nat
    decreases rs.len()
{
    if rs.len() == 0 { 0 } else { (if warned(rs.last()) { 1nat } else { 0nat }) + warnings(rs.drop_last()) }
}

pub struct CompileResult { pub ok: bool, pub compile_called: bool, pub printed: nat, pub retained: nat }

/// Resolution failure never fails the step; the step fails only if an actual compile fails.
/// Printed warnings are unconditional; the summary keeps them only during a whole-namespace sync.
pub open spec fn sync_compile(rs: Seq<SyncRes>, summary_active: bool, compile_ok: bool) -> CompileResult {
    let called = compile_set_size(rs) > 0;
    CompileResult {
        ok: !called || compile_ok,
        compile_called: called,
        printed: warnings(rs),
        retained: if summary_active { warnings(rs) } else { 0 },
    }
}

pub open spec fn pkg(resolve_ok: bool) -> SyncRes {
    SyncRes { compilable: true, compile_scope: true, from_project: false, resolve_ok,
              children: seq![Child { is_cls: true, defined: true }] }
}

pub proof fn sync_compile_neighbors() {
    reveal_with_fuel(compile_set_size, 3);
    reveal_with_fuel(warnings, 3);
    reveal_with_fuel(kept, 2);
    // A: the survivor is compiled although the other resource could not be resolved.
    let a = sync_compile(seq![pkg(true), pkg(false)], true, true);
    assert(a.ok && a.compile_called && a.printed == 1 && a.retained == 1);
    // B: nothing survives, so no compile is called and the step still succeeds.
    let b = sync_compile(seq![pkg(false)], false, false);
    assert(b.ok && !b.compile_called && b.printed == 1 && b.retained == 0);
    // An undefined class child is left out; a non-class child is not.
    let undefined = SyncRes { children: seq![Child { is_cls: true, defined: false }], ..pkg(true) };
    assert(!sync_compile(seq![undefined], false, false).compile_called);
    let routine = SyncRes { children: seq![Child { is_cls: false, defined: false }], ..pkg(true) };
    assert(sync_compile(seq![routine], false, true).compile_called);
    // A deployed project resource is neither compiled nor warned about.
    let project = SyncRes { from_project: true, ..pkg(false) };
    assert(sync_compile(seq![project], true, true).printed == 0);
}

pub proof fn native_replay() {
    reveal_with_fuel(compile_set_size, 3); reveal_with_fuel(warnings, 3); reveal_with_fuel(kept, 2);
    // Y1.ok
    
    assert((sync_compile(seq![pkg(true), pkg(false)], true, true).ok) == (false));
    // Y1.compile_called
    
    assert((sync_compile(seq![pkg(true), pkg(false)], true, true).compile_called) == (true));
    // Y1.retained
    
    assert((sync_compile(seq![pkg(true), pkg(false)], true, true).retained) == (1));
    // Y2.ok
    
    assert((sync_compile(seq![pkg(false)], false, false).ok) == (true));
    // Y2.compile_called
    
    assert((sync_compile(seq![pkg(false)], false, false).compile_called) == (false));
    // Y2.retained
    
    assert((sync_compile(seq![pkg(false)], false, false).retained) == (0));
    // Y.printed
    
    assert((sync_compile(seq![pkg(true), pkg(false)], true, true).printed + sync_compile(seq![pkg(false)], false, false).printed) == (2));
}

}
fn main() {}
