// Iteration75 (repair group R8B): manifest neighbours for system requirements and attribute expressions.
// IPM 571e04a1. The deciding definitions are inherited unchanged and are faithful to the product lines below.
//
//   resources.rs::requirement_failures  Storage/SystemRequirements.cls:61-70 CheckInteroperability
//       tInteroperabilityRequired = $case(Interoperability,"enabled":1,:0) || Health, failure only when the
//       namespace is not interoperability-enabled. Interoperability 1 = "enabled", 2 = "disabled".
//   attr_evaluator.rs::macro_value      Utils/Module.cls:2763-2785 EvaluateMacroHelper: a generated class
//       returns $$$name under #IfDef and "" otherwise. The macro's value is the preprocessor's (an input).
//   attr_evaluator.rs::evaluate_expression  Storage/Module.cls:1885-1894 %EvaluateExpression: $zstrip leading
//       whitespace, return @expr, "" when it throws. The interpreter result is an input.
// Native receipt for both expression kinds: consolidated/native/results/20261114T025636Z-r1-evaluator
// (E16 `$$$OK` gives 1, E17 undefined macro gives "", E10/E11 `#{1+2}` gives 3, E13/E14 give "").
#[path="../../consolidated/iter3/resources.rs"]
pub mod res;
#[path="../../consolidated/verus/attr_evaluator.rs"]
pub mod attr;
use vstd::prelude::*;

verus! {

use res::{Requirements, Environment, Tri, Failure, requirement_failures};
use attr::{macro_value, evaluate_expression, strip_leading};

// module.requirement.values#4: Health=true with Interoperability="disabled" fails on a namespace without
// interoperability and passes on an interoperability-enabled one (both on a Health instance).
pub proof fn health_interop_neighbors() {
    let user = Environment { health: true, interoperability: true, namespace: seq![85int, 83, 69, 82] };
    let off = Environment { interoperability: false, ..user };
    let health_disabled = Requirements { health: Tri::On, interoperability: 2, sys_namespace: false };
    assert(requirement_failures(health_disabled, off) == seq![Failure::Interoperability]);
    assert(requirement_failures(health_disabled, user).len() == 0);
    // Without Health, "disabled" never requires interoperability.
    let plain_disabled = Requirements { health: Tri::Unset, ..health_disabled };
    assert(requirement_failures(plain_disabled, off).len() == 0);
}

// module.parameter.expressions#6: a defined macro gives its value, an undefined one gives nothing.
pub proof fn defined_macro_gives_value(name: int, definitions: Map<int, Seq<char>>)
    requires definitions.dom().contains(name)
    ensures macro_value(name, definitions, true).value == definitions[name],
        !macro_value(name, definitions, true).error
{}

pub proof fn macro_neighbors() {
    let defs = Map::<int, Seq<char>>::empty().insert(1, seq!['1']);
    assert(macro_value(1, defs, true).value == seq!['1']);
    assert(macro_value(2, defs, true).value == Seq::<char>::empty());
}

// module.parameter.expressions#8: a successful expression gives the interpreter's value after the leading
// blank is stripped; a failing one gives nothing.
pub proof fn expression_success_gives_value(raw: Seq<char>, interpreter: spec_fn(Seq<char>) -> Option<Seq<char>>,
    value: Seq<char>)
    requires interpreter(strip_leading(raw)) == Some(value)
    ensures evaluate_expression(raw, interpreter) == value
{}

pub open spec fn one_plus_two(e: Seq<char>) -> Option<Seq<char>> {
    if e == seq!['1', '+', '2'] { Some(seq!['3']) } else { None }
}

pub proof fn expression_neighbors() {
    reveal_with_fuel(strip_leading, 3);
    assert(strip_leading(seq![' ', '1', '+', '2']) =~= seq!['1', '+', '2']);
    assert(evaluate_expression(seq![' ', '1', '+', '2'], |e: Seq<char>| one_plus_two(e)) == seq!['3']);
    assert(strip_leading(seq!['1', '/', '0']) =~= seq!['1', '/', '0']);
    assert(evaluate_expression(seq!['1', '/', '0'], |e: Seq<char>| one_plus_two(e)) == Seq::<char>::empty());
}

}
fn main() {}
