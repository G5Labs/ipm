//! Shared parsing for the source guards in this directory: which Rust files
//! and items compile into a production build, and the values of string
//! literals.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};

pub fn parse_rust(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Rust grammar");
    parser.parse(source, None).expect("parse Rust source")
}

pub fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// The attribute items written directly before `node`, skipping comments.
pub fn item_attributes<'a>(node: Node<'_>, source: &'a str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut previous = node.prev_named_sibling();
    while let Some(sibling) = previous {
        match sibling.kind() {
            "attribute_item" => {
                found.push(
                    sibling
                        .utf8_text(source.as_bytes())
                        .expect("UTF-8 attribute"),
                );
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        previous = sibling.prev_named_sibling();
    }
    found
}

/// A `cfg` predicate evaluated for a production build: `test` is off,
/// features whose name contains `test` are off, and every other flag may be
/// either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cfg {
    On,
    Off,
    Unknown,
}

/// True when `attribute` is a `#[cfg(...)]` that is off in every production
/// build, such as `cfg(test)` or `cfg(any(test, feature = "test-helpers"))`.
/// `cfg(any(test, unix))` is not: it compiles on Unix.
pub fn excludes_production(attribute: &str) -> bool {
    let compact: String = attribute.split_whitespace().collect();
    let Some(predicate) = compact
        .strip_prefix("#[cfg(")
        .and_then(|rest| rest.strip_suffix(")]"))
    else {
        return false;
    };
    matches!(evaluate(predicate), Some(Cfg::Off))
}

pub fn any_excludes_production(attributes: &[&str]) -> bool {
    attributes
        .iter()
        .any(|attribute| excludes_production(attribute))
}

fn evaluate(predicate: &str) -> Option<Cfg> {
    let (value, rest) = cfg_predicate(predicate)?;
    rest.is_empty().then_some(value)
}

fn cfg_predicate(input: &str) -> Option<(Cfg, &str)> {
    let name_end = input
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(input.len());
    let (name, rest) = input.split_at(name_end);
    if name.is_empty() {
        return None;
    }
    if let Some(rest) = rest.strip_prefix('=') {
        let rest = rest.strip_prefix('"')?;
        let close = rest.find('"')?;
        let value = &rest[..close];
        let result = if name == "feature" && value.contains("test") {
            Cfg::Off
        } else {
            Cfg::Unknown
        };
        return Some((result, &rest[close + 1..]));
    }
    if let Some(mut rest) = rest.strip_prefix('(') {
        let mut values = Vec::new();
        loop {
            if let Some(after) = rest.strip_prefix(')') {
                rest = after;
                break;
            }
            let (value, after) = cfg_predicate(rest)?;
            values.push(value);
            rest = after.strip_prefix(',').unwrap_or(after);
        }
        let combined = match (name, values.as_slice()) {
            ("all", values) => {
                if values.contains(&Cfg::Off) {
                    Cfg::Off
                } else if values.iter().all(|value| *value == Cfg::On) {
                    Cfg::On
                } else {
                    Cfg::Unknown
                }
            }
            ("any", values) => {
                if values.contains(&Cfg::On) {
                    Cfg::On
                } else if values.iter().all(|value| *value == Cfg::Off) {
                    Cfg::Off
                } else {
                    Cfg::Unknown
                }
            }
            ("not", [value]) => match value {
                Cfg::On => Cfg::Off,
                Cfg::Off => Cfg::On,
                Cfg::Unknown => Cfg::Unknown,
            },
            _ => return None,
        };
        return Some((combined, rest));
    }
    let result = if name == "test" {
        Cfg::Off
    } else {
        Cfg::Unknown
    };
    Some((result, rest))
}

/// The value of a Rust string literal as written in source: escapes decoded
/// for ordinary strings, raw strings taken as written.
pub fn string_value(literal: &str) -> String {
    let body = literal.strip_prefix('b').unwrap_or(literal);
    if let Some(raw) = body.strip_prefix('r') {
        let hashes = raw.chars().take_while(|c| *c == '#').count();
        let quoted = &raw[hashes..];
        let inner = quoted.strip_prefix('"').unwrap_or(quoted);
        let end = inner.len().saturating_sub(1 + hashes);
        return inner[..end].to_string();
    }
    let inner = body
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(body);
    let mut value = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            value.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => value.push('\n'),
            Some('r') => value.push('\r'),
            Some('t') => value.push('\t'),
            Some('0') => value.push('\0'),
            Some(escaped @ ('\\' | '"' | '\'')) => value.push(escaped),
            Some('\n' | '\r') => {
                while chars.peek().is_some_and(|next| next.is_whitespace()) {
                    chars.next();
                }
            }
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                if let Some(decoded) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    value.push(decoded);
                }
            }
            Some('u') => {
                let hex: String = chars
                    .by_ref()
                    .skip_while(|next| *next == '{')
                    .take_while(|next| *next != '}')
                    .collect();
                if let Some(decoded) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    value.push(decoded);
                }
            }
            Some(other) => {
                value.push('\\');
                value.push(other);
            }
            None => value.push('\\'),
        }
    }
    value
}

/// An out-of-line `mod name;` declaration: its name, the inline modules
/// enclosing it, the files it can resolve to and whether it compiles only in
/// tests.
#[derive(Debug)]
pub struct ModuleDeclaration {
    pub name: String,
    pub inline: Vec<String>,
    pub candidates: Vec<PathBuf>,
    pub test_only: bool,
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

fn module_candidates(
    declaring_file: &Path,
    inline_modules: &[String],
    module: &str,
    path_attribute: Option<&str>,
) -> Vec<PathBuf> {
    let directory = declaring_file
        .parent()
        .expect("source file has a directory");
    let stem = declaring_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let mut base = if matches!(stem, "mod" | "lib" | "main") {
        directory.to_path_buf()
    } else {
        directory.join(stem)
    };
    for inline in inline_modules {
        base.push(inline);
    }
    match path_attribute {
        Some(path) if inline_modules.is_empty() => vec![normalize(&directory.join(path))],
        Some(path) => vec![normalize(&base.join(path))],
        None => vec![
            normalize(&base.join(format!("{module}.rs"))),
            normalize(&base.join(module).join("mod.rs")),
        ],
    }
}

fn path_attribute(attributes: &[&str]) -> Option<String> {
    attributes.iter().find_map(|attribute| {
        let compact: String = attribute.split_whitespace().collect();
        compact
            .strip_prefix("#[path=\"")
            .and_then(|rest| rest.strip_suffix("\"]"))
            .map(str::to_owned)
    })
}

/// Every out-of-line module declared in `source`, including those inside
/// inline modules and test-only items.
pub fn module_declarations(path: &Path, source: &str, tree: &Tree) -> Vec<ModuleDeclaration> {
    fn visit(
        node: Node<'_>,
        path: &Path,
        source: &str,
        inline_modules: &mut Vec<String>,
        inherited_test_only: bool,
        found: &mut Vec<ModuleDeclaration>,
    ) {
        let attributes = item_attributes(node, source);
        let test_only = inherited_test_only || any_excludes_production(&attributes);
        if node.kind() == "mod_item" {
            // `mod r#task_service;` declares the module `task_service`.
            let name = node
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source.as_bytes()).ok())
                .unwrap_or_default()
                .trim_start_matches("r#")
                .to_string();
            match node.child_by_field_name("body") {
                None => {
                    let path_attribute = path_attribute(&attributes);
                    let candidates =
                        module_candidates(path, inline_modules, &name, path_attribute.as_deref());
                    found.push(ModuleDeclaration {
                        name,
                        inline: inline_modules.clone(),
                        candidates,
                        test_only,
                    });
                }
                Some(body) => {
                    inline_modules.push(name);
                    visit(body, path, source, inline_modules, test_only, found);
                    inline_modules.pop();
                }
            }
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, path, source, inline_modules, test_only, found);
        }
    }

    let mut found = Vec::new();
    visit(
        tree.root_node(),
        path,
        source,
        &mut Vec::new(),
        false,
        &mut found,
    );
    found
}

/// Files that compile only in tests: out-of-line modules declared test-only,
/// and every module file declared beneath them.
pub fn test_only_files(
    declarations: &BTreeMap<PathBuf, Vec<ModuleDeclaration>>,
) -> BTreeSet<PathBuf> {
    let mut files: BTreeSet<PathBuf> = declarations
        .values()
        .flatten()
        .filter(|declaration| declaration.test_only)
        .flat_map(|declaration| declaration.candidates.iter().cloned())
        .collect();
    loop {
        let before = files.len();
        let current: Vec<PathBuf> = files.iter().cloned().collect();
        for file in current {
            if let Some(children) = declarations.get(&file) {
                for child in children {
                    files.extend(child.candidates.iter().cloned());
                }
            }
        }
        if files.len() == before {
            return files;
        }
    }
}

/// Production Rust files under `roots`, each with its source and parse tree.
pub fn production_sources(roots: &[PathBuf]) -> Vec<(PathBuf, String, Tree)> {
    let mut files = Vec::new();
    for root in roots {
        collect_rust_files(root, &mut files);
    }
    files.sort();
    let mut parsed = Vec::new();
    let mut declarations = BTreeMap::new();
    for path in files {
        let source = fs::read_to_string(&path).expect("read Rust source file");
        let tree = parse_rust(&source);
        declarations.insert(path.clone(), module_declarations(&path, &source, &tree));
        parsed.push((path, source, tree));
    }
    let test_only = test_only_files(&declarations);
    parsed
        .into_iter()
        .filter(|(path, _, _)| !test_only.contains(path))
        .collect()
}

#[test]
fn cfg_predicates_are_evaluated_for_a_production_build() {
    for (attribute, excluded) in [
        ("#[cfg(test)]", true),
        ("#[cfg(any(test, feature = \"test-helpers\"))]", true),
        (
            "#[cfg(feature = \"execution-lifecycle-test-support\")]",
            true,
        ),
        ("#[cfg(all(test, unix))]", true),
        ("#[cfg(not(not(test)))]", true),
        ("#[cfg(any(test, unix))]", false),
        ("#[cfg(not(test))]", false),
        ("#[cfg(not(any(test, unix)))]", false),
        ("#[cfg(feature = \"dev\")]", false),
        ("#[cfg(unix)]", false),
        ("#[derive(Debug)]", false),
        ("#[cfg(test", false),
    ] {
        assert_eq!(excludes_production(attribute), excluded, "{attribute}");
    }
}

#[test]
fn string_values_decode_ordinary_escapes_and_keep_raw_strings() {
    assert_eq!(
        string_value(r#""UPDATE \"tasks\" SET name = $1""#),
        "UPDATE \"tasks\" SET name = $1"
    );
    assert_eq!(
        string_value("\"UPDATE \\\n        tasks SET\""),
        "UPDATE tasks SET"
    );
    assert_eq!(string_value(r#""a\tb\u{41}\x42""#), "a\tbAB");
    assert_eq!(
        string_value(r##"r#"UPDATE "tasks" SET \n"#"##),
        "UPDATE \"tasks\" SET \\n"
    );
    assert_eq!(string_value(r#"b"UPDATE tasks SET""#), "UPDATE tasks SET");
}

#[test]
fn test_only_status_reaches_nested_and_inline_module_files() {
    let root = Path::new("/crate/src");
    let lib = root.join("lib.rs");
    let lib_source = r#"
        mod services;
        #[cfg(test)]
        mod tests;
        mod outer {
            #[cfg(test)]
            mod probe;
            mod kept;
        }
        #[cfg(test)]
        #[path = "../fixtures/shared.rs"]
        mod shared;
    "#;
    let tests_source = "mod fixture;";
    let mut declarations = BTreeMap::new();
    declarations.insert(
        lib.clone(),
        module_declarations(&lib, lib_source, &parse_rust(lib_source)),
    );
    let tests_file = root.join("tests.rs");
    declarations.insert(
        tests_file.clone(),
        module_declarations(&tests_file, tests_source, &parse_rust(tests_source)),
    );
    let test_only = test_only_files(&declarations);
    for expected in [
        "/crate/src/tests.rs",
        "/crate/src/tests/fixture.rs",
        "/crate/src/outer/probe.rs",
        "/crate/fixtures/shared.rs",
    ] {
        assert!(
            test_only.contains(Path::new(expected)),
            "{expected} is test-only"
        );
    }
    for production in ["/crate/src/services.rs", "/crate/src/outer/kept.rs"] {
        assert!(
            !test_only.contains(Path::new(production)),
            "{production} is production"
        );
    }
}

#[test]
fn raw_module_declarations_are_normalized() {
    let lib = Path::new("/crate/src/lib.rs");
    let source =
        "#[path = \"task_service.rs\"]\npub mod r#task_service;\nmod r#outer { mod r#inner; }\n";
    let found = module_declarations(lib, source, &parse_rust(source));
    assert_eq!(found[0].name, "task_service");
    assert!(found[0].inline.is_empty());
    assert_eq!(
        found[0].candidates,
        vec![PathBuf::from("/crate/src/task_service.rs")]
    );
    assert_eq!(found[1].name, "inner");
    assert_eq!(found[1].inline, vec!["outer".to_string()]);
    assert_eq!(
        found[1].candidates,
        vec![
            PathBuf::from("/crate/src/outer/inner.rs"),
            PathBuf::from("/crate/src/outer/inner/mod.rs"),
        ]
    );
}
