//! Iteration-75 neighbours for the anonymous Artifactory filter parser (artifact.filter#1-#4).
//!
//! The deciding computation is the inherited iter3 `parse_fields`, `parse_field` and `condition`, which
//! follow ResourceProcessor/ArtifactoryTarball.cls ParseAnonymousFilter (lines 323-347) and
//! ParseCondition (lines 351-375). This file adds the cases the inherited `filter_boundaries` never
//! exercised: a property-only filter, an accepted type=file, an array condition and an unknown key beside
//! a valid name.
#[path = "../../consolidated/iter3/artifacts.rs"]
mod artifacts;

use vstd::prelude::*;
use artifacts::*;

verus! {

// "file" as character codes, as in the inherited model.
pub open spec fn file_text() -> Text { seq![102int, 105, 108, 101] }

/// artifact.filter#1. ArtifactoryTarball.cls:344-346 refuses only when criteria.name is "" and no
/// property was set, so a lone @property is enough, and a lone repo is not.
pub proof fn property_alone_is_enough() {
    reveal_with_fuel(parse_fields, 3);
    let prop = (Key::Property(seq![99int]), Condition::Scalar(seq![1int]));
    let repo = (Key::Repo, Condition::Scalar(seq![7int]));
    let name = (Key::Name, Condition::Scalar(seq![5int]));
    assert(parse_fields(seq![prop], empty_criteria()).is_some());
    assert(parse_fields(seq![repo, prop], empty_criteria()).is_some());
    assert(parse_fields(seq![name], empty_criteria()).is_some());
    assert(parse_fields(seq![repo], empty_criteria()).is_none());
    assert(parse_fields(seq![], empty_criteria()).is_none());
}

/// artifact.filter#2. An unknown key throws at ArtifactoryTarball.cls:341-342 while the keys are read,
/// so a valid name before or after it does not rescue the filter. An array condition throws at
/// ArtifactoryTarball.cls:362-364 (`'condition.%IsA("%Library.DynamicObject")`).
pub proof fn unsupported_key_or_operator_fails() {
    reveal_with_fuel(parse_fields, 3);
    let name = (Key::Name, Condition::Scalar(seq![5int]));
    let other = (Key::Other, Condition::Scalar(seq![1int]));
    assert(parse_fields(seq![name, other], empty_criteria()).is_none());
    assert(parse_fields(seq![other, name], empty_criteria()).is_none());
    assert(parse_fields(seq![(Key::Name, Condition::Array)], empty_criteria()).is_none());
    assert(parse_fields(seq![name, (Key::Property(seq![99int]), Condition::Array)], empty_criteria()).is_none());
    // The supported neighbour of each.
    assert(parse_fields(seq![name, (Key::Property(seq![99int]), Condition::Scalar(seq![1int]))], empty_criteria()).is_some());
}

/// artifact.filter#3. The five allowed keys together parse, and type must be "file"
/// (ArtifactoryTarball.cls:334-337), given plainly or through $eq.
pub proof fn allowed_keys_and_type_file() {
    reveal_with_fuel(parse_fields, 6);
    let name = (Key::Name, Condition::Scalar(seq![5int]));
    let all = seq![(Key::Repo, Condition::Scalar(seq![7int])), name,
        (Key::Path, Condition::Scalar(seq![8int])), (Key::Type, Condition::Scalar(file_text())),
        (Key::Property(seq![99int]), Condition::Scalar(seq![1int]))];
    assert(parse_fields(all, empty_criteria()).is_some());
    assert(parse_fields(seq![(Key::Type, Condition::Scalar(file_text())), name], empty_criteria()).is_some());
    assert(parse_fields(seq![(Key::Type, Condition::Object(seq![(Op::Eq, Some(file_text()))])), name],
        empty_criteria()).is_some());
    assert(parse_fields(seq![(Key::Type, Condition::Scalar(seq![100int])), name], empty_criteria()).is_none());
    // type=file adds no criterion: the parsed criteria equal those of the name alone.
    assert(parse_fields(seq![(Key::Type, Condition::Scalar(file_text())), name], empty_criteria())
        == parse_fields(seq![name], empty_criteria()));
}

/// artifact.filter#4. Each condition shape on both sides: a plain value and one $eq are accepted for
/// every key, $match only for name and path, and arrays, empty or multi-operator objects and an object
/// operand are refused (ArtifactoryTarball.cls:358-374).
pub proof fn condition_shapes() {
    let v = seq![1int];
    assert(condition(Condition::Scalar(v), false) == Some(Pattern { value: v, wildcard: false }));
    assert(condition(Condition::Object(seq![(Op::Eq, Some(v))]), false) == Some(Pattern { value: v, wildcard: false }));
    assert(condition(Condition::Object(seq![(Op::Match, Some(v))]), true) == Some(Pattern { value: v, wildcard: true }));
    assert(condition(Condition::Object(seq![(Op::Match, Some(v))]), false).is_none());
    assert(condition(Condition::Object(seq![(Op::Other, Some(v))]), true).is_none());
    assert(condition(Condition::Object(seq![]), true).is_none());
    assert(condition(Condition::Object(seq![(Op::Eq, Some(v)), (Op::Match, Some(v))]), true).is_none());
    assert(condition(Condition::Object(seq![(Op::Eq, None)]), true).is_none());
    assert(condition(Condition::Array, true).is_none());
    assert(condition(Condition::Array, false).is_none());
    assert(parse_field(empty_criteria(), Key::Path, Condition::Object(seq![(Op::Match, Some(v))])).is_some());
    assert(parse_field(empty_criteria(), Key::Property(v), Condition::Object(seq![(Op::Match, Some(v))])).is_none());
    assert(parse_field(empty_criteria(), Key::Type, Condition::Object(seq![(Op::Match, Some(file_text()))])).is_none());
}

fn main() {}

} // verus!
