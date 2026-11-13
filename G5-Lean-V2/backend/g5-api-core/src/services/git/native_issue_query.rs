//! The native tracker's search text (`g5.issue.native.search`): words and
//! quoted phrases that must all match, `-` or `NOT` to negate, `OR` between
//! two terms, and the `assignee:`, `author:`, `label:`, `no:assignee` and
//! `no:label` qualifiers.
//! Parsing is pure; [`super::native_issues::search`] resolves the people the
//! query names and evaluates it in SQL.

use g5_contracts::error::ExternalIssueErrorKind;

use crate::services::git::github_issues::{reserved_qualifier, IssueError};

/// Longer text is cut, as GitHub domains cut it.
const MAX_QUERY_CHARS: usize = 200;

/// GitHub issue-search qualifiers G5 issues cannot honour: refused, never
/// read as words (`g5.issue.native.search`).
const UNSUPPORTED_QUALIFIERS: &[&str] = &[
    "archived",
    "closed",
    "commenter",
    "comments",
    "created",
    "in",
    "interactions",
    "involves",
    "language",
    "linked",
    "mentions",
    "milestone",
    "parent-issue",
    "project",
    "reactions",
    "reason",
    "sort",
    "sub-issue",
    "team",
    "updated",
];

/// Who a person qualifier names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Person {
    /// `@me`: the caller, or the searching agent's owner.
    Me,
    /// A name or email, lowercased; matched by [`person_matches`].
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// A word or quoted phrase: the stemmed title and description, the
    /// issue's state, or an assignee's name or email.
    Words {
        text: String,
        phrase: bool,
    },
    Assignee(Person),
    Author(Person),
    Unassigned,
    /// Carries one of these labels (lowercased names).
    Label(Vec<String>),
    /// Carries no label.
    Unlabeled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    pub negated: bool,
    pub term: Term,
}

/// Groups that must all hold; the clauses of a group are alternatives
/// (`OR`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeQuery {
    pub groups: Vec<Vec<Clause>>,
}

struct Token {
    negated: bool,
    /// The lowercased qualifier name of `name:value`.
    key: Option<String>,
    value: String,
    /// The token as typed, for a `name:value` read as a word.
    raw: String,
    quoted: bool,
}

fn quoted(chars: &[char], index: &mut usize) -> String {
    *index += 1;
    let start = *index;
    while *index < chars.len() && chars[*index] != '"' {
        *index += 1;
    }
    let value = chars[start..*index].iter().collect();
    if *index < chars.len() {
        *index += 1;
    }
    value
}

fn qualifier_name(word: &str) -> Option<(String, String)> {
    let (key, value) = word.split_once(':')?;
    (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic() || c == '-'))
        .then(|| (key.to_ascii_lowercase(), value.to_string()))
}

fn tokens(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().take(MAX_QUERY_CHARS).collect();
    let mut index = 0;
    let mut out = Vec::new();
    while index < chars.len() {
        if chars[index].is_whitespace() {
            index += 1;
            continue;
        }
        let negated = chars[index] == '-'
            && chars
                .get(index + 1)
                .is_some_and(|next| !next.is_whitespace());
        if negated {
            index += 1;
        }
        if chars[index] == '"' {
            let value = quoted(&chars, &mut index);
            out.push(Token {
                negated,
                key: None,
                raw: value.clone(),
                value,
                quoted: true,
            });
            continue;
        }
        let start = index;
        while index < chars.len() && !chars[index].is_whitespace() && chars[index] != '"' {
            index += 1;
        }
        let word: String = chars[start..index].iter().collect();
        match qualifier_name(&word) {
            Some((key, value)) => {
                let (value, is_quoted) = if value.is_empty() && chars.get(index) == Some(&'"') {
                    (quoted(&chars, &mut index), true)
                } else {
                    (value, false)
                };
                out.push(Token {
                    negated,
                    raw: if is_quoted {
                        format!("{word}{value}")
                    } else {
                        word.clone()
                    },
                    key: Some(key),
                    value,
                    quoted: is_quoted,
                });
            }
            None => out.push(Token {
                negated,
                key: None,
                raw: word.clone(),
                value: word,
                quoted: false,
            }),
        }
    }
    out
}

fn invalid_query(message: impl Into<String>) -> IssueError {
    IssueError::new(ExternalIssueErrorKind::InvalidQuery, message)
}

fn unsupported(qualifier: &str) -> IssueError {
    invalid_query(format!(
        "G5 issues cannot be searched by {qualifier}; use words, assignee:, author:, label:, no:assignee or no:label"
    ))
}

fn person(key: &str, value: &str) -> Result<Person, IssueError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_query(format!(
            "{key}: needs @me, a name or an email"
        )));
    }
    if value.eq_ignore_ascii_case("@me") {
        return Ok(Person::Me);
    }
    Ok(Person::Named(value.trim_start_matches('@').to_lowercase()))
}

/// The term a `name:value` token stands for; `None` for a reserved
/// qualifier, which the State control owns as on GitHub domains.
fn qualified(token: &Token, key: &str) -> Result<Option<Term>, IssueError> {
    if reserved_qualifier(&format!("{key}:")).is_some() {
        return Ok(None);
    }
    let term = match key {
        "assignee" => Term::Assignee(person(key, &token.value)?),
        "author" => Term::Author(person(key, &token.value)?),
        "no" if token.value.eq_ignore_ascii_case("assignee") => Term::Unassigned,
        "no" if token.value.eq_ignore_ascii_case("label") => Term::Unlabeled,
        "label" => Term::Label(label_names(token)?),
        "no" => return Err(unsupported(&format!("no:{}", token.value))),
        key if UNSUPPORTED_QUALIFIERS.contains(&key) => {
            return Err(unsupported(&format!("{key}:")))
        }
        // `Error:`, a URL: ordinary text.
        _ => Term::Words {
            text: token.raw.clone(),
            phrase: token.quoted,
        },
    };
    Ok(Some(term))
}

/// The labels a `label:` names, lowercased: a quoted value is one name, as
/// typed; an unquoted `a,b` names either, as on GitHub.
fn label_names(token: &Token) -> Result<Vec<String>, IssueError> {
    let names: Vec<String> = if token.quoted {
        vec![token.value.trim().to_lowercase()]
    } else {
        token
            .value
            .split(',')
            .map(|name| name.trim().to_lowercase())
            .collect()
    };
    if names.iter().any(String::is_empty) {
        return Err(invalid_query("label: needs a label name"));
    }
    Ok(names)
}

impl NativeQuery {
    pub fn parse(text: &str) -> Result<Self, IssueError> {
        let mut query = Self::default();
        let mut join = false;
        let mut negate_next = false;
        for token in tokens(text) {
            if token.key.is_none() && !token.quoted && !token.negated {
                match token.value.as_str() {
                    "OR" => {
                        join = !query.groups.is_empty();
                        continue;
                    }
                    "AND" => continue,
                    "NOT" => {
                        negate_next = true;
                        continue;
                    }
                    _ => {}
                }
            }
            let term = match &token.key {
                Some(key) => qualified(&token, key)?,
                None if token.value.trim().is_empty() => None,
                None => Some(Term::Words {
                    text: token.value.clone(),
                    phrase: token.quoted,
                }),
            };
            let negated = token.negated != negate_next;
            negate_next = false;
            let Some(term) = term else {
                join = false;
                continue;
            };
            let clause = Clause { negated, term };
            match query.groups.last_mut() {
                Some(group) if join => group.push(clause),
                _ => query.groups.push(vec![clause]),
            }
            join = false;
        }
        Ok(query)
    }

    /// Groups of words only. The title-fragment and key/number alternatives
    /// stand in for these; groups with a qualifier always filter.
    pub fn is_text_group(group: &[Clause]) -> bool {
        group
            .iter()
            .all(|clause| matches!(clause.term, Term::Words { .. }))
    }

    /// The words of single, positive word groups as typed: what a title
    /// fragment or an issue key/number is matched against.
    pub fn free_text(&self) -> String {
        self.groups
            .iter()
            .filter_map(|group| match group.as_slice() {
                [Clause {
                    negated: false,
                    term: Term::Words { text, .. },
                }] => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        self.groups.iter().flatten().map(|clause| &clause.term)
    }
}

/// `Open`/`closed` as a word names the issue's state.
pub fn state_word(text: &str) -> Option<&'static str> {
    if text.eq_ignore_ascii_case("open") {
        Some("open")
    } else if text.eq_ignore_ascii_case("closed") {
        Some("closed")
    } else {
        None
    }
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
}

/// A person `needle` (lowercased) names: their whole email or display name,
/// or, from two characters, the start of a word of either (`jan` for
/// `Jane Doe` or `jane.doe@example.com`) or of the whole name.
pub fn person_matches(needle: &str, email: &str, display_name: &str) -> bool {
    let needle = needle.trim_matches(|c: char| !c.is_alphanumeric());
    if needle.is_empty() {
        return false;
    }
    let email = email.to_lowercase();
    let name = display_name.to_lowercase();
    if email == needle || name == needle {
        return true;
    }
    if needle.chars().count() < 2 {
        return false;
    }
    let local = email.split('@').next().unwrap_or_default();
    local.starts_with(needle)
        || name.starts_with(needle)
        || words(local).any(|word| word.starts_with(needle))
        || words(&name).any(|word| word.starts_with(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str) -> Term {
        Term::Words {
            text: text.to_string(),
            phrase: false,
        }
    }

    fn clause(negated: bool, term: Term) -> Clause {
        Clause { negated, term }
    }

    #[test]
    fn words_phrases_negation_and_or_group() {
        let query = NativeQuery::parse(r#"crash "on login" -flaky a OR b NOT c"#).unwrap();
        assert_eq!(
            query.groups,
            vec![
                vec![clause(false, word("crash"))],
                vec![clause(
                    false,
                    Term::Words {
                        text: "on login".to_string(),
                        phrase: true
                    }
                )],
                vec![clause(true, word("flaky"))],
                vec![clause(false, word("a")), clause(false, word("b"))],
                vec![clause(true, word("c"))],
            ]
        );
        assert_eq!(query.free_text(), "crash on login");
    }

    #[test]
    fn person_qualifiers_and_reserved_ones() {
        let query = NativeQuery::parse(
            r#"assignee:@me -author:"Jane Doe" no:assignee is:open state:closed repo:x/y"#,
        )
        .unwrap();
        assert_eq!(
            query.groups,
            vec![
                vec![clause(false, Term::Assignee(Person::Me))],
                vec![clause(
                    true,
                    Term::Author(Person::Named("jane doe".to_string()))
                )],
                vec![clause(false, Term::Unassigned)],
            ]
        );
        assert_eq!(
            NativeQuery::parse("assignee:@Jane").unwrap().groups,
            vec![vec![clause(
                false,
                Term::Assignee(Person::Named("jane".to_string()))
            )]]
        );
    }

    #[test]
    fn unsupported_qualifiers_are_refused_and_other_colons_are_words() {
        for text in [
            "-milestone:v1",
            "in:title",
            "no:milestone",
            "assignee:",
            "label:",
            "label:a,,b",
        ] {
            let error = NativeQuery::parse(text).unwrap_err();
            assert_eq!(error.kind, ExternalIssueErrorKind::InvalidQuery, "{text}");
        }
        let query = NativeQuery::parse("Error: https://g5.example/acme/issues/g5/2").unwrap();
        assert_eq!(
            query.groups,
            vec![
                vec![clause(false, word("Error:"))],
                vec![clause(false, word("https://g5.example/acme/issues/g5/2"))],
            ]
        );
    }

    #[test]
    fn labels_are_named_as_on_github() {
        let query =
            NativeQuery::parse(r#"label:Bug -label:"priority: p0" label:wip,Blocked no:label"#)
                .unwrap();
        assert_eq!(
            query.groups,
            vec![
                vec![clause(false, Term::Label(vec!["bug".to_string()]))],
                vec![clause(true, Term::Label(vec!["priority: p0".to_string()]))],
                vec![clause(
                    false,
                    Term::Label(vec!["wip".to_string(), "blocked".to_string()])
                )],
                vec![clause(false, Term::Unlabeled)],
            ]
        );
        assert!(!NativeQuery::is_text_group(&query.groups[0]));
    }

    #[test]
    fn text_groups_and_free_text() {
        let query = NativeQuery::parse("ACME-12 assignee:jane").unwrap();
        assert!(NativeQuery::is_text_group(&query.groups[0]));
        assert!(!NativeQuery::is_text_group(&query.groups[1]));
        assert_eq!(query.free_text(), "ACME-12");
        assert_eq!(NativeQuery::parse("").unwrap(), NativeQuery::default());
        assert_eq!(
            NativeQuery::parse("OR AND").unwrap(),
            NativeQuery::default()
        );
    }

    #[test]
    fn people_match_by_name_or_email() {
        let matches = |needle: &str| person_matches(needle, "jane.doe@example.com", "Jane Doe");
        for needle in [
            "jane",
            "ja",
            "doe",
            "jane doe",
            "jane.doe@example.com",
            "jane,",
        ] {
            assert!(matches(needle), "{needle}");
        }
        for needle in ["j", "ane", "smith", "example"] {
            assert!(!matches(needle), "{needle}");
        }
        assert!(person_matches("b", "b@example.com", "B"));
    }

    #[test]
    fn state_words() {
        assert_eq!(state_word("Closed"), Some("closed"));
        assert_eq!(state_word("opened"), None);
    }
}
