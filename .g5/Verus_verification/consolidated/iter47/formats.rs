#![allow(non_snake_case)]
// Iteration47: test report formats (JSON, YAML, TOON) rendering the same result tree.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter47/bindings.json):
//   Test/Abstract.cls::BuildResultTree, GetSummary, ToFile   one shared tree and summary for every format
//   Test/JsonOutput.cls::WriteToFileStream                   %ToJSON of the tree (text escaped, exact)
//   Test/YamlOutput.cls::BuildYaml, BuildYamlRows, OutputToDevice, WriteToFileStream
//                                                            descriptions as "|" block scalars
//   Test/ToonOutput.cls::BuildToonRows, EscapeToonField, OutputToDevice, WriteToFileStream
//                                                            RFC 4180 quoted rows; an error row plus a
//                                                            status row for a method without assertions
//
// A message is a sequence of character codes. Premises (native-qualified, observed at the pin): a raw
// CR in a description does not reach the YAML or TOON file (the stream copy drops it; JSON escapes it
// as text first), and a YAML "|" block scalar reads back with its first-line indentation removed and
// one trailing newline ("clip"), as parsed by PyYAML 6.0.1 on the host.
use vstd::prelude::*;

verus! {

pub open spec fn CR() -> int { 13 }
pub open spec fn LF() -> int { 10 }
pub open spec fn SP() -> int { 32 }

pub open spec fn drop_cr(m: Seq<int>) -> Seq<int>
    decreases m.len()
{
    if m.len() == 0 { seq![] } else if m[0] == CR() { drop_cr(m.drop_first()) } else { seq![m[0]] + drop_cr(m.drop_first()) }
}
pub open spec fn drop_leading_spaces(m: Seq<int>) -> Seq<int>
    decreases m.len()
{
    if m.len() > 0 && m[0] == SP() { drop_leading_spaces(m.drop_first()) } else { m }
}

pub open spec fn json_text(m: Seq<int>) -> Seq<int> { m }
pub open spec fn toon_text(m: Seq<int>) -> Seq<int> { drop_cr(m) }
pub open spec fn yaml_text(m: Seq<int>) -> Seq<int> { drop_leading_spaces(drop_cr(m)).push(LF()) }
// Reader comparison used by the native observer: exact, and exact after removing trailing newlines.
pub open spec fn strip_trailing_lf(m: Seq<int>) -> Seq<int>
    decreases m.len()
{
    if m.len() > 0 && m.last() == LF() { strip_trailing_lf(m.drop_last()) } else { m }
}
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// TOON rows for one method.
pub open spec fn toon_rows(asserts: nat, has_error: bool) -> nat {
    asserts + (if has_error { 1nat } else { 0nat }) + (if asserts == 0 { 1nat } else { 0nat })
}

// Shared summary: every format prints the same method counts.
pub struct Summary { pub methods: nat, pub failed: nat }
pub open spec fn summary_json(s: Summary) -> Summary { s }
pub open spec fn summary_yaml(s: Summary) -> Summary { s }
pub open spec fn summary_toon(s: Summary) -> Summary { s }

pub proof fn json_exact(m: Seq<int>) ensures json_text(m) == m {}
pub proof fn summaries_agree(s: Summary)
    ensures summary_json(s) == summary_yaml(s), summary_yaml(s) == summary_toon(s)
{}
pub proof fn error_method_two_toon_rows() ensures toon_rows(0, true) == 2, toon_rows(2, false) == 2 {}

// Fixture messages.
pub open spec fn punct() -> Seq<int> { seq![104, 97, 115, 58, 32, 34, 113, 34, 44, 32, 35] }      // has: "q", #
pub open spec fn lf() -> Seq<int> { seq![102, 10, 115] }                                            // f\ns
pub open spec fn crlf() -> Seq<int> { seq![111, 13, 10, 116] }                                      // o\r\nt
pub open spec fn leading() -> Seq<int> { seq![32, 32, 108] }                                        // "  l"

pub proof fn fixture_rendering()
    ensures
        toon_text(punct()) == punct(), toon_text(lf()) == lf(), toon_text(crlf()) != crlf(), toon_text(leading()) == leading(),
        yaml_text(punct()) != punct(), strip_trailing_lf(yaml_text(punct())) == punct(),
        strip_trailing_lf(yaml_text(lf())) == lf(),
        strip_trailing_lf(yaml_text(crlf())) != crlf(),
        strip_trailing_lf(yaml_text(leading())) != leading(),
{
    assert(toon_text(punct()) =~= punct()) by (compute);
    assert(toon_text(lf()) =~= lf()) by (compute);
    assert(toon_text(crlf()) =~= seq![111, 10, 116]) by (compute);
    assert(toon_text(leading()) =~= leading()) by (compute);
    assert(yaml_text(punct()).len() == punct().len() + 1) by (compute);
    assert(strip_trailing_lf(yaml_text(punct())) =~= punct()) by (compute);
    assert(strip_trailing_lf(yaml_text(lf())) =~= lf()) by (compute);
    assert(strip_trailing_lf(yaml_text(crlf())) =~= seq![111, 10, 116]) by (compute);
    assert(strip_trailing_lf(yaml_text(leading())) =~= seq![108]) by (compute);
}

} // verus!

fn main() {}
