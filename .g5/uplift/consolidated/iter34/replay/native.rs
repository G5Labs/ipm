#[path="../../../../Verus_verification/consolidated/iter34/versions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((code(installed(repo_d(), E::Any, true))) == 3) by (compute);assert((code(installed(repo_d(), E::Any, true))) == 3); // d1.version
assert((code(installed(repo_c(), E::Any, true))) == 5) by (compute);assert((code(installed(repo_c(), E::Any, true))) == 5); // c1.version
assert((pos(repo_d(), v200(), true)) == 1) by (compute);assert((pos(repo_d(), v200(), true)) == 1); // q5.pos.7
assert((pos(repo_b(), v1100(), false)) == 1) by (compute);assert((pos(repo_b(), v1100(), false)) == 1); // q1.pos.2
assert((pos(repo_b(), v190(), false)) == 2) by (compute);assert((pos(repo_b(), v190(), false)) == 2); // q1.pos.1
assert((count(repo_b(), false)) == 2) by (compute);assert((count(repo_b(), false)) == 2); // q1.count
assert((pos(repo_b(), v200b1(), true)) == 1) by (compute);assert((pos(repo_b(), v200b1(), true)) == 1); // q2.pos.3
assert((pos(repo_b(), v1100(), true)) == 2) by (compute);assert((pos(repo_b(), v1100(), true)) == 2); // q2.pos.2
assert((pos(repo_b(), v190(), true)) == 3) by (compute);assert((pos(repo_b(), v190(), true)) == 3); // q2.pos.1
assert((count(repo_b(), true)) == 3) by (compute);assert((count(repo_b(), true)) == 3); // q2.count
assert(((if installed(repo_b(), E::Any, true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_b(), E::Any, true).is_some() { 1int } else { 0int })) == 1); // a2.ok
assert((code(installed(repo_b(), E::Any, true))) == 3) by (compute);assert((code(installed(repo_b(), E::Any, true))) == 3); // a2.version
assert(((if installed(repo_b(), E::MajorX(1), true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_b(), E::MajorX(1), true).is_some() { 1int } else { 0int })) == 1); // a3.ok
assert((code(installed(repo_b(), E::MajorX(1), true))) == 2) by (compute);assert((code(installed(repo_b(), E::MajorX(1), true))) == 2); // a3.version
assert(((if installed(repo_b(), E::Exact(v200b1()), true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_b(), E::Exact(v200b1()), true).is_some() { 1int } else { 0int })) == 1); // a4.ok
assert((code(installed(repo_b(), E::Exact(v200b1()), true))) == 3) by (compute);assert((code(installed(repo_b(), E::Exact(v200b1()), true))) == 3); // a4.version
assert(((if installed(repo_b(), E::Exact(v200b1()), true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_b(), E::Exact(v200b1()), true).is_some() { 1int } else { 0int })) == 1); // a5.ok
assert((code(installed(repo_b(), E::Exact(v200b1()), true))) == 3) by (compute);assert((code(installed(repo_b(), E::Exact(v200b1()), true))) == 3); // a5.version
assert(((if installed(repo_b(), E::Any, true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_b(), E::Any, true).is_some() { 1int } else { 0int })) == 1); // a6.ok
assert((code(installed(repo_b(), E::Any, true))) == 3) by (compute);assert((code(installed(repo_b(), E::Any, true))) == 3); // a6.version
assert(((if installed(repo_b(), E::MajorX(1), true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_b(), E::MajorX(1), true).is_some() { 1int } else { 0int })) == 1); // a7.ok
assert((code(installed(repo_b(), E::MajorX(1), true))) == 2) by (compute);assert((code(installed(repo_b(), E::MajorX(1), true))) == 2); // a7.version
assert((pos(repo_c(), v100rc10(), true)) == 1) by (compute);assert((pos(repo_c(), v100rc10(), true)) == 1); // q3.pos.6
assert((pos(repo_c(), v100rc2(), true)) == 2) by (compute);assert((pos(repo_c(), v100rc2(), true)) == 2); // q3.pos.5
assert((pos(repo_c(), v090(), true)) == 3) by (compute);assert((pos(repo_c(), v090(), true)) == 3); // q3.pos.4
assert((count(repo_c(), true)) == 3) by (compute);assert((count(repo_c(), true)) == 3); // q3.count
assert((pos(repo_c(), v090(), false)) == 1) by (compute);assert((pos(repo_c(), v090(), false)) == 1); // q4.pos.4
assert((count(repo_c(), false)) == 1) by (compute);assert((count(repo_c(), false)) == 1); // q4.count
assert(((if installed(repo_c(), E::Any, true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_c(), E::Any, true).is_some() { 1int } else { 0int })) == 1); // c1.ok
assert(((if installed(repo_c(), E::Exact(v100rc2()), true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_c(), E::Exact(v100rc2()), true).is_some() { 1int } else { 0int })) == 1); // c2.ok
assert((code(installed(repo_c(), E::Exact(v100rc2()), true))) == 5) by (compute);assert((code(installed(repo_c(), E::Exact(v100rc2()), true))) == 5); // c2.version
assert((pos(repo_d(), v200b1(), true)) == 2) by (compute);assert((pos(repo_d(), v200b1(), true)) == 2); // q5.pos.3
assert((count(repo_d(), true)) == 2) by (compute);assert((count(repo_d(), true)) == 2); // q5.count
assert(((if installed(repo_d(), E::Any, true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_d(), E::Any, true).is_some() { 1int } else { 0int })) == 1); // d1.ok
assert(((if installed(repo_d(), E::Exact(v200()), true).is_some() { 1int } else { 0int })) == 1) by (compute);assert(((if installed(repo_d(), E::Exact(v200()), true).is_some() { 1int } else { 0int })) == 1); // d2.ok
assert((code(installed(repo_d(), E::Exact(v200()), true))) == 7) by (compute);assert((code(installed(repo_d(), E::Exact(v200()), true))) == 7); // d2.version
}
}
fn main() {}
