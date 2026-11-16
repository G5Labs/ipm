#[path="../../../../Verus_verification/consolidated/iter50/defmods.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((mode_code(effective(s3(), seq![None, Some(3int), None, None], Some(4), 1))) == 3) by (compute);assert((mode_code(effective(s3(), seq![None, Some(3int), None, None], Some(4), 1))) == 3); // l2.mode
assert((mode_code(effective(s3(), empty(), Some(4), 1))) == 2) by (compute);assert((mode_code(effective(s3(), empty(), Some(4), 1))) == 2); // l1.mode
assert((b(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).ok)) == 0) by (compute);assert((b(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).ok)) == 0); // f1.ok
assert((none_listed(empty())) == 1) by (compute);assert((none_listed(empty())) == 1); // g0.get.none
assert((listed(empty(), 1, 1)) == 0) by (compute);assert((listed(empty(), 1, 1)) == 0); // g0.get.modestored
assert((listed(empty(), 1, 2)) == 0) by (compute);assert((listed(empty(), 1, 2)) == 0); // g0.get.modesecond
assert((listed(empty(), 2, 5)) == 0) by (compute);assert((listed(empty(), 2, 5)) == 0); // g0.get.othersub
assert((listed(empty(), 3, 1)) == 0) by (compute);assert((listed(empty(), 3, 1)) == 0); // g0.get.verbose
assert((none_listed(default_modifiers(empty(), setc(empty())).stored)) == 1) by (compute);assert((none_listed(default_modifiers(empty(), setc(empty())).stored)) == 1); // s0.get.none
assert((listed(default_modifiers(empty(), setc(empty())).stored, 1, 1)) == 0) by (compute);assert((listed(default_modifiers(empty(), setc(empty())).stored, 1, 1)) == 0); // s0.get.modestored
assert((listed(default_modifiers(empty(), setc(empty())).stored, 1, 2)) == 0) by (compute);assert((listed(default_modifiers(empty(), setc(empty())).stored, 1, 2)) == 0); // s0.get.modesecond
assert((listed(default_modifiers(empty(), setc(empty())).stored, 2, 5)) == 0) by (compute);assert((listed(default_modifiers(empty(), setc(empty())).stored, 2, 5)) == 0); // s0.get.othersub
assert((listed(default_modifiers(empty(), setc(empty())).stored, 3, 1)) == 0) by (compute);assert((listed(default_modifiers(empty(), setc(empty())).stored, 3, 1)) == 0); // s0.get.verbose
assert((none_listed(s2())) == 0) by (compute);assert((none_listed(s2())) == 0); // s2.get.none
assert((listed(s2(), 1, 1)) == 1) by (compute);assert((listed(s2(), 1, 1)) == 1); // s2.get.modestored
assert((listed(s2(), 1, 2)) == 0) by (compute);assert((listed(s2(), 1, 2)) == 0); // s2.get.modesecond
assert((listed(s2(), 2, 5)) == 1) by (compute);assert((listed(s2(), 2, 5)) == 1); // s2.get.othersub
assert((listed(s2(), 3, 1)) == 0) by (compute);assert((listed(s2(), 3, 1)) == 0); // s2.get.verbose
assert((none_listed(s3())) == 0) by (compute);assert((none_listed(s3())) == 0); // s3.get.none
assert((listed(s3(), 1, 1)) == 0) by (compute);assert((listed(s3(), 1, 1)) == 0); // s3.get.modestored
assert((listed(s3(), 1, 2)) == 1) by (compute);assert((listed(s3(), 1, 2)) == 1); // s3.get.modesecond
assert((listed(s3(), 2, 5)) == 1) by (compute);assert((listed(s3(), 2, 5)) == 1); // s3.get.othersub
assert((listed(s3(), 3, 1)) == 0) by (compute);assert((listed(s3(), 3, 1)) == 0); // s3.get.verbose
assert((none_listed(s3())) == 0) by (compute);assert((none_listed(s3())) == 0); // l2.get.none
assert((listed(s3(), 1, 1)) == 0) by (compute);assert((listed(s3(), 1, 1)) == 0); // l2.get.modestored
assert((listed(s3(), 1, 2)) == 1) by (compute);assert((listed(s3(), 1, 2)) == 1); // l2.get.modesecond
assert((listed(s3(), 2, 5)) == 1) by (compute);assert((listed(s3(), 2, 5)) == 1); // l2.get.othersub
assert((listed(s3(), 3, 1)) == 0) by (compute);assert((listed(s3(), 3, 1)) == 0); // l2.get.verbose
assert((none_listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored)) == 0) by (compute);assert((none_listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored)) == 0); // f1.get.none
assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 1, 1)) == 0) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 1, 1)) == 0); // f1.get.modestored
assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 1, 2)) == 1) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 1, 2)) == 1); // f1.get.modesecond
assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 2, 5)) == 1) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 2, 5)) == 1); // f1.get.othersub
assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 3, 1)) == 1) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).stored, 3, 1)) == 1); // f1.get.verbose
assert((none_listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored)) == 1) by (compute);assert((none_listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored)) == 1); // d0.get.none
assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 1, 1)) == 0) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 1, 1)) == 0); // d0.get.modestored
assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 1, 2)) == 0) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 1, 2)) == 0); // d0.get.modesecond
assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 2, 5)) == 0) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 2, 5)) == 0); // d0.get.othersub
assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 3, 1)) == 0) by (compute);assert((listed(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, 3, 1)) == 0); // d0.get.verbose
assert((b(default_modifiers(empty(), Cmd { set: false, get: false, delete: false, unknown_modifier: false, data: empty() }).ok)) == 0) by (compute);assert((b(default_modifiers(empty(), Cmd { set: false, get: false, delete: false, unknown_modifier: false, data: empty() }).ok)) == 0); // e1.ok
assert((b(default_modifiers(empty(), Cmd { set: true, get: true, delete: false, unknown_modifier: false, data: empty() }).ok)) == 0) by (compute);assert((b(default_modifiers(empty(), Cmd { set: true, get: true, delete: false, unknown_modifier: false, data: empty() }).ok)) == 0); // e2.ok
assert((b(default_modifiers(empty(), setc(empty())).ok)) == 1) by (compute);assert((b(default_modifiers(empty(), setc(empty())).ok)) == 1); // s0.ok
assert((b(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).ok)) == 1) by (compute);assert((b(default_modifiers(v0(), Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).ok)) == 1); // d0.ok
assert((verbose_load(s3(), empty())) == 0) by (compute);assert((verbose_load(s3(), empty())) == 0); // l1.verbose
assert((verbose_load(s3(), seq![None, Some(3int), None, None])) == 0) by (compute);assert((verbose_load(s3(), seq![None, Some(3int), None, None])) == 0); // l2.verbose
assert((mode_code(effective(v0(), empty(), Some(4), 1))) == 2) by (compute);assert((mode_code(effective(v0(), empty(), Some(4), 1))) == 2); // v1.mode
assert((verbose_load(v0(), empty())) == 1) by (compute);assert((verbose_load(v0(), empty())) == 1); // v1.verbose
assert((mode_code(effective(v0(), seq![None, None, None, Some(0int)], Some(4), 1))) == 2) by (compute);assert((mode_code(effective(v0(), seq![None, None, None, Some(0int)], Some(4), 1))) == 2); // v2.mode
assert((verbose_load(v0(), seq![None, None, None, Some(0int)])) == 0) by (compute);assert((verbose_load(v0(), seq![None, None, None, Some(0int)])) == 0); // v2.verbose
assert((mode_code(effective(empty(), empty(), Some(4), 1))) == 4) by (compute);assert((mode_code(effective(empty(), empty(), Some(4), 1))) == 4); // d1.mode
assert((verbose_load(empty(), empty())) == 0) by (compute);assert((verbose_load(empty(), empty())) == 0); // d1.verbose
}
}
fn main() {}
