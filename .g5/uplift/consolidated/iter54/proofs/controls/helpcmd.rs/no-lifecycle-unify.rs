#![allow(non_snake_case)]
// Iteration54: `help [<command>]` rendering and the `version`/intro registry readers.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter54/bindings.json):
//   CLI.cls::%Help                     resolve name (command, then alias table at subscript 1); an unknown name warns
//                                      and falls back to the full listing; numeric names hit structural subscripts
//                                      (1 = aliases, -1 = max length), so they list everything without a warning
//   CLI.cls::UnifyLifecycles           phase shortcut commands and module-action show each other's modifiers
//   CLI.cls::%HelpForCommand, ...Details/Parameters/Modifiers/Examples   one-line summary vs detailed sections
//   Main.cls::Version                  IPM line per namespace with IPM, then one line per remote registry
//   Main.cls::GetVersion, BuildIntroMessages   intro shows "Current registry <url>" only when a remote registry exists
// The command table is the pinned Commands XDATA (39 commands, 38 with a <description>). Premise: the XDATA
// projection (%GetCommandStructure) matches the source text, checked natively by the listing count.
use vstd::prelude::*;

verus! {

pub enum Name { Empty, Cmd(int), Alias(int), Structural, Unknown }
pub struct View { pub warning: bool, pub listing: bool, pub detailed: Option<int> }

pub open spec fn help(n: Name) -> View {
    match n {
        Name::Empty => View { warning: false, listing: true, detailed: None },
        Name::Cmd(c) => View { warning: false, listing: false, detailed: Some(c) },
        Name::Alias(c) => View { warning: false, listing: false, detailed: Some(c) },
        Name::Structural => View { warning: false, listing: true, detailed: None },
        Name::Unknown => View { warning: true, listing: true, detailed: None },
    }
}

pub open spec fn COMMANDS() -> int { 39 }
pub open spec fn WITH_DESCRIPTION() -> int { 38 }
pub open spec fn COMPILE() -> int { 7 }
pub open spec fn MODULE_ACTION() -> int { 3 }
pub open spec fn INSTALL() -> int { 20 }
pub open spec fn VERSION() -> int { 31 }
pub open spec fn phase_shortcut(c: int) -> bool { c == COMPILE() }
// Modifiers shown in detailed help: own modifiers, plus module-action's for phase shortcuts.
pub open spec fn shows_only(c: int) -> bool { c == MODULE_ACTION() }
pub open spec fn shows_dev(c: int) -> bool { c == MODULE_ACTION() || phase_shortcut(c) || c == INSTALL() }

pub open spec fn listed(v: View) -> int { if v.listing { COMMANDS() } else if v.detailed.is_some() { 1 } else { 0 } }

// Intro registry line and version lines.
pub open spec fn intro_current_registry(remote_registries: nat) -> bool { remote_registries > 0 }
pub open spec fn version_lines(namespaces_with_ipm: nat, remote_registries: nat) -> nat { namespaces_with_ipm + remote_registries }

pub proof fn neighbors()
    ensures
        !help(Name::Cmd(INSTALL())).warning && help(Name::Cmd(INSTALL())).detailed == Some(INSTALL()),
        help(Name::Alias(VERSION())).detailed == Some(VERSION()),
        help(Name::Unknown).warning && help(Name::Unknown).listing,
        !help(Name::Structural).warning && help(Name::Structural).listing,
        listed(help(Name::Empty)) == 39,
        shows_only(COMPILE()) && !shows_only(INSTALL()) && shows_dev(INSTALL()),
        !intro_current_registry(0) && intro_current_registry(1),
        version_lines(1, 0) == 1,
{}

} // verus!

fn main() {}
