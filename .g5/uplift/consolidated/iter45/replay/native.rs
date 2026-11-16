#[path="../../../../Verus_verification/consolidated/iter45/scan.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

reveal_with_fuel(any_unreadable_change, 2); assert(((if sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: false, disk: 4, base: Some(1), ns: Some(1) }]).ok { 1int } else { 0int })) == 0);assert(((if sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: false, disk: 4, base: Some(1), ns: Some(1) }]).ok { 1int } else { 0int })) == 0); // f4.ok
reveal_with_fuel(any_unreadable_change, 2); assert(((match sync(seq![inner(2, 1)]).files[0].ns { Some(v) => v, None => -1 })) == 2);assert(((match sync(seq![inner(2, 1)]).files[0].ns { Some(v) => v, None => -1 })) == 2); // f1.inner
reveal_with_fuel(any_unreadable_change, 2); assert(((match sync(seq![F { depth: 1, upper_ext: true, skip_dir: false, readable: true, disk: 3, base: None, ns: None }]).files[0].ns { Some(v) => v, None => -1 })) == 3);assert(((match sync(seq![F { depth: 1, upper_ext: true, skip_dir: false, readable: true, disk: 3, base: None, ns: None }]).files[0].ns { Some(v) => v, None => -1 })) == 3); // f3.up
reveal_with_fuel(any_unreadable_change, 2); assert(((match sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: true, disk: 4, base: Some(1), ns: Some(1) }]).files[0].ns { Some(v) => v, None => -1 })) == 4);assert(((match sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: true, disk: 4, base: Some(1), ns: Some(1) }]).files[0].ns { Some(v) => v, None => -1 })) == 4); // f5.top
reveal_with_fuel(any_unreadable_change, 2); assert(((match sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: false, disk: 4, base: Some(1), ns: Some(1) }]).files[0].ns { Some(v) => v, None => -1 })) == 1);assert(((match sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: false, disk: 4, base: Some(1), ns: Some(1) }]).files[0].ns { Some(v) => v, None => -1 })) == 1); // f4.top
reveal_with_fuel(any_unreadable_change, 2); assert(((if sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: true, disk: 4, base: Some(1), ns: Some(1) }]).ok { 1int } else { 0int })) == 1);assert(((if sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: true, disk: 4, base: Some(1), ns: Some(1) }]).ok { 1int } else { 0int })) == 1); // f5.ok
}
}
fn main() {}
