#[path="../../../../Verus_verification/consolidated/iter36/installer.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((l1().calls[1].greeting) == 0) by (compute);assert((l1().calls[1].greeting) == 0); // l1.call2.greeting
assert((l3().calls[0].upgrade) == 0) by (compute);assert((l3().calls[0].upgrade) == 0); // l3.call1.upgrade
assert((e1().calls[0].kind) == 0) by (compute);assert((e1().calls[0].kind) == 0); // e1.call1.kind
assert((b(l1().ok)) == 1) by (compute);assert((b(l1().ok)) == 1); // l1.ok
assert((b(l1().installed)) == 1) by (compute);assert((b(l1().installed)) == 1); // l1.installed
assert((l1().calls.len() as int) == 3) by (compute);assert((l1().calls.len() as int) == 3); // l1.calls
assert((l1().calls[0].kind) == 1) by (compute);assert((l1().calls[0].kind) == 1); // l1.call1.kind
assert((l1().calls[0].db_empty) == 1) by (compute);assert((l1().calls[0].db_empty) == 1); // l1.call1.dbempty
assert((l1().calls[0].dev) == 0) by (compute);assert((l1().calls[0].dev) == 0); // l1.call1.dev
assert((l1().calls[0].upgrade) == 0) by (compute);assert((l1().calls[0].upgrade) == 0); // l1.call1.upgrade
assert((l1().calls[1].kind) == 2) by (compute);assert((l1().calls[1].kind) == 2); // l1.call2.kind
assert((l1().calls[1].ns) == 5) by (compute);assert((l1().calls[1].ns) == 5); // l1.call2.ns
assert((l1().calls[1].mode) == 1) by (compute);assert((l1().calls[1].mode) == 1); // l1.call2.mode
assert((l1().calls[1].installdir) == 1) by (compute);assert((l1().calls[1].installdir) == 1); // l1.call2.installdir
assert((l1().calls[1].lifecycle) == 1) by (compute);assert((l1().calls[1].lifecycle) == 1); // l1.call2.lifecycle
assert((l1().calls[2].kind) == 5) by (compute);assert((l1().calls[2].kind) == 5); // l1.call3.kind
assert((l1().calls[2].greeting) == 3) by (compute);assert((l1().calls[2].greeting) == 3); // l1.call3.greeting
assert((b(l3().ok)) == 1) by (compute);assert((b(l3().ok)) == 1); // l3.ok
assert((b(l3().installed)) == 1) by (compute);assert((b(l3().installed)) == 1); // l3.installed
assert((l3().calls.len() as int) == 3) by (compute);assert((l3().calls.len() as int) == 3); // l3.calls
assert((l3().calls[0].kind) == 1) by (compute);assert((l3().calls[0].kind) == 1); // l3.call1.kind
assert((l3().calls[0].db_empty) == 1) by (compute);assert((l3().calls[0].db_empty) == 1); // l3.call1.dbempty
assert((l3().calls[0].dev) == 0) by (compute);assert((l3().calls[0].dev) == 0); // l3.call1.dev
assert((l3().calls[1].kind) == 2) by (compute);assert((l3().calls[1].kind) == 2); // l3.call2.kind
assert((l3().calls[1].ns) == 5) by (compute);assert((l3().calls[1].ns) == 5); // l3.call2.ns
assert((l3().calls[1].mode) == 2) by (compute);assert((l3().calls[1].mode) == 2); // l3.call2.mode
assert((l3().calls[1].greeting) == 4) by (compute);assert((l3().calls[1].greeting) == 4); // l3.call2.greeting
assert((l3().calls[1].installdir) == 1) by (compute);assert((l3().calls[1].installdir) == 1); // l3.call2.installdir
assert((l3().calls[1].lifecycle) == 1) by (compute);assert((l3().calls[1].lifecycle) == 1); // l3.call2.lifecycle
assert((l3().calls[2].kind) == 5) by (compute);assert((l3().calls[2].kind) == 5); // l3.call3.kind
assert((l3().calls[2].greeting) == 4) by (compute);assert((l3().calls[2].greeting) == 4); // l3.call3.greeting
assert((b(l5().ok)) == 1) by (compute);assert((b(l5().ok)) == 1); // l5.ok
assert((b(l5().installed)) == 1) by (compute);assert((b(l5().installed)) == 1); // l5.installed
assert((l5().calls.len() as int) == 3) by (compute);assert((l5().calls.len() as int) == 3); // l5.calls
assert((l5().calls[0].kind) == 1) by (compute);assert((l5().calls[0].kind) == 1); // l5.call1.kind
assert((l5().calls[0].db_empty) == 1) by (compute);assert((l5().calls[0].db_empty) == 1); // l5.call1.dbempty
assert((l5().calls[0].dev) == 0) by (compute);assert((l5().calls[0].dev) == 0); // l5.call1.dev
assert((l5().calls[0].upgrade) == 1) by (compute);assert((l5().calls[0].upgrade) == 1); // l5.call1.upgrade
assert((l5().calls[1].kind) == 2) by (compute);assert((l5().calls[1].kind) == 2); // l5.call2.kind
assert((l5().calls[1].ns) == 5) by (compute);assert((l5().calls[1].ns) == 5); // l5.call2.ns
assert((l5().calls[1].mode) == 1) by (compute);assert((l5().calls[1].mode) == 1); // l5.call2.mode
assert((l5().calls[1].greeting) == 0) by (compute);assert((l5().calls[1].greeting) == 0); // l5.call2.greeting
assert((l5().calls[1].installdir) == 1) by (compute);assert((l5().calls[1].installdir) == 1); // l5.call2.installdir
assert((l5().calls[1].lifecycle) == 1) by (compute);assert((l5().calls[1].lifecycle) == 1); // l5.call2.lifecycle
assert((l5().calls[2].kind) == 5) by (compute);assert((l5().calls[2].kind) == 5); // l5.call3.kind
assert((l5().calls[2].greeting) == 3) by (compute);assert((l5().calls[2].greeting) == 3); // l5.call3.greeting
assert((b(u1().ok)) == 1) by (compute);assert((b(u1().ok)) == 1); // u1.ok
assert((b(u1().installed)) == 0) by (compute);assert((b(u1().installed)) == 0); // u1.installed
assert((u1().calls.len() as int) == 2) by (compute);assert((u1().calls.len() as int) == 2); // u1.calls
assert((u1().calls[0].kind) == 3) by (compute);assert((u1().calls[0].kind) == 3); // u1.call1.kind
assert((u1().calls[0].purge) == 0) by (compute);assert((u1().calls[0].purge) == 0); // u1.call1.purge
assert((u1().calls[1].kind) == 4) by (compute);assert((u1().calls[1].kind) == 4); // u1.call2.kind
assert((u1().calls[1].db_empty) == 1) by (compute);assert((u1().calls[1].db_empty) == 1); // u1.call2.dbempty
assert((b(l2().ok)) == 1) by (compute);assert((b(l2().ok)) == 1); // l2.ok
assert((b(l2().installed)) == 1) by (compute);assert((b(l2().installed)) == 1); // l2.installed
assert((l2().calls.len() as int) == 3) by (compute);assert((l2().calls.len() as int) == 3); // l2.calls
assert((l2().calls[0].kind) == 1) by (compute);assert((l2().calls[0].kind) == 1); // l2.call1.kind
assert((l2().calls[0].db_empty) == 1) by (compute);assert((l2().calls[0].db_empty) == 1); // l2.call1.dbempty
assert((l2().calls[0].dev) == 1) by (compute);assert((l2().calls[0].dev) == 1); // l2.call1.dev
assert((l2().calls[0].upgrade) == 0) by (compute);assert((l2().calls[0].upgrade) == 0); // l2.call1.upgrade
assert((l2().calls[1].kind) == 2) by (compute);assert((l2().calls[1].kind) == 2); // l2.call2.kind
assert((l2().calls[1].ns) == 5) by (compute);assert((l2().calls[1].ns) == 5); // l2.call2.ns
assert((l2().calls[1].mode) == 1) by (compute);assert((l2().calls[1].mode) == 1); // l2.call2.mode
assert((l2().calls[1].greeting) == 0) by (compute);assert((l2().calls[1].greeting) == 0); // l2.call2.greeting
assert((l2().calls[1].installdir) == 1) by (compute);assert((l2().calls[1].installdir) == 1); // l2.call2.installdir
assert((l2().calls[1].lifecycle) == 1) by (compute);assert((l2().calls[1].lifecycle) == 1); // l2.call2.lifecycle
assert((l2().calls[2].kind) == 5) by (compute);assert((l2().calls[2].kind) == 5); // l2.call3.kind
assert((l2().calls[2].greeting) == 3) by (compute);assert((l2().calls[2].greeting) == 3); // l2.call3.greeting
assert((b(u2().ok)) == 1) by (compute);assert((b(u2().ok)) == 1); // u2.ok
assert((b(u2().installed)) == 0) by (compute);assert((b(u2().installed)) == 0); // u2.installed
assert((u2().calls.len() as int) == 2) by (compute);assert((u2().calls.len() as int) == 2); // u2.calls
assert((u2().calls[0].kind) == 3) by (compute);assert((u2().calls[0].kind) == 3); // u2.call1.kind
assert((u2().calls[0].purge) == 0) by (compute);assert((u2().calls[0].purge) == 0); // u2.call1.purge
assert((u2().calls[1].kind) == 4) by (compute);assert((u2().calls[1].kind) == 4); // u2.call2.kind
assert((u2().calls[1].db_empty) == 1) by (compute);assert((u2().calls[1].db_empty) == 1); // u2.call2.dbempty
assert((b(e1().ok)) == 0) by (compute);assert((b(e1().ok)) == 0); // e1.ok
assert((b(e1().installed)) == 0) by (compute);assert((b(e1().installed)) == 0); // e1.installed
assert((e1().calls.len() as int) == 1) by (compute);assert((e1().calls.len() as int) == 1); // e1.calls
assert((b(n1().ok)) == 1) by (compute);assert((b(n1().ok)) == 1); // n1.ok
assert((b(n1().installed)) == 1) by (compute);assert((b(n1().installed)) == 1); // n1.installed
assert((n1().calls.len() as int) == 0) by (compute);assert((n1().calls.len() as int) == 0); // n1.calls
}
}
fn main() {}
