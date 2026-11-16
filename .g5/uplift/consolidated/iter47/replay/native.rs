#[path="../../../../Verus_verification/consolidated/iter47/formats.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(strip_trailing_lf(yaml_text(leading())) =~= leading())) == 0) by (compute);assert((b(strip_trailing_lf(yaml_text(leading())) =~= leading())) == 0); // yamlf.stripped.TestELeading
assert((b(toon_text(crlf()) =~= crlf())) == 0) by (compute);assert((b(toon_text(crlf()) =~= crlf())) == 0); // toonf.exact.TestDCrLf
assert((toon_rows(0, true) as int) == 2) by (compute);assert((toon_rows(0, true) as int) == 2); // toonf.rows.TestFThrow
assert((b(json_text(punct()) =~= punct())) == 1) by (compute);assert((b(json_text(punct()) =~= punct())) == 1); // jsonf.exact.TestBPunct
assert((b(toon_text(punct()) =~= punct())) == 1) by (compute);assert((b(toon_text(punct()) =~= punct())) == 1); // toonf.exact.TestBPunct
assert((b(strip_trailing_lf(toon_text(punct())) =~= punct())) == 1) by (compute);assert((b(strip_trailing_lf(toon_text(punct())) =~= punct())) == 1); // toonf.stripped.TestBPunct
assert((b(yaml_text(punct()) =~= punct())) == 0) by (compute);assert((b(yaml_text(punct()) =~= punct())) == 0); // yamlf.exact.TestBPunct
assert((b(strip_trailing_lf(yaml_text(punct())) =~= punct())) == 1) by (compute);assert((b(strip_trailing_lf(yaml_text(punct())) =~= punct())) == 1); // yamlf.stripped.TestBPunct
assert((b(json_text(lf()) =~= lf())) == 1) by (compute);assert((b(json_text(lf()) =~= lf())) == 1); // jsonf.exact.TestCLineFeed
assert((b(toon_text(lf()) =~= lf())) == 1) by (compute);assert((b(toon_text(lf()) =~= lf())) == 1); // toonf.exact.TestCLineFeed
assert((b(strip_trailing_lf(toon_text(lf())) =~= lf())) == 1) by (compute);assert((b(strip_trailing_lf(toon_text(lf())) =~= lf())) == 1); // toonf.stripped.TestCLineFeed
assert((b(yaml_text(lf()) =~= lf())) == 0) by (compute);assert((b(yaml_text(lf()) =~= lf())) == 0); // yamlf.exact.TestCLineFeed
assert((b(strip_trailing_lf(yaml_text(lf())) =~= lf())) == 1) by (compute);assert((b(strip_trailing_lf(yaml_text(lf())) =~= lf())) == 1); // yamlf.stripped.TestCLineFeed
assert((b(json_text(crlf()) =~= crlf())) == 1) by (compute);assert((b(json_text(crlf()) =~= crlf())) == 1); // jsonf.exact.TestDCrLf
assert((b(strip_trailing_lf(toon_text(crlf())) =~= crlf())) == 0) by (compute);assert((b(strip_trailing_lf(toon_text(crlf())) =~= crlf())) == 0); // toonf.stripped.TestDCrLf
assert((b(yaml_text(crlf()) =~= crlf())) == 0) by (compute);assert((b(yaml_text(crlf()) =~= crlf())) == 0); // yamlf.exact.TestDCrLf
assert((b(strip_trailing_lf(yaml_text(crlf())) =~= crlf())) == 0) by (compute);assert((b(strip_trailing_lf(yaml_text(crlf())) =~= crlf())) == 0); // yamlf.stripped.TestDCrLf
assert((b(json_text(leading()) =~= leading())) == 1) by (compute);assert((b(json_text(leading()) =~= leading())) == 1); // jsonf.exact.TestELeading
assert((b(toon_text(leading()) =~= leading())) == 1) by (compute);assert((b(toon_text(leading()) =~= leading())) == 1); // toonf.exact.TestELeading
assert((b(strip_trailing_lf(toon_text(leading())) =~= leading())) == 1) by (compute);assert((b(strip_trailing_lf(toon_text(leading())) =~= leading())) == 1); // toonf.stripped.TestELeading
assert((b(yaml_text(leading()) =~= leading())) == 0) by (compute);assert((b(yaml_text(leading()) =~= leading())) == 0); // yamlf.exact.TestELeading
assert((summary_json(Summary { methods: 6, failed: 5 }).methods as int) == 6) by (compute);assert((summary_json(Summary { methods: 6, failed: 5 }).methods as int) == 6); // jsonf.methods
assert((summary_json(Summary { methods: 6, failed: 5 }).failed as int) == 5) by (compute);assert((summary_json(Summary { methods: 6, failed: 5 }).failed as int) == 5); // jsonf.failed
assert((summary_yaml(Summary { methods: 6, failed: 5 }).methods as int) == 6) by (compute);assert((summary_yaml(Summary { methods: 6, failed: 5 }).methods as int) == 6); // yamlf.methods
assert((summary_yaml(Summary { methods: 6, failed: 5 }).failed as int) == 5) by (compute);assert((summary_yaml(Summary { methods: 6, failed: 5 }).failed as int) == 5); // yamlf.failed
assert((summary_toon(Summary { methods: 6, failed: 5 }).methods as int) == 6) by (compute);assert((summary_toon(Summary { methods: 6, failed: 5 }).methods as int) == 6); // toonf.methods
assert((summary_toon(Summary { methods: 6, failed: 5 }).failed as int) == 5) by (compute);assert((summary_toon(Summary { methods: 6, failed: 5 }).failed as int) == 5); // toonf.failed
assert((summary_json(Summary { methods: 6, failed: 5 }).methods as int) == 6) by (compute);assert((summary_json(Summary { methods: 6, failed: 5 }).methods as int) == 6); // jsond.methods
assert((summary_json(Summary { methods: 6, failed: 5 }).failed as int) == 5) by (compute);assert((summary_json(Summary { methods: 6, failed: 5 }).failed as int) == 5); // jsond.failed
assert((summary_yaml(Summary { methods: 6, failed: 5 }).methods as int) == 6) by (compute);assert((summary_yaml(Summary { methods: 6, failed: 5 }).methods as int) == 6); // yamld.methods
assert((summary_yaml(Summary { methods: 6, failed: 5 }).failed as int) == 5) by (compute);assert((summary_yaml(Summary { methods: 6, failed: 5 }).failed as int) == 5); // yamld.failed
assert((summary_toon(Summary { methods: 6, failed: 5 }).methods as int) == 6) by (compute);assert((summary_toon(Summary { methods: 6, failed: 5 }).methods as int) == 6); // toond.methods
assert((summary_toon(Summary { methods: 6, failed: 5 }).failed as int) == 5) by (compute);assert((summary_toon(Summary { methods: 6, failed: 5 }).failed as int) == 5); // toond.failed
}
}
fn main() {}
