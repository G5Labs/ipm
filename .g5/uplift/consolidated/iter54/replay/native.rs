#[path="../../../../Verus_verification/consolidated/iter54/helpcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert(((if help(Name::Structural).warning { 1int } else { 0int })) == 0);assert(((if help(Name::Structural).warning { 1int } else { 0int })) == 0); // h5.warning
assert(((if shows_only(COMPILE()) { 1int } else { 0int })) == 1);assert(((if shows_only(COMPILE()) { 1int } else { 0int })) == 1); // h6.only
assert(((if intro_current_registry(1) { 1int } else { 0int })) == 1);assert(((if intro_current_registry(1) { 1int } else { 0int })) == 1); // i3.currentregistry
assert(((if help(Name::Empty).warning { 1int } else { 0int })) == 0);assert(((if help(Name::Empty).warning { 1int } else { 0int })) == 0); // h1.warning
assert(((if help(Name::Empty).listing { 1int } else { 0int })) == 1);assert(((if help(Name::Empty).listing { 1int } else { 0int })) == 1); // h1.available
assert((listed(help(Name::Empty))) == 39);assert((listed(help(Name::Empty))) == 39); // h1.listed
assert(((if help(Name::Cmd(INSTALL())).warning { 1int } else { 0int })) == 0);assert(((if help(Name::Cmd(INSTALL())).warning { 1int } else { 0int })) == 0); // h2.warning
assert(((if help(Name::Cmd(INSTALL())).listing { 1int } else { 0int })) == 0);assert(((if help(Name::Cmd(INSTALL())).listing { 1int } else { 0int })) == 0); // h2.available
assert((listed(help(Name::Cmd(INSTALL())))) == 1);assert((listed(help(Name::Cmd(INSTALL())))) == 1); // h2.listed
assert(((if help(Name::Alias(VERSION())).warning { 1int } else { 0int })) == 0);assert(((if help(Name::Alias(VERSION())).warning { 1int } else { 0int })) == 0); // h3.warning
assert(((if help(Name::Alias(VERSION())).listing { 1int } else { 0int })) == 0);assert(((if help(Name::Alias(VERSION())).listing { 1int } else { 0int })) == 0); // h3.available
assert((listed(help(Name::Alias(VERSION())))) == 1);assert((listed(help(Name::Alias(VERSION())))) == 1); // h3.listed
assert(((if help(Name::Unknown).warning { 1int } else { 0int })) == 1);assert(((if help(Name::Unknown).warning { 1int } else { 0int })) == 1); // h4.warning
assert(((if help(Name::Unknown).listing { 1int } else { 0int })) == 1);assert(((if help(Name::Unknown).listing { 1int } else { 0int })) == 1); // h4.available
assert((listed(help(Name::Unknown))) == 39);assert((listed(help(Name::Unknown))) == 39); // h4.listed
assert(((if help(Name::Structural).listing { 1int } else { 0int })) == 1);assert(((if help(Name::Structural).listing { 1int } else { 0int })) == 1); // h5.available
assert((listed(help(Name::Structural))) == 39);assert((listed(help(Name::Structural))) == 39); // h5.listed
assert(((if help(Name::Cmd(COMPILE())).warning { 1int } else { 0int })) == 0);assert(((if help(Name::Cmd(COMPILE())).warning { 1int } else { 0int })) == 0); // h6.warning
assert(((if help(Name::Cmd(COMPILE())).listing { 1int } else { 0int })) == 0);assert(((if help(Name::Cmd(COMPILE())).listing { 1int } else { 0int })) == 0); // h6.available
assert((listed(help(Name::Cmd(COMPILE())))) == 1);assert((listed(help(Name::Cmd(COMPILE())))) == 1); // h6.listed
assert(((if help(Name::Unknown).warning { 1int } else { 0int })) == 1);assert(((if help(Name::Unknown).warning { 1int } else { 0int })) == 1); // h8.warning
assert(((if help(Name::Unknown).listing { 1int } else { 0int })) == 1);assert(((if help(Name::Unknown).listing { 1int } else { 0int })) == 1); // h8.available
assert((listed(help(Name::Unknown))) == 39);assert((listed(help(Name::Unknown))) == 39); // h8.listed
assert(((if help(Name::Cmd(MODULE_ACTION())).warning { 1int } else { 0int })) == 0);assert(((if help(Name::Cmd(MODULE_ACTION())).warning { 1int } else { 0int })) == 0); // h9.warning
assert(((if help(Name::Cmd(MODULE_ACTION())).listing { 1int } else { 0int })) == 0);assert(((if help(Name::Cmd(MODULE_ACTION())).listing { 1int } else { 0int })) == 0); // h9.available
assert((listed(help(Name::Cmd(MODULE_ACTION())))) == 1);assert((listed(help(Name::Cmd(MODULE_ACTION())))) == 1); // h9.listed
assert(((if shows_only(INSTALL()) { 1int } else { 0int })) == 0);assert(((if shows_only(INSTALL()) { 1int } else { 0int })) == 0); // h2.only
assert(((if shows_dev(INSTALL()) { 1int } else { 0int })) == 1);assert(((if shows_dev(INSTALL()) { 1int } else { 0int })) == 1); // h2.dev
assert(((if shows_only(MODULE_ACTION()) { 1int } else { 0int })) == 1);assert(((if shows_only(MODULE_ACTION()) { 1int } else { 0int })) == 1); // h9.only
assert((WITH_DESCRIPTION()) == 38);assert((WITH_DESCRIPTION()) == 38); // h7.descriptions
assert((COMMANDS()) == 39);assert((COMMANDS()) == 39); // h7.listed
assert(((if intro_current_registry(0) { 1int } else { 0int })) == 0);assert(((if intro_current_registry(0) { 1int } else { 0int })) == 0); // i1.currentregistry
assert(((if intro_current_registry(0) { 1int } else { 0int })) == 0);assert(((if intro_current_registry(0) { 1int } else { 0int })) == 0); // i2.currentregistry
assert(((if !intro_current_registry(0) { 1int } else { 0int })) == 1);assert(((if !intro_current_registry(0) { 1int } else { 0int })) == 1); // i1.noregistry
assert(((if !intro_current_registry(1) { 1int } else { 0int })) == 0);assert(((if !intro_current_registry(1) { 1int } else { 0int })) == 0); // i3.noregistry
}
}
fn main() {}
