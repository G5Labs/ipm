#[path="../../../../Verus_verification/consolidated/iter46/summary.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(sysreq(d()))) == 0);assert((b(sysreq(d()))) == 0); // d.sysreq.heading
assert((0int) == 0);assert((0int) == 0); // a.defaults.mode
assert((b(match a().req { Some(r) => health_entry(r), None => false })) == 0);assert((b(match a().req { Some(r) => health_entry(r), None => false })) == 0); // a.sysreq.health
assert((blocks(a())) == 4);assert((blocks(a())) == 4); // a.blocks
assert((b(sysreq(a()))) == 1);assert((b(sysreq(a()))) == 1); // a.sysreq.heading
assert((b(defaults_shown(a()) > 0)) == 1);assert((b(defaults_shown(a()) > 0)) == 1); // a.defaults.heading
assert((b(a().after_lines > 0)) == 1);assert((b(a().after_lines > 0)) == 1); // a.after.heading
assert((b(shown_code(a()) > 0)) == 1);assert((b(shown_code(a()) > 0)) == 1); // a.code.heading
assert((b(a().files > 0)) == 1);assert((b(a().files > 0)) == 1); // a.files.heading
assert((b(a().deps > 0)) == 1);assert((b(a().deps > 0)) == 1); // a.depends.heading
assert((b(a().dependents > 0)) == 0);assert((b(a().dependents > 0)) == 0); // a.requiredby.heading
assert((b(keywords_shown(a()))) == 0);assert((b(keywords_shown(a()))) == 0); // a.keywords
assert((b(a().desc)) == 1);assert((b(a().desc)) == 1); // a.desc.any
assert((0int) == 0);assert((0int) == 0); // a.code.scoped
assert((a().parameters as int) == 1);assert((a().parameters as int) == 1); // a.stored.parameters
assert((blocks(bmod())) == 2);assert((blocks(bmod())) == 2); // b.blocks
assert((b(sysreq(bmod()))) == 0);assert((b(sysreq(bmod()))) == 0); // b.sysreq.heading
assert((b(defaults_shown(bmod()) > 0)) == 0);assert((b(defaults_shown(bmod()) > 0)) == 0); // b.defaults.heading
assert((b(bmod().after_lines > 0)) == 0);assert((b(bmod().after_lines > 0)) == 0); // b.after.heading
assert((b(shown_code(bmod()) > 0)) == 1);assert((b(shown_code(bmod()) > 0)) == 1); // b.code.heading
assert((b(bmod().files > 0)) == 0);assert((b(bmod().files > 0)) == 0); // b.files.heading
assert((b(bmod().deps > 0)) == 0);assert((b(bmod().deps > 0)) == 0); // b.depends.heading
assert((b(bmod().dependents > 0)) == 1);assert((b(bmod().dependents > 0)) == 1); // b.requiredby.heading
assert((b(keywords_shown(bmod()))) == 0);assert((b(keywords_shown(bmod()))) == 0); // b.keywords
assert((b(bmod().desc)) == 0);assert((b(bmod().desc)) == 0); // b.desc.any
assert((0int) == 0);assert((0int) == 0); // b.code.scoped
assert((bmod().parameters as int) == 0);assert((bmod().parameters as int) == 0); // b.stored.parameters
assert((blocks(d())) == 1);assert((blocks(d())) == 1); // d.blocks
assert((b(defaults_shown(d()) > 0)) == 0);assert((b(defaults_shown(d()) > 0)) == 0); // d.defaults.heading
assert((b(d().after_lines > 0)) == 0);assert((b(d().after_lines > 0)) == 0); // d.after.heading
assert((b(shown_code(d()) > 0)) == 1);assert((b(shown_code(d()) > 0)) == 1); // d.code.heading
assert((b(d().files > 0)) == 0);assert((b(d().files > 0)) == 0); // d.files.heading
assert((b(d().deps > 0)) == 0);assert((b(d().deps > 0)) == 0); // d.depends.heading
assert((b(d().dependents > 0)) == 0);assert((b(d().dependents > 0)) == 0); // d.requiredby.heading
assert((b(keywords_shown(d()))) == 0);assert((b(keywords_shown(d()))) == 0); // d.keywords
assert((b(d().desc)) == 0);assert((b(d().desc)) == 0); // d.desc.any
assert((0int) == 0);assert((0int) == 0); // d.code.scoped
assert((d().parameters as int) == 0);assert((d().parameters as int) == 0); // d.stored.parameters
assert((blocks(h())) == 1);assert((blocks(h())) == 1); // h.blocks
assert((b(sysreq(h()))) == 0);assert((b(sysreq(h()))) == 0); // h.sysreq.heading
assert((b(defaults_shown(h()) > 0)) == 0);assert((b(defaults_shown(h()) > 0)) == 0); // h.defaults.heading
assert((b(h().after_lines > 0)) == 0);assert((b(h().after_lines > 0)) == 0); // h.after.heading
assert((b(shown_code(h()) > 0)) == 1);assert((b(shown_code(h()) > 0)) == 1); // h.code.heading
assert((b(h().files > 0)) == 0);assert((b(h().files > 0)) == 0); // h.files.heading
assert((b(h().deps > 0)) == 0);assert((b(h().deps > 0)) == 0); // h.depends.heading
assert((b(h().dependents > 0)) == 0);assert((b(h().dependents > 0)) == 0); // h.requiredby.heading
assert((b(keywords_shown(h()))) == 0);assert((b(keywords_shown(h()))) == 0); // h.keywords
assert((b(h().desc)) == 0);assert((b(h().desc)) == 0); // h.desc.any
assert((0int) == 0);assert((0int) == 0); // h.code.scoped
assert((h().parameters as int) == 0);assert((h().parameters as int) == 0); // h.stored.parameters
assert((b(match a().req { Some(r) => r.version, None => false })) == 1);assert((b(match a().req { Some(r) => r.version, None => false })) == 1); // a.sysreq.iris
assert((b(match a().req { Some(r) => r.interop, None => false })) == 1);assert((b(match a().req { Some(r) => r.interop, None => false })) == 1); // a.sysreq.interop
}
}
fn main() {}
