use vstd::prelude::*;
mod attr_evaluator;
use attr_evaluator::*;
verus! {
pub proof fn native_e05() ensures system_expression(SystemName::NamespaceLower,seq!['U', 'S', 'E', 'R'],seq!['/', 'u', 's', 'r', '/', 'i', 'r', 'i', 's', 's', 'y', 's', '/', 'm', 'g', 'r', '/']) == Some(seq!['u', 's', 'e', 'r']) { reveal_with_fuel(lower_text,6); }
pub proof fn native_e23() ensures system_expression(SystemName::ManagerDirectory,seq!['U', 'S', 'E', 'R'],seq!['/', 'u', 's', 'r', '/', 'i', 'r', 'i', 's', 's', 'y', 's', '/', 'm', 'g', 'r', '/']) == Some(seq!['/', 'u', 's', 'r', '/', 'i', 'r', 'i', 's', 's', 'y', 's', '/', 'm', 'g', 'r', '/']) {  }
pub proof fn native_e15() ensures evaluate(seq!['$', '$', '$', 'O', 'K', ' ', 'n', 'o', ' ', 'b', 'r', 'a', 'c', 'e'], macro_helper(None)).value == seq!['$', '$', '$', 'O', 'K', ' ', 'n', 'o', ' ', 'b', 'r', 'a', 'c', 'e'] {  }
pub proof fn native_e20() ensures with_builtins(parameters(map![1int => seq!['9', '.', '9', '.', '9']], Map::empty()), map![1int => seq!['1', '.', '2', '.', '3']])[1] == seq!['1', '.', '2', '.', '3'] {  }
pub proof fn native_e21() ensures name_matches(seq!['a', '.', 'b'], seq!['a', 'x', 'b']), regex_replace_match(seq!['{', '$', 'a', 'x', 'b', '}'],0,6,true,seq!['D', 'O', 'T'], Seq::empty()) == Some(seq!['D', 'O', 'T']) { reveal_with_fuel(name_matches,5); reveal_with_fuel(quote_backslashes,5); reveal_with_fuel(attr_evaluator::env_config::replacement,6); }
pub proof fn native_e32() ensures regex_replace_match(seq!['{', '$', 'p', 'r', 'i', 'c', 'e', '}'],0,8,true,seq!['$', '5'], Seq::empty()).is_none() { reveal_with_fuel(quote_backslashes,4); reveal_with_fuel(attr_evaluator::env_config::replacement,4); }
pub proof fn native_e34() ensures regex_replace_match(seq!['$', '{', 'w', 'i', 'n', '}'],0,6,true,seq!['C', ':', '\\', 'd', 'i', 'r', '\\', 's', 'u', 'b'], Seq::empty()) == Some(seq!['C', ':', '\\', 'd', 'i', 'r', '\\', 's', 'u', 'b']) { full_match_backslash_literal(seq!['$', '{', 'w', 'i', 'n', '}'],seq!['C', ':', '\\', 'd', 'i', 'r', '\\', 's', 'u', 'b'],Seq::empty()); }
pub proof fn native_e30() ensures !resolve_bounded(map![1int => seq![Token::Reference(2)],2int => seq![Token::Reference(1)]],Map::empty(),seq![1int,2int],20).circular_error { declared_cycle_error_is_unreachable(map![1int => seq![Token::Reference(2)],2int => seq![Token::Reference(1)]],Map::empty(),seq![1int,2int],20); }
pub proof fn native_p1() ensures after_message(macro_helper(Some(seq![]))).module, after_message(macro_helper(Some(seq![]))).committed, after_message(macro_helper(Some(seq![]))).history_finalized, after_message(macro_helper(Some(seq![]))).status_ok == true, after_message(macro_helper(Some(seq![]))).history_success == true {  }
pub proof fn native_p2txn() ensures after_message(macro_helper(None)).module, after_message(macro_helper(None)).committed, after_message(macro_helper(None)).history_finalized, after_message(macro_helper(None)).status_ok == false, after_message(macro_helper(None)).history_success == false {  }
pub proof fn native_p3() ensures after_message(macro_helper(Some(seq![]))).module, after_message(macro_helper(Some(seq![]))).committed, after_message(macro_helper(Some(seq![]))).history_finalized, after_message(macro_helper(Some(seq![]))).status_ok == true, after_message(macro_helper(Some(seq![]))).history_success == true {  }
}
fn main(){}
