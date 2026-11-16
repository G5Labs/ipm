#![allow(non_snake_case)]
// Iteration65: developer-mode versus normal-mode refresh across resource kinds.
//
// Source roles at pin 571e04a1: Lifecycle/Base.cls::%Reload (root from the RootDirectory parameter, or the
// stored root only in developer mode), the phase chains of iteration30 (reload = Initialize..Reload,
// compile = ..Compile, load = ..Activate), General/Sync/Pipeline.cls::Run (developer mode required;
// resources whose processor lacks sync support are skipped and named only with -verbose), and the
// per-kind processors: Document/Default (source loaded on Reload), Global (data imported on Reload),
// DeepSeeItem (item loaded on Reload), compiled kinds observable after Compile, LocalizedMessages
// (messages generated at Compile), FileCopy and WebApplication (applied in Activate).
// Kinds: 0 CLS, 1 INC, 2 MAC, 3 GBL, 4 FileCopy, 5 WebApplication, 6 LOC, 7 DFI.
// Commands: 0 load <dir>, 1 <m> reload, 2 <m> compile, 3 sync <m>.
use vstd::prelude::*;

verus! {

pub open spec fn on_reload(k: int) -> bool { k == 1 || k == 3 || k == 7 }
pub open spec fn on_compile(k: int) -> bool { k == 0 || k == 2 || k == 6 }
pub open spec fn on_activate(k: int) -> bool { k == 4 || k == 5 }
pub open spec fn sync_support(k: int) -> bool { k == 0 || k == 1 || k == 2 || k == 6 || k == 7 }

// Does command c refresh kind k of a module in mode dev (it reads the edited disk value)?
pub open spec fn refreshes(k: int, dev: bool, c: int) -> bool {
    if c == 0 { true }
    else if c == 1 { on_reload(k) }
    else if c == 2 { dev && (on_reload(k) || on_compile(k)) }
    else if c == 3 { dev && sync_support(k) }
    else { false }
}
pub open spec fn command_ok(dev: bool, c: int) -> bool { c != 3 || dev }

// Native schedule: (command, disk value) per step; value observed after the step.
pub open spec fn step_cmd(s: int) -> int { if s <= 1 { 0 } else if s == 2 { 1 } else if s == 3 { 2 } else { 3 } }
pub open spec fn step_disk(s: int) -> int { if s == 0 { 1 } else if s == 1 { 2 } else if s <= 3 { 3 } else if s == 4 { 4 } else { 5 } }
pub open spec fn value(k: int, dev: bool, s: int) -> int
    decreases s
{
    if s <= 0 { 1 }
    else if refreshes(k, dev, step_cmd(s)) && command_ok(dev, step_cmd(s)) { step_disk(s) }
    else { value(k, dev, s - 1) }
}

pub proof fn normal_mode_only_load_refreshes(k: int)
    ensures !refreshes(k, false, 1), !refreshes(k, false, 2), !command_ok(false, 3), refreshes(k, false, 0)
{}
pub proof fn compile_chain_skips_activate(k: int)
    requires on_activate(k)
    ensures !refreshes(k, true, 2), refreshes(k, true, 0)
{}
pub proof fn sync_skips_unsupported()
    ensures !refreshes(3, true, 3), !refreshes(4, true, 3), !refreshes(5, true, 3), refreshes(0, true, 3), refreshes(7, true, 3)
{}
pub proof fn schedule()
    ensures value(1, true, 2) == 3, value(0, true, 2) == 2, value(0, true, 3) == 3, value(4, true, 3) == 2,
            value(3, true, 4) == 3, value(6, true, 4) == 4, value(1, false, 3) == 2, value(0, false, 4) == 2
{
    reveal_with_fuel(value, 6);
}

} // verus!

fn main() {}
