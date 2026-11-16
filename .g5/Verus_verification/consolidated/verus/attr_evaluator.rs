use vstd::prelude::*;
#[path = "env_config.rs"]
pub mod env_config;
use env_config::replacement;

verus! {
pub open spec fn lower(c: char) -> char { match c {
        'A' => 'a',
        'B' => 'b',
        'C' => 'c',
        'D' => 'd',
        'E' => 'e',
        'F' => 'f',
        'G' => 'g',
        'H' => 'h',
        'I' => 'i',
        'J' => 'j',
        'K' => 'k',
        'L' => 'l',
        'M' => 'm',
        'N' => 'n',
        'O' => 'o',
        'P' => 'p',
        'Q' => 'q',
        'R' => 'r',
        'S' => 's',
        'T' => 't',
        'U' => 'u',
        'V' => 'v',
        'W' => 'w',
        'X' => 'x',
        'Y' => 'y',
        'Z' => 'z',
        _ => c,
} }
pub enum SystemName { Namespace, Ns, NamespaceLower, ManagerDirectory, Unrecognized }
pub open spec fn lower_text(text: Seq<char>) -> Seq<char>
    decreases text.len()
{
    if text.len() == 0 { Seq::empty() }
    else { seq![lower(text[0])] + lower_text(text.subrange(1, text.len() as int)) }
}
// This scoped adapter represents four native consumers, not the remaining system variables.
pub open spec fn system_expression(name: SystemName, namespace: Seq<char>, manager_directory: Seq<char>) -> Option<Seq<char>> {
    match name {
        SystemName::Namespace | SystemName::Ns => Some(namespace),
        SystemName::NamespaceLower => Some(lower_text(namespace)),
        SystemName::ManagerDirectory => Some(manager_directory),
        SystemName::Unrecognized => None,
    }
}
pub proof fn system_namespace_alias_and_lower(ns: Seq<char>, mgr: Seq<char>)
    ensures system_expression(SystemName::Namespace, ns, mgr) == system_expression(SystemName::Ns, ns, mgr),
        system_expression(SystemName::NamespaceLower, ns, mgr) == Some(lower_text(ns)),
        system_expression(SystemName::ManagerDirectory, ns, mgr) == Some(mgr) {}
// The demonstrated regex-name domain: ASCII literals and dot wildcard.
// Full ICU regex syntax is an explicit open environment correspondence boundary.
pub open spec fn name_matches(pattern: Seq<char>, name: Seq<char>) -> bool
    decreases pattern.len()
{
    pattern.len() == name.len() && (pattern.len() == 0 ||
        ((pattern[0] == '.' || lower(pattern[0]) == lower(name[0])) &&
        name_matches(pattern.subrange(1, pattern.len() as int), name.subrange(1, name.len() as int))))
}
pub proof fn dotted_name_is_not_literal()
    ensures name_matches(seq!['a','.','b'], seq!['a','x','b']),
        name_matches(seq!['N','S'], seq!['n','s'])
{ reveal_with_fuel(name_matches, 5); }
pub open spec fn quote_backslashes(text: Seq<char>) -> Seq<char>
    decreases text.len()
{
    if text.len() == 0 { Seq::empty() }
    else { (if text[0] == '\\' { seq!['\\','\\'] } else { seq![text[0]] }) +
        quote_backslashes(text.subrange(1, text.len() as int)) }
}
pub proof fn quote_cons(c: char, rest: Seq<char>)
    ensures quote_backslashes(seq![c] + rest) ==
        (if c == '\\' { seq!['\\','\\'] } else { seq![c] }) + quote_backslashes(rest)
{
    assert((seq![c] + rest).subrange(1, (seq![c] + rest).len() as int) =~= rest);
    reveal_with_fuel(quote_backslashes, 1);
}
pub proof fn replace_plain_cons(c: char, rest: Seq<char>, whole: Seq<char>, group: Seq<char>)
    requires c != '\\', c != '$'
    ensures replacement(seq![c] + rest, whole, group) ==
        (if replacement(rest, whole, group).is_some() { Some(seq![c] + replacement(rest, whole, group).unwrap()) } else { None })
{
    assert((seq![c] + rest).subrange(1, (seq![c] + rest).len() as int) =~= rest);
    reveal_with_fuel(replacement, 1);
}
pub proof fn replace_escaped_cons(c: char, rest: Seq<char>, whole: Seq<char>, group: Seq<char>)
    ensures replacement(seq!['\\', c] + rest, whole, group) ==
        (if replacement(rest, whole, group).is_some() { Some(seq![c] + replacement(rest, whole, group).unwrap()) } else { None })
{
    assert((seq!['\\',c] + rest).subrange(2, (seq!['\\',c] + rest).len() as int) =~= rest);
    reveal_with_fuel(replacement, 1);
}
pub proof fn backslash_quote_is_inverse(text: Seq<char>, whole: Seq<char>, group: Seq<char>)
    requires !text.contains('$')
    ensures replacement(quote_backslashes(text), whole, group) == Some(text)
    decreases text.len()
{
    if text.len() > 0 {
        let c = text[0]; let rest = text.subrange(1, text.len() as int);
        assert(text =~= seq![c] + rest);
        backslash_quote_is_inverse(rest, whole, group);
        quote_cons(c, rest);
        if c == '\\' { replace_escaped_cons(c, quote_backslashes(rest), whole, group); }
        else { replace_plain_cons(c, quote_backslashes(rest), whole, group); }
    }
}
// A concrete regex match is a supplied matcher result, not an assumed output of evaluation.
pub open spec fn regex_replace_match(text: Seq<char>, start: int, end: int, matched: bool, value: Seq<char>, group1: Seq<char>) -> Option<Seq<char>>
    recommends matched ==> 0 <= start <= end <= text.len()
{
    if !matched { Some(text) }
    else {
        let rep = replacement(quote_backslashes(value), text.subrange(start, end), group1);
        if rep.is_some() { Some(text.subrange(0, start) + rep.unwrap() + text.subrange(end, text.len() as int)) }
        else { None }
    }
}
pub proof fn full_match_backslash_literal(text: Seq<char>, value: Seq<char>, group: Seq<char>)
    requires !value.contains('$')
    ensures regex_replace_match(text, 0, text.len() as int, true, value, group) == Some(value)
{
    assert(text.subrange(0, text.len() as int) =~= text);
    backslash_quote_is_inverse(value, text, group);
    assert((text.subrange(0, 0) + value + text.subrange(text.len() as int, text.len() as int)) =~= value);
}
pub proof fn matched_dollar_can_fail(text: Seq<char>)
    requires text.len() > 0
    ensures regex_replace_match(text, 0, text.len() as int, true, seq!['$','5'], Seq::empty()).is_none(),
        regex_replace_match(text, 0, 0, false, seq!['$','5'], Seq::empty()) == Some(text)
{ reveal_with_fuel(replacement, 4); reveal_with_fuel(quote_backslashes, 4); }
pub proof fn attribute_backslash_is_literal()
    ensures regex_replace_match(seq!['x'], 0, 1, true, seq!['a','\\','b'], Seq::empty()) == Some(seq!['a','\\','b'])
{ reveal_with_fuel(replacement, 7); reveal_with_fuel(quote_backslashes, 5); }

pub open spec fn parameters(defaults: Map<int, Seq<char>>, command: Map<int, Seq<char>>) -> Map<int, Seq<char>> {
    Map::new(defaults.dom(), |k: int| if command.dom().contains(k) { command[k] } else { defaults[k] })
}
pub open spec fn with_builtins(custom: Map<int, Seq<char>>, builtin: Map<int, Seq<char>>) -> Map<int, Seq<char>> {
    Map::new(custom.dom().union(builtin.dom()), |k: int| if builtin.dom().contains(k) { builtin[k] } else { custom[k] })
}
pub proof fn parameter_precedence(defaults: Map<int, Seq<char>>, command: Map<int, Seq<char>>, builtin: Map<int, Seq<char>>, k: int)
    requires defaults.dom().contains(k), command.dom().contains(k)
    ensures parameters(defaults, command)[k] == command[k],
        builtin.dom().contains(k) ==> with_builtins(parameters(defaults, command), builtin)[k] == builtin[k],
        !builtin.dom().contains(k) ==> with_builtins(parameters(defaults, command), builtin)[k] == command[k] {}

pub enum Token { Literal(Seq<char>), Reference(int) }
pub open spec fn replace_refs(tokens: Seq<Token>, local: Map<int, Seq<Token>>, system: Map<int, Seq<Token>>) -> Seq<Token>
    decreases tokens.len()
{
    if tokens.len() == 0 { Seq::empty() }
    else {
        let head = match tokens[0] { Token::Literal(_) => seq![tokens[0]],
            Token::Reference(k) => if local.dom().contains(k) { local[k] }
                else if system.dom().contains(k) { system[k] } else { seq![tokens[0]] } };
        head + replace_refs(tokens.subrange(1, tokens.len() as int), local, system)
    }
}
pub open spec fn resolve_pass(cp: Map<int, Seq<Token>>, system: Map<int, Seq<Token>>, order: Seq<int>) -> Map<int, Seq<Token>>
    decreases order.len()
{
    if order.len() == 0 { cp }
    else {
        let k = order[0];
        let next = if cp.dom().contains(k) { cp.insert(k, replace_refs(cp[k], cp, system)) } else { cp };
        resolve_pass(next, system, order.subrange(1, order.len() as int))
    }
}
pub struct ResolveResult { pub params: Map<int, Seq<Token>>, pub circular_error: bool }
pub open spec fn resolve_bounded(cp: Map<int, Seq<Token>>, system: Map<int, Seq<Token>>, order: Seq<int>, levels: nat) -> ResolveResult
    decreases levels
{
    if levels == 0 { ResolveResult { params: cp, circular_error: false } }
    else {
        let next = resolve_pass(cp, system, order);
        if next == cp { ResolveResult { params: cp, circular_error: false } }
        else { resolve_bounded(next, system, order, (levels-1) as nat) }
    }
}
pub proof fn declared_cycle_error_is_unreachable(cp: Map<int, Seq<Token>>, system: Map<int, Seq<Token>>, order: Seq<int>, levels: nat)
    ensures !resolve_bounded(cp, system, order, levels).circular_error
    decreases levels
{
    if levels > 0 { declared_cycle_error_is_unreachable(resolve_pass(cp, system, order), system, order, (levels-1) as nat); }
}
pub proof fn nested_lookup_uses_local_before_system(local: Map<int, Seq<Token>>, system: Map<int, Seq<Token>>, k: int)
    requires local.dom().contains(k)
    ensures replace_refs(seq![Token::Reference(k)], local, system) == local[k]
{ reveal_with_fuel(replace_refs, 2); }
pub proof fn cycle_witness()
    ensures !resolve_bounded(map![1int => seq![Token::Reference(2)], 2int => seq![Token::Reference(1)]],
        Map::empty(), seq![1int,2int], 20).circular_error
{
    declared_cycle_error_is_unreachable(map![1int => seq![Token::Reference(2)], 2int => seq![Token::Reference(1)]], Map::empty(), seq![1int,2int], 20);
}

pub struct EvalResult { pub value: Seq<char>, pub error: bool, pub generated_macro_class: bool }
// Compile/run outcomes come from IRIS, whose expression language is not assumed total.
pub open spec fn macro_helper(compiler_result: Option<Seq<char>>) -> EvalResult {
    match compiler_result {
        Some(value) => EvalResult { value, error: false, generated_macro_class: false },
        None => EvalResult { value: Seq::empty(), error: true, generated_macro_class: true },
    }
}
pub open spec fn macro_value(name: int, definitions: Map<int, Seq<char>>, compilation_ok: bool) -> EvalResult {
    if !compilation_ok { macro_helper(None) }
    else { macro_helper(Some(if definitions.dom().contains(name) { definitions[name] } else { Seq::empty() })) }
}
pub open spec fn strip_leading(text: Seq<char>) -> Seq<char>
    decreases text.len()
{
    if text.len() > 0 && (text[0] == ' ' || text[0] == '\t' || text[0] == '\n' || text[0] == '\r') {
        strip_leading(text.subrange(1, text.len() as int))
    } else { text }
}
pub open spec fn expression(run_result: Option<Seq<char>>) -> Seq<char> { run_result.unwrap_or(Seq::empty()) }
pub open spec fn evaluate_expression(raw: Seq<char>, interpreter: spec_fn(Seq<char>) -> Option<Seq<char>>) -> Seq<char> {
    expression(interpreter(strip_leading(raw)))
}
pub proof fn expression_strips_leading_blank(interpreter: spec_fn(Seq<char>) -> Option<Seq<char>>)
    ensures evaluate_expression(seq![' ','1','+','2'], interpreter) == expression(interpreter(seq!['1','+','2']))
{ reveal_with_fuel(strip_leading, 3); assert(strip_leading(seq![' ','1','+','2']) =~= seq!['1','+','2']); }
pub proof fn expression_failure_is_empty(raw: Seq<char>, interpreter: spec_fn(Seq<char>) -> Option<Seq<char>>)
    requires interpreter(strip_leading(raw)).is_none()
    ensures evaluate_expression(raw, interpreter) == Seq::<char>::empty() {}
pub proof fn undefined_macro_is_empty(name: int, definitions: Map<int, Seq<char>>)
    requires !definitions.dom().contains(name)
    ensures macro_value(name, definitions, true).value == Seq::<char>::empty(), !macro_value(name, definitions, true).error {}

// The opening brace gate precedes macros, parameters and expressions in the real body.
pub open spec fn evaluate(raw: Seq<char>, evaluated_pipeline: EvalResult) -> EvalResult {
    if !raw.contains('{') { EvalResult { value: raw, error: false, generated_macro_class: false } }
    else { evaluated_pipeline }
}
pub proof fn brace_free_macro_is_unchanged(raw: Seq<char>, later: EvalResult)
    requires !raw.contains('{')
    ensures evaluate(raw, later).value == raw, !evaluate(raw, later).error,
        !evaluate(raw, later).generated_macro_class {}
pub proof fn macro_error_keeps_generated_class()
    ensures macro_helper(None).error, macro_helper(None).generated_macro_class,
        expression(None) == Seq::<char>::empty() {}

pub struct StoredLoad { pub module: bool, pub committed: bool, pub history_exists: bool, pub history_finalized: bool, pub history_success: bool, pub status_ok: bool }
// Boundary premise: all load phases and final commit succeeded; caller transaction absent.
pub open spec fn after_message(message: EvalResult) -> StoredLoad {
    StoredLoad { module: true, committed: true, history_exists: true, history_finalized: true,
        history_success: !message.error, status_ok: !message.error }
}
pub proof fn failed_message_is_not_rollback(message: EvalResult)
    requires message.error
    ensures after_message(message).module, after_message(message).committed,
        after_message(message).history_exists, after_message(message).history_finalized,
        !after_message(message).history_success, !after_message(message).status_ok {}
pub proof fn successful_neighbor(message: EvalResult)
    requires !message.error
    ensures after_message(message).module, after_message(message).committed,
        after_message(message).history_success, after_message(message).status_ok {}
pub proof fn late_error_witness()
    ensures after_message(evaluate(seq!['{','x','}'], macro_helper(None))).module,
        after_message(evaluate(seq!['{','x','}'], macro_helper(None))).committed,
        !after_message(evaluate(seq!['{','x','}'], macro_helper(None))).status_ok,
        after_message(evaluate(seq!['$','$','$','O','K'], macro_helper(None))).status_ok {}
}
fn main() {}
