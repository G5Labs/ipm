use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use super::anchor_types::{CodeElementKind, ParsedCodeBlock};
use super::language_profiles::{self, LanguageProfile};

/// Extracts named code elements from source files using tree-sitter.
pub struct CodeAnchorExtractor {
    profiles: Vec<Box<dyn LanguageProfile>>,
}

impl CodeAnchorExtractor {
    pub fn new() -> Self {
        Self {
            profiles: language_profiles::default_profiles(),
        }
    }

    /// Extract all named code elements from a source file.
    /// Selects the appropriate `LanguageProfile` based on file extension.
    /// Returns an empty vec for unsupported extensions.
    pub fn extract(&self, content: &str, extension: &str) -> Vec<ParsedCodeBlock> {
        let ext = extension.strip_prefix('.').unwrap_or(extension);
        let profile = match self.profiles.iter().find(|p| p.extensions().contains(&ext)) {
            Some(p) => p,
            None => return vec![],
        };

        let language = profile.language();
        let mut parser = Parser::new();
        if parser.set_language(&language).is_err() {
            return vec![];
        }

        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return vec![],
        };

        let query = match Query::new(&language, profile.anchors_query()) {
            Ok(q) => q,
            Err(_) => return vec![],
        };

        let name_idx = match query.capture_index_for_name("name") {
            Some(idx) => idx,
            None => return vec![],
        };
        let def_idx = query.capture_index_for_name("def");
        let kind_capture_indices: Vec<(u32, CodeElementKind)> = [
            ("function", CodeElementKind::Function),
            ("class", CodeElementKind::Class),
            ("interface", CodeElementKind::Interface),
            ("type_alias", CodeElementKind::TypeAlias),
            ("enum", CodeElementKind::Enum),
            ("const", CodeElementKind::Const),
        ]
        .into_iter()
        .filter_map(|(capture_name, kind)| {
            query
                .capture_index_for_name(capture_name)
                .map(|capture_idx| (capture_idx, kind))
        })
        .collect();

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

        let mut results = Vec::new();

        while let Some(m) = matches.next() {
            let mut name: Option<&str> = None;
            let mut def_node: Option<Node<'_>> = None;
            let mut def_range: Option<std::ops::Range<usize>> = None;
            let mut def_start_line: Option<usize> = None;
            let mut kind_from_capture: Option<CodeElementKind> = None;

            for capture in m.captures {
                if capture.index == name_idx {
                    name = Some(&content[capture.node.byte_range()]);
                }
                if let Some(di) = def_idx {
                    if capture.index == di {
                        def_node = Some(capture.node);
                        def_range = Some(capture.node.byte_range());
                        def_start_line = Some(capture.node.start_position().row + 1);
                    }
                }
                if kind_from_capture.is_none() {
                    for (capture_idx, kind) in &kind_capture_indices {
                        if capture.index == *capture_idx {
                            kind_from_capture = Some(*kind);
                            break;
                        }
                    }
                }
            }

            if let Some(name_str) = name {
                let kind = kind_from_capture
                    .unwrap_or_else(|| infer_kind_from_query(m.pattern_index, ext));
                let block_name = canonical_code_block_name(ext, name_str, kind, def_node, content);
                let byte_range = def_range.unwrap_or(0..0);
                let line_number = def_start_line.unwrap_or(1);

                results.push(ParsedCodeBlock {
                    name: block_name,
                    kind,
                    line_number,
                    byte_range,
                });
            }
        }

        results
    }
}

fn canonical_code_block_name(
    ext: &str,
    name: &str,
    kind: CodeElementKind,
    def_node: Option<Node<'_>>,
    content: &str,
) -> String {
    if ext == "rs" && kind == CodeElementKind::Function {
        if let Some(owner) = def_node.and_then(|node| rust_impl_owner_for_function(node, content)) {
            return format!("{owner}::{name}");
        }
    }

    name.to_string()
}

fn rust_impl_owner_for_function(function_node: Node<'_>, content: &str) -> Option<String> {
    if function_node.kind() != "function_item" {
        return None;
    }

    let mut ancestor = function_node.parent();
    while let Some(node) = ancestor {
        match node.kind() {
            "impl_item" => return rust_impl_owner_name(node, content),
            // A nested function inside a method is not itself an impl method.
            "function_item" => return None,
            _ => ancestor = node.parent(),
        }
    }

    None
}

fn rust_impl_owner_name(impl_node: Node<'_>, content: &str) -> Option<String> {
    let type_node = impl_node.child_by_field_name("type")?;
    let type_text = content.get(type_node.byte_range())?;
    rust_type_anchor_name(type_text)
}

fn rust_type_anchor_name(type_text: &str) -> Option<String> {
    let mut without_generics = String::new();
    let mut generic_depth = 0usize;

    for ch in type_text.chars() {
        match ch {
            '<' => generic_depth += 1,
            '>' if generic_depth > 0 => generic_depth -= 1,
            _ if generic_depth == 0 => without_generics.push(ch),
            _ => {}
        }
    }

    let mut current = String::new();
    let mut candidates = Vec::new();
    for ch in without_generics.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            candidates.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        candidates.push(current);
    }

    candidates
        .into_iter()
        .rev()
        .find(|candidate| candidate != "dyn" && candidate != "impl" && candidate != "for")
}

impl Default for CodeAnchorExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Infer `CodeElementKind` from the query pattern index and language extension.
///
/// The pattern index corresponds to the order of patterns in the S-expression
/// query string for each language profile.
fn infer_kind_from_query(pattern_index: usize, ext: &str) -> CodeElementKind {
    match ext {
        "rs" => match pattern_index {
            0 => CodeElementKind::Function,
            1 => CodeElementKind::Struct,
            2 => CodeElementKind::Enum,
            3 => CodeElementKind::Trait,
            4 => CodeElementKind::Impl,
            5 => CodeElementKind::Const,
            6 => CodeElementKind::TypeAlias,
            _ => CodeElementKind::Function,
        },
        "py" => match pattern_index {
            0 => CodeElementKind::Function,
            1 => CodeElementKind::Class,
            _ => CodeElementKind::Function,
        },
        "ts" | "tsx" => match pattern_index {
            0 | 1 => CodeElementKind::Function,
            2 | 3 => CodeElementKind::Class,
            4 | 5 => CodeElementKind::Interface,
            6 | 7 => CodeElementKind::TypeAlias,
            8 | 9 => CodeElementKind::Enum,
            10 | 11 => CodeElementKind::Const,
            _ => CodeElementKind::Function,
        },
        "js" | "jsx" | "mjs" => match pattern_index {
            0 => CodeElementKind::Function,
            1 => CodeElementKind::Class,
            2 => CodeElementKind::Const,
            _ => CodeElementKind::Function,
        },
        "go" => match pattern_index {
            0 => CodeElementKind::Function,
            1 => CodeElementKind::Method,
            2 => CodeElementKind::Struct, // Go type declarations
            _ => CodeElementKind::Function,
        },
        "java" => match pattern_index {
            0 => CodeElementKind::Method,
            1 => CodeElementKind::Class,
            2 => CodeElementKind::Interface,
            3 => CodeElementKind::Enum,
            _ => CodeElementKind::Function,
        },
        "c" | "h" => match pattern_index {
            0 => CodeElementKind::Function,
            1 => CodeElementKind::Struct,
            2 => CodeElementKind::Enum,
            3 => CodeElementKind::Typedef,
            _ => CodeElementKind::Function,
        },
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => match pattern_index {
            0 => CodeElementKind::Function,
            1 => CodeElementKind::Class,
            2 => CodeElementKind::Struct,
            3 => CodeElementKind::Enum,
            4 => CodeElementKind::Namespace,
            _ => CodeElementKind::Function,
        },
        _ => CodeElementKind::Function,
    }
}

/// Extract top-level (column-0) shell function definitions with
/// filename-qualified anchors (parsing spec D6). Mirrors the Python
/// registrar contract (mcp_intent_server spec D20) so both engines discover
/// identical `.sh` and shell-shebang anchors: `name() {` and
/// `function name {` forms, one `Function` block per unique name, definition
/// line as span.
pub fn extract_shell_blocks(content: &str, file_name: &str) -> Vec<ParsedCodeBlock> {
    let prefix = std::path::Path::new(file_name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.to_string());

    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut offset = 0usize;

    for (idx, line) in content.split('\n').enumerate() {
        let line_start = offset;
        offset += line.len() + 1;

        if let Some(name) = match_shell_function(line) {
            let qualified = format!("{prefix}::{name}");
            if seen.insert(qualified.clone()) {
                results.push(ParsedCodeBlock {
                    name: qualified,
                    kind: CodeElementKind::Function,
                    line_number: idx + 1,
                    byte_range: line_start..line_start + line.len(),
                });
            }
        }
    }

    results
}

/// Match a column-0 `name() {` or `function name {` definition line and
/// return the bare function name. Indented (nested) definitions are
/// intentionally not anchored.
fn match_shell_function(line: &str) -> Option<&str> {
    if line.is_empty() || line.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = line
        .strip_prefix("function ")
        .map(str::trim_start)
        .unwrap_or(line);
    let name_len = rest
        .char_indices()
        .take_while(|&(i, c)| {
            if i == 0 {
                c.is_ascii_alphabetic() || c == '_'
            } else {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
            }
        })
        .count();
    if name_len == 0 {
        return None;
    }
    let (name, tail) = rest.split_at(name_len);
    let mut tail = tail.trim_start();
    if let Some(after_parens) = tail.strip_prefix("()") {
        tail = after_parens.trim_start();
    }
    tail.starts_with('{').then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extractor() -> CodeAnchorExtractor {
        CodeAnchorExtractor::new()
    }

    // ── Rust tests ──────────────────────────────────────────

    #[test]
    fn rust_simple_function() {
        let src = "pub fn greet() { println!(\"hello\"); }\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "greet");
        assert_eq!(results[0].kind, CodeElementKind::Function);
    }

    #[test]
    fn rust_async_function() {
        let src = "pub async fn fetch_data() -> Result<(), Error> { Ok(()) }\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "fetch_data");
        assert_eq!(results[0].kind, CodeElementKind::Function);
    }

    #[test]
    fn rust_struct_with_fields() {
        let src = "pub struct Task {\n    pub id: Uuid,\n    pub name: String,\n}\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Task");
        assert_eq!(results[0].kind, CodeElementKind::Struct);
    }

    #[test]
    fn rust_enum_with_variants() {
        let src = "pub enum Status { Active, Inactive }\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Status");
        assert_eq!(results[0].kind, CodeElementKind::Enum);
    }

    #[test]
    fn rust_trait_definition() {
        let src = "pub trait Service {\n    fn execute(&self);\n}\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Service");
        assert_eq!(results[0].kind, CodeElementKind::Trait);
    }

    #[test]
    fn rust_const_definition() {
        let src = "pub const MAX_RETRY: u32 = 5;\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "MAX_RETRY");
        assert_eq!(results[0].kind, CodeElementKind::Const);
    }

    #[test]
    fn rust_generic_struct() {
        let src = "pub struct Wrapper<T> {\n    inner: T,\n}\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Wrapper");
    }

    #[test]
    fn rust_nested_generics() {
        let src = "pub fn process<T: Clone>(items: Vec<HashMap<String, Vec<T>>>) {}\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "process");
    }

    #[test]
    fn rust_multi_line_signature() {
        let src = r#"pub fn complex_handler(
    state: State<Arc<AppState>>,
    auth: AuthContext,
    body: Json<Request>,
) -> Result<Json<Response>, AppError> {}
"#;
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "complex_handler");
    }

    #[test]
    fn rust_attribute_block() {
        let src = "#[derive(Debug, Clone)]\npub struct Foo {\n    bar: i32,\n}\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Foo");
        assert_eq!(results[0].kind, CodeElementKind::Struct);
    }

    #[test]
    fn rust_no_elements() {
        let src = "// just comments\n// nothing else\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn rust_multiple_elements() {
        let src = r#"
pub struct MyStruct { val: i32 }
pub enum MyEnum { A, B }
pub fn my_func() {}
pub const MY_CONST: u32 = 42;
"#;
        let results = extractor().extract(src, "rs");
        assert!(results.len() >= 4);
    }

    // ── Python tests ────────────────────────────────────────

    #[test]
    fn python_function() {
        let src = "def greet(name):\n    print(f\"Hello {name}\")\n";
        let results = extractor().extract(src, "py");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "greet");
        assert_eq!(results[0].kind, CodeElementKind::Function);
    }

    #[test]
    fn python_class() {
        let src = "class MyService:\n    def run(self):\n        pass\n";
        let results = extractor().extract(src, "py");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.name == "MyService"));
    }

    // ── TypeScript tests ────────────────────────────────────

    #[test]
    fn typescript_function() {
        let src = "function handleClick(): void { console.log('clicked'); }\n";
        let results = extractor().extract(src, "ts");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "handleClick");
    }

    #[test]
    fn typescript_interface() {
        let src = "interface UserProps {\n  name: string;\n  age: number;\n}\n";
        let results = extractor().extract(src, "ts");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "UserProps");
        assert_eq!(results[0].kind, CodeElementKind::Interface);
    }

    // ── Unsupported extension ───────────────────────────────

    #[test]
    fn unsupported_extension_returns_empty() {
        let src = "puts 'hello world'\n";
        let results = extractor().extract(src, "rb");
        assert_eq!(results.len(), 0);
    }

    // ── Go tests ────────────────────────────────────────────

    #[test]
    fn go_function() {
        let src = "func main() {\n\tfmt.Println(\"hello\")\n}\n";
        let results = extractor().extract(src, "go");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
        assert_eq!(results[0].kind, CodeElementKind::Function);
    }

    // ── Java tests ──────────────────────────────────────────

    #[test]
    fn java_class_and_method() {
        let src = "public class HelloWorld {\n    public void greet() { }\n}\n";
        let results = extractor().extract(src, "java");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.name == "HelloWorld"));
    }

    // ── C tests ─────────────────────────────────────────────

    #[test]
    fn c_function() {
        let src = "int main(int argc, char *argv[]) {\n    return 0;\n}\n";
        let results = extractor().extract(src, "c");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
        assert_eq!(results[0].kind, CodeElementKind::Function);
    }

    // ── C++ tests ───────────────────────────────────────────

    #[test]
    fn cpp_class() {
        let src = "class Widget {\npublic:\n    void draw();\n};\n";
        let results = extractor().extract(src, "cpp");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.name == "Widget"));
    }

    // ── Edge cases ──────────────────────────────────────────

    #[test]
    fn empty_content() {
        let results = extractor().extract("", "rs");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn partial_syntax_error() {
        // tree-sitter should produce partial tree
        let src = "pub fn broken( { }\npub fn valid() {}\n";
        let results = extractor().extract(src, "rs");
        // Should extract at least the valid function
        assert!(results.iter().any(|r| r.name == "valid"));
    }

    // ── TC-PRS-101: Rust Complex Real-World Module ──────────

    #[test]
    fn rust_complex_real_world_module() {
        let src = r#"
use std::collections::HashMap;

pub const MAX_RETRY_COUNT: u32 = 5;

/// A task in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub name: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    Working,
    Completed,
    Failed,
}

pub trait TaskProcessor: Send + Sync {
    fn process(&self, task: &Task) -> Result<(), Error>;
    fn can_handle(&self, task: &Task) -> bool;
}

impl Task {
    pub fn new(name: &str) -> Self {
        Task { id: Uuid::new_v4(), name: name.to_string(), status: TaskStatus::Pending }
    }
    pub fn transition(&mut self, new_status: TaskStatus) -> Result<(), Error> {
        self.status = new_status;
        Ok(())
    }

    fn validate_transition(&self, _new: &TaskStatus) -> bool { true }
}

pub type TaskId = Uuid;

pub async fn spawn_worker(name: &str) -> JoinHandle<()> {
    tokio::spawn(async move {})
}
"#;
        let results = extractor().extract(src, "rs");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert!(
            names.contains(&"MAX_RETRY_COUNT"),
            "Should extract constant: got {:?}",
            names
        );
        assert!(
            names.contains(&"Task"),
            "Should extract struct: got {:?}",
            names
        );
        assert!(
            names.contains(&"TaskStatus"),
            "Should extract enum: got {:?}",
            names
        );
        assert!(
            names.contains(&"TaskProcessor"),
            "Should extract trait: got {:?}",
            names
        );
        assert!(
            names.contains(&"spawn_worker"),
            "Should extract async function: got {:?}",
            names
        );
        assert!(
            names.contains(&"TaskId"),
            "Should extract type alias: got {:?}",
            names
        );

        // Verify kinds
        assert!(results
            .iter()
            .any(|r| r.name == "MAX_RETRY_COUNT" && r.kind == CodeElementKind::Const));
        assert!(results
            .iter()
            .any(|r| r.name == "Task" && r.kind == CodeElementKind::Struct));
        assert!(results
            .iter()
            .any(|r| r.name == "TaskStatus" && r.kind == CodeElementKind::Enum));
        assert!(results
            .iter()
            .any(|r| r.name == "TaskProcessor" && r.kind == CodeElementKind::Trait));
        assert!(results
            .iter()
            .any(|r| r.name == "TaskId" && r.kind == CodeElementKind::TypeAlias));

        // use statement should NOT be extracted
        assert!(!names.contains(&"HashMap"));
    }

    #[test]
    fn rust_impl_block_captured() {
        let src = "pub struct Svc {}\nimpl Svc {\n    pub fn poll(&self) {}\n}\n";
        let results = extractor().extract(src, "rs");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"Svc"), "Should contain Svc struct or impl");
        assert!(
            names.contains(&"Svc::poll"),
            "impl methods should be owner-qualified: {:?}",
            results
        );
        assert!(
            !names.contains(&"poll"),
            "impl methods should not also emit bare aliases: {:?}",
            results
        );
        // Impl block for Svc should be captured
        assert!(
            results
                .iter()
                .any(|r| r.name == "Svc" && r.kind == CodeElementKind::Impl)
                || results
                    .iter()
                    .any(|r| r.name == "Svc" && r.kind == CodeElementKind::Struct),
            "Should have Svc as struct or impl: {:?}",
            results
        );
    }

    #[test]
    fn rust_trait_impl() {
        let src = "pub struct MyType {}\nimpl Display for MyType {\n    fn fmt(&self, f: &mut Formatter) -> Result { Ok(()) }\n}\n";
        let results = extractor().extract(src, "rs");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        // Should capture MyType struct and the impl
        assert!(names.contains(&"MyType"));
        assert!(
            names.contains(&"MyType::fmt"),
            "trait impl methods should use the implemented type: {:?}",
            results
        );
        assert!(
            !names.contains(&"fmt"),
            "trait impl methods should not also emit bare aliases: {:?}",
            results
        );
    }

    #[test]
    fn rust_top_level_functions_remain_bare_while_impl_methods_are_qualified() {
        let src = r#"
pub fn top_level() {}

pub struct RegistryRouter;

impl RegistryRouter {
    fn production() {}
    fn client_for(&self) {}
}

pub struct OciExtractor;

impl OciExtractor {
    pub async fn materialize_rootfs(&self) {}
}
"#;
        let results = extractor().extract(src, "rs");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert!(names.contains(&"top_level"), "{results:?}");
        assert!(names.contains(&"RegistryRouter::production"), "{results:?}");
        assert!(names.contains(&"RegistryRouter::client_for"), "{results:?}");
        assert!(
            names.contains(&"OciExtractor::materialize_rootfs"),
            "{results:?}"
        );
        assert!(
            !names.contains(&"production")
                && !names.contains(&"client_for")
                && !names.contains(&"materialize_rootfs"),
            "impl methods should not also emit bare aliases: {:?}",
            results
        );
    }

    #[test]
    fn rust_generic_impl_methods_use_base_type_name() {
        let src = r#"
pub struct Wrapper<T> {
    inner: T,
}

impl<T> Wrapper<T> {
    pub fn inner(&self) -> &T {
        &self.inner
    }
}
"#;
        let results = extractor().extract(src, "rs");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert!(names.contains(&"Wrapper::inner"), "{results:?}");
        assert!(
            !names.contains(&"inner"),
            "generic impl method should not also emit a bare alias: {:?}",
            results
        );
    }

    #[test]
    fn rust_deeply_nested_generics() {
        let src = r#"pub fn process<T: Clone + Send + 'static>(
    items: Vec<HashMap<String, Vec<T>>>,
    callback: impl Fn(&T) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>>>>,
) -> Result<Vec<T>, Box<dyn Error>> {}
"#;
        let results = extractor().extract(src, "rs");
        assert_eq!(
            results.len(),
            1,
            "Should extract exactly one function: {:?}",
            results
        );
        assert_eq!(results[0].name, "process");
        assert_eq!(results[0].kind, CodeElementKind::Function);
        // Byte range should cover entire function
        assert!(results[0].byte_range.end > results[0].byte_range.start);
    }

    #[test]
    fn rust_multiple_impl_blocks() {
        let src = r#"
pub struct Config {}

impl Config {
    pub fn new() -> Self { Config {} }
    pub fn validate(&self) -> bool { true }
}

impl Default for Config {
    fn default() -> Self { Config {} }
}
"#;
        let results = extractor().extract(src, "rs");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"Config"),
            "Should extract Config struct: {:?}",
            names
        );
    }

    #[test]
    fn rust_all_element_kinds() {
        let src = r#"
pub fn my_func() {}
pub struct MyStruct {}
pub enum MyEnum { A }
pub trait MyTrait {}
impl MyStruct {}
pub const MY_CONST: i32 = 42;
pub type MyAlias = Vec<String>;
"#;
        let results = extractor().extract(src, "rs");
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::Function),
            "Should have Function"
        );
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::Struct),
            "Should have Struct"
        );
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::Enum),
            "Should have Enum"
        );
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::Trait),
            "Should have Trait"
        );
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::Impl),
            "Should have Impl"
        );
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::Const),
            "Should have Const"
        );
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::TypeAlias),
            "Should have TypeAlias"
        );
    }

    // ── TC-PRS-102: TypeScript Complex Module ───────────────

    #[test]
    fn typescript_complex_module() {
        let src = r#"
export const API_VERSION = "v1";

export interface TaskConfig {
  maxRetries: number;
  backoffMs: number;
}

export type TaskId = string & { readonly __brand: unique symbol };

export class TaskService {
  private readonly config: TaskConfig;

  constructor(config: TaskConfig) {
    this.config = config;
  }

  async processTask(taskId: TaskId): Promise<void> {
    // ...
  }

  private validateConfig(): boolean {
    return this.config.maxRetries > 0;
  }
}

export async function createTaskService(config: TaskConfig): Promise<TaskService> {
  return new TaskService(config);
}

export enum TaskStatus {
  Pending = "pending",
  Working = "working",
  Completed = "completed",
}
"#;
        let results = extractor().extract(src, "ts");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert!(
            names.contains(&"API_VERSION"),
            "Should extract const: got {:?}",
            names
        );
        assert!(
            names.contains(&"TaskConfig"),
            "Should extract interface: got {:?}",
            names
        );
        assert!(
            names.contains(&"TaskId"),
            "Should extract type alias: got {:?}",
            names
        );
        assert!(
            names.contains(&"TaskService"),
            "Should extract class: got {:?}",
            names
        );
        assert!(
            names.contains(&"createTaskService"),
            "Should extract function: got {:?}",
            names
        );
        assert!(
            names.contains(&"TaskStatus"),
            "Should extract enum: got {:?}",
            names
        );

        // Verify kinds
        assert!(results
            .iter()
            .any(|r| r.name == "TaskConfig" && r.kind == CodeElementKind::Interface));
        assert!(results
            .iter()
            .any(|r| r.name == "TaskService" && r.kind == CodeElementKind::Class));
        assert!(results
            .iter()
            .any(|r| r.name == "TaskStatus" && r.kind == CodeElementKind::Enum));
    }

    #[test]
    fn typescript_arrow_function_const() {
        let src =
            "export const handler = (req: Request): Response => { return new Response(); };\n";
        let results = extractor().extract(src, "ts");
        assert!(
            results.iter().any(|r| r.name == "handler"),
            "Should extract const arrow function: {:?}",
            results
        );
    }

    #[test]
    fn typescript_skips_nested_lexical_bindings() {
        let src = r#"
const handler = () => {
  const range = 1;
  let text = "local";
  return `${range}:${text}`;
};
"#;
        let results = extractor().extract(src, "ts");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert!(names.contains(&"handler"), "got {names:?}");
        assert!(
            !names.contains(&"range") && !names.contains(&"text"),
            "nested locals should not become grounding anchors: {names:?}"
        );
    }

    #[test]
    fn typescript_skips_namespace_exports() {
        let src = r#"
namespace Internal {
  export const nested = 1;
}

export const topLevel = 2;
"#;
        let results = extractor().extract(src, "ts");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert!(names.contains(&"topLevel"), "got {names:?}");
        assert!(
            !names.contains(&"nested"),
            "namespace exports should not become file-level anchors: {names:?}"
        );
    }

    #[test]
    fn typescript_exported_and_non_exported() {
        let src = "export function publicFn() {}\nfunction privateFn() {}\n";
        let results = extractor().extract(src, "ts");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"publicFn"));
        assert!(names.contains(&"privateFn"));
    }

    // ── TC-PRS-103: Python Complex Module ───────────────────

    #[test]
    fn python_complex_module() {
        let src = r#"
MAX_RETRY = 5

class TaskProcessor:
    def __init__(self, config: dict):
        self.config = config

    def process(self, task_id: str) -> bool:
        pass

    @staticmethod
    def validate(data: dict) -> bool:
        pass

async def run_worker(processor: TaskProcessor) -> None:
    pass

def _internal_helper():
    pass
"#;
        let results = extractor().extract(src, "py");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        assert!(
            names.contains(&"TaskProcessor"),
            "Should extract class: got {:?}",
            names
        );
        assert!(
            names.contains(&"run_worker"),
            "Should extract async function: got {:?}",
            names
        );
        assert!(
            names.contains(&"_internal_helper"),
            "Should extract private function: got {:?}",
            names
        );

        // Nested methods should also be captured
        assert!(
            names.contains(&"__init__")
                || names.contains(&"process")
                || names.contains(&"validate"),
            "Should extract at least some class methods: got {:?}",
            names
        );
    }

    #[test]
    fn python_decorated_function() {
        let src = "@app.route('/api')\ndef handler():\n    return 'ok'\n";
        let results = extractor().extract(src, "py");
        assert!(
            results.iter().any(|r| r.name == "handler"),
            "Decorated function should be extracted: {:?}",
            results
        );
    }

    #[test]
    fn python_nested_class() {
        let src = "class Outer:\n    class Inner:\n        def method(self):\n            pass\n";
        let results = extractor().extract(src, "py");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"Outer"),
            "Should extract Outer class: {:?}",
            names
        );
        assert!(
            names.contains(&"Inner"),
            "Should extract Inner class: {:?}",
            names
        );
    }

    // ── TC-PRS-104: Multi-Language Dispatch ──────────────────

    #[test]
    fn dispatch_all_supported_extensions() {
        let ext = extractor();

        // Rust
        let rs = ext.extract("pub fn f() {}\n", "rs");
        assert!(!rs.is_empty(), "Rust profile should extract elements");

        // Python
        let py = ext.extract("def f():\n    pass\n", "py");
        assert!(!py.is_empty(), "Python profile should extract elements");

        // TypeScript
        let ts = ext.extract("function f(): void {}\n", "ts");
        assert!(!ts.is_empty(), "TypeScript profile should extract elements");

        // JavaScript
        let js = ext.extract("function f() {}\n", "js");
        assert!(!js.is_empty(), "JavaScript profile should extract elements");

        // Go
        let go = ext.extract("func main() {}\n", "go");
        assert!(!go.is_empty(), "Go profile should extract elements");

        // Java
        let java = ext.extract("public class Foo { public void bar() {} }\n", "java");
        assert!(!java.is_empty(), "Java profile should extract elements");

        // C
        let c = ext.extract("int main() { return 0; }\n", "c");
        assert!(!c.is_empty(), "C profile should extract elements");

        // C++
        let cpp = ext.extract("class Foo {};\n", "cpp");
        assert!(!cpp.is_empty(), "C++ profile should extract elements");
    }

    #[test]
    fn unsupported_extensions_return_empty() {
        let ext = extractor();
        assert!(
            ext.extract("puts 'hello'", "rb").is_empty(),
            "Ruby unsupported"
        );
        assert!(
            ext.extract("print('hi')", "swift").is_empty(),
            "Swift unsupported"
        );
        assert!(
            ext.extract("echo 'hi'", "sh").is_empty(),
            "Bash unsupported"
        );
        assert!(
            ext.extract("fn main() {}", "zig").is_empty(),
            "Zig unsupported"
        );
    }

    #[test]
    fn extension_with_leading_dot() {
        // The extractor strips leading dot
        let results = extractor().extract("pub fn test() {}\n", ".rs");
        assert!(
            !results.is_empty(),
            "Should handle extension with leading dot"
        );
    }

    #[test]
    fn tsx_extension() {
        // TSX uses its own profile while sharing the TypeScript anchor query.
        let src = "export function Component(): void {}\n";
        let results = extractor().extract(src, "tsx");
        assert!(
            results.iter().any(|r| r.name == "Component"),
            "TSX should extract via its dedicated profile: {:?}",
            results
        );
    }

    #[test]
    fn tsx_extracts_top_level_components_without_nested_locals() {
        let src = r#"
type Props = { value: string };

export const SpecEditor = ({ value }: Props) => {
  const range = value.length;
  const text = value.trim();
  return <section>{text}:{range}</section>;
};

function renderSpecInline(value: string) {
  return <span>{value}</span>;
}

const RefExpansionPanel = () => <aside />;
"#;
        let results = extractor().extract(src, "tsx");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

        for expected in [
            "Props",
            "SpecEditor",
            "renderSpecInline",
            "RefExpansionPanel",
        ] {
            assert!(names.contains(&expected), "missing {expected}: {names:?}");
        }
        for local in ["range", "text"] {
            assert!(
                !names.contains(&local),
                "nested local {local} should not be registered: {names:?}"
            );
        }
    }

    #[test]
    fn typescript_named_default_exports() {
        let src = r#"
export default function LandingPage() {
  return null;
}

export default class ApiClient {}
"#;
        let results = extractor().extract(src, "ts");

        assert!(results
            .iter()
            .any(|r| r.name == "LandingPage" && r.kind == CodeElementKind::Function));
        assert!(results
            .iter()
            .any(|r| r.name == "ApiClient" && r.kind == CodeElementKind::Class));
    }

    #[test]
    fn jsx_extension() {
        let src = "function Component() { return null; }\n";
        let results = extractor().extract(src, "jsx");
        assert!(
            results.iter().any(|r| r.name == "Component"),
            "JSX should use JavaScript profile"
        );
    }

    #[test]
    fn cpp_alternate_extensions() {
        let src = "class Widget {};\n";
        for ext in &["cpp", "cc", "cxx", "hpp", "hxx"] {
            let results = extractor().extract(src, ext);
            assert!(!results.is_empty(), "C++ extension .{} should work", ext);
        }
    }

    #[test]
    fn c_header_extension() {
        let src = "struct Point { int x; int y; };\n";
        let results = extractor().extract(src, "h");
        assert!(!results.is_empty(), ".h extension should use C profile");
    }

    // ── TC-PRS-105: Error-Tolerant Parsing ──────────────────

    #[test]
    fn rust_partial_syntax_incomplete_struct() {
        // tree-sitter error recovery is best-effort — it should NOT crash
        // and should extract at least some elements
        let src = "pub struct Incomplete {\n    // missing closing brace\n\npub fn valid_func() -> bool { true }\n";
        let results = extractor().extract(src, "rs");
        // The parser should not panic and should return some results.
        // Incomplete may be captured (tree-sitter can still identify the struct name),
        // valid_func may or may not be captured depending on error recovery.
        assert!(
            !results.is_empty(),
            "Should extract at least something from partial syntax: {:?}",
            results
        );
    }

    #[test]
    fn rust_syntax_error_at_end() {
        let src = "pub struct Good { pub x: i32 }\npub fn also_good() {}\npub fn bad(\n";
        let results = extractor().extract(src, "rs");
        assert!(
            results.iter().any(|r| r.name == "Good"),
            "Should extract Good struct: {:?}",
            results
        );
        assert!(
            results.iter().any(|r| r.name == "also_good"),
            "Should extract also_good fn: {:?}",
            results
        );
    }

    #[test]
    fn typescript_syntax_error_recovery() {
        let src = "export function valid(): void {}\nexport class { invalid }\nexport function alsoValid(): void {}\n";
        let results = extractor().extract(src, "ts");
        assert!(
            results.iter().any(|r| r.name == "valid"),
            "Should extract valid function: {:?}",
            results
        );
    }

    // ── TC-PRS-106: Complex Generic Signatures ──────────────

    #[test]
    fn rust_where_clause_function() {
        let src = r#"pub fn handler<T, E>(input: T) -> Result<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: std::error::Error + From<std::io::Error>,
{
    Ok(input.clone())
}
"#;
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "handler");
    }

    #[test]
    fn rust_lifetime_parameters() {
        let src = "pub fn borrow<'a, 'b: 'a>(x: &'a str, y: &'b str) -> &'a str { x }\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "borrow");
    }

    #[test]
    fn rust_const_generic() {
        let src = "pub struct FixedArray<const N: usize> {\n    data: [u8; N],\n}\n";
        let results = extractor().extract(src, "rs");
        assert!(
            results.iter().any(|r| r.name == "FixedArray"),
            "Should extract const generic struct: {:?}",
            results
        );
    }

    // ── Go extended tests ───────────────────────────────────

    #[test]
    fn go_method_on_receiver() {
        let src = "type Server struct{}\nfunc (s *Server) Start() error { return nil }\n";
        let results = extractor().extract(src, "go");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"Start"),
            "Should extract method with receiver: {:?}",
            names
        );
    }

    #[test]
    fn go_interface_type() {
        let src = "type Handler interface {\n\tHandle(r Request) Response\n}\n";
        let results = extractor().extract(src, "go");
        assert!(
            results.iter().any(|r| r.name == "Handler"),
            "Should extract Go interface: {:?}",
            results
        );
    }

    #[test]
    fn go_multiple_constructs() {
        let src = r#"
package main

import "fmt"

type Config struct {
    Port int
    Host string
}

type Service interface {
    Run() error
}

func NewConfig() *Config {
    return &Config{Port: 8080, Host: "localhost"}
}

func (c *Config) Validate() bool {
    return c.Port > 0
}

func main() {
    fmt.Println("hello")
}
"#;
        let results = extractor().extract(src, "go");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"Config"),
            "Should extract Config struct: {:?}",
            names
        );
        assert!(
            names.contains(&"Service"),
            "Should extract Service interface: {:?}",
            names
        );
        assert!(
            names.contains(&"NewConfig"),
            "Should extract NewConfig function: {:?}",
            names
        );
        assert!(
            names.contains(&"Validate"),
            "Should extract Validate method: {:?}",
            names
        );
        assert!(
            names.contains(&"main"),
            "Should extract main function: {:?}",
            names
        );
    }

    // ── Java extended tests ─────────────────────────────────

    #[test]
    fn java_full_class() {
        let src = r#"
public class TaskService {
    private final Config config;

    public TaskService(Config config) {
        this.config = config;
    }

    public void process(String taskId) {
        // ...
    }

    private boolean validate() {
        return true;
    }
}
"#;
        let results = extractor().extract(src, "java");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"TaskService"),
            "Should extract class: {:?}",
            names
        );
        assert!(
            names.contains(&"process"),
            "Should extract method: {:?}",
            names
        );
        assert!(
            names.contains(&"validate"),
            "Should extract private method: {:?}",
            names
        );
    }

    #[test]
    fn java_interface() {
        let src = "public interface Processor {\n    void execute();\n}\n";
        let results = extractor().extract(src, "java");
        assert!(results
            .iter()
            .any(|r| r.name == "Processor" && r.kind == CodeElementKind::Interface));
    }

    #[test]
    fn java_enum() {
        let src = "public enum Status {\n    ACTIVE,\n    INACTIVE,\n    PENDING\n}\n";
        let results = extractor().extract(src, "java");
        assert!(results
            .iter()
            .any(|r| r.name == "Status" && r.kind == CodeElementKind::Enum));
    }

    // ── C extended tests ────────────────────────────────────

    #[test]
    fn c_struct_definition() {
        let src = "struct Point {\n    int x;\n    int y;\n};\n";
        let results = extractor().extract(src, "c");
        assert!(
            results
                .iter()
                .any(|r| r.name == "Point" && r.kind == CodeElementKind::Struct),
            "Should extract C struct: {:?}",
            results
        );
    }

    #[test]
    fn c_enum_definition() {
        let src = "enum Color {\n    RED,\n    GREEN,\n    BLUE\n};\n";
        let results = extractor().extract(src, "c");
        assert!(
            results
                .iter()
                .any(|r| r.name == "Color" && r.kind == CodeElementKind::Enum),
            "Should extract C enum: {:?}",
            results
        );
    }

    #[test]
    fn c_typedef() {
        // tree-sitter C grammar matches typedef with type_identifier declarator
        let src = "typedef struct node_s NodeT;\n";
        let results = extractor().extract(src, "c");
        assert!(
            results.iter().any(|r| r.kind == CodeElementKind::Typedef),
            "Should extract C typedef: {:?}",
            results
        );
    }

    #[test]
    fn c_multiple_constructs() {
        let src = r#"
struct Node {
    int value;
    struct Node* next;
};

enum Direction { NORTH, SOUTH, EAST, WEST };

typedef struct Node NodeT;

int main(int argc, char* argv[]) {
    return 0;
}

void helper(int x) {
    // no-op
}
"#;
        let results = extractor().extract(src, "c");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"Node"),
            "Should extract Node struct: {:?}",
            names
        );
        assert!(
            names.contains(&"Direction"),
            "Should extract Direction enum: {:?}",
            names
        );
        assert!(
            names.contains(&"NodeT"),
            "Should extract NodeT typedef: {:?}",
            names
        );
        assert!(
            names.contains(&"main"),
            "Should extract main function: {:?}",
            names
        );
        assert!(
            names.contains(&"helper"),
            "Should extract helper function: {:?}",
            names
        );
    }

    // ── C++ extended tests ──────────────────────────────────

    #[test]
    fn cpp_namespace() {
        let src = "namespace my_ns {\n    class Widget {};\n}\n";
        let results = extractor().extract(src, "cpp");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"my_ns"),
            "Should extract namespace: {:?}",
            names
        );
        assert!(results
            .iter()
            .any(|r| r.name == "my_ns" && r.kind == CodeElementKind::Namespace));
    }

    #[test]
    fn cpp_struct() {
        let src = "struct Point {\n    double x;\n    double y;\n};\n";
        let results = extractor().extract(src, "cpp");
        assert!(
            results
                .iter()
                .any(|r| r.name == "Point" && r.kind == CodeElementKind::Struct),
            "Should extract C++ struct: {:?}",
            results
        );
    }

    #[test]
    fn cpp_enum() {
        let src = "enum class Color { Red, Green, Blue };\n";
        let results = extractor().extract(src, "cpp");
        assert!(
            results
                .iter()
                .any(|r| r.name == "Color" && r.kind == CodeElementKind::Enum),
            "Should extract C++ scoped enum: {:?}",
            results
        );
    }

    #[test]
    fn cpp_multiple_constructs() {
        let src = r#"
namespace engine {

class Renderer {
public:
    void draw();
};

struct Vertex {
    float x, y, z;
};

enum class Blend { Alpha, Additive };

}

void standalone_func() {}
"#;
        let results = extractor().extract(src, "cpp");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"engine"),
            "Should extract namespace: {:?}",
            names
        );
        assert!(
            names.contains(&"Renderer"),
            "Should extract class: {:?}",
            names
        );
        assert!(
            names.contains(&"Vertex"),
            "Should extract struct: {:?}",
            names
        );
        assert!(names.contains(&"Blend"), "Should extract enum: {:?}", names);
        assert!(
            names.contains(&"standalone_func"),
            "Should extract function: {:?}",
            names
        );
    }

    // ── JavaScript extended tests ───────────────────────────

    #[test]
    fn javascript_class_and_function() {
        let src = "class App {\n  constructor() {}\n  render() {}\n}\nfunction init() {}\n";
        let results = extractor().extract(src, "js");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"App"),
            "Should extract JS class: {:?}",
            names
        );
        assert!(
            names.contains(&"init"),
            "Should extract JS function: {:?}",
            names
        );
    }

    #[test]
    fn javascript_const_arrow() {
        let src = "const handler = (req) => { return 'ok'; };\n";
        let results = extractor().extract(src, "js");
        assert!(
            results.iter().any(|r| r.name == "handler"),
            "Should extract const arrow: {:?}",
            results
        );
    }

    #[test]
    fn javascript_mjs_extension() {
        let src = "export function init() {}\nexport const handler = () => 'ok';\n";
        let results = extractor().extract(src, "mjs");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"init"),
            "Should extract .mjs function: {:?}",
            names
        );
        assert!(
            names.contains(&"handler"),
            "Should extract .mjs const: {:?}",
            names
        );
    }

    // ── Edge case: empty content per language ───────────────

    #[test]
    fn empty_content_all_languages() {
        let ext = extractor();
        for lang_ext in &[
            "rs", "py", "ts", "tsx", "js", "jsx", "mjs", "go", "java", "c", "h", "cpp",
        ] {
            let results = ext.extract("", lang_ext);
            assert!(
                results.is_empty(),
                "Empty content for .{} should return empty",
                lang_ext
            );
        }
    }

    // ── Line number accuracy ────────────────────────────────

    #[test]
    fn line_numbers_are_one_indexed_and_accurate() {
        let src = "// comment\n// another\npub fn target() {}\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "target");
        assert_eq!(results[0].line_number, 3);
    }

    #[test]
    fn line_numbers_multiple_elements() {
        let src = "pub struct A {}\n\npub struct B {}\n\npub fn c() {}\n";
        let results = extractor().extract(src, "rs");
        assert!(results.len() >= 3);
        // Lines should be monotonically increasing for elements in order
        let lines: Vec<usize> = results.iter().map(|r| r.line_number).collect();
        for i in 1..lines.len() {
            assert!(
                lines[i] >= lines[i - 1],
                "Line numbers should be non-decreasing: {:?}",
                lines
            );
        }
    }

    // ── Byte range accuracy ─────────────────────────────────

    #[test]
    fn byte_ranges_are_valid() {
        let src = "pub fn first() {}\npub fn second() {}\npub fn third() {}\n";
        let results = extractor().extract(src, "rs");
        assert_eq!(results.len(), 3);
        for result in &results {
            assert!(
                result.byte_range.start < result.byte_range.end,
                "Byte range should be non-empty for {}: {:?}",
                result.name,
                result.byte_range
            );
            assert!(
                result.byte_range.end <= src.len(),
                "Byte range should not exceed source length for {}: {:?}",
                result.name,
                result.byte_range
            );
        }
    }

    // ── Comments-only files ─────────────────────────────────

    #[test]
    fn python_comments_only() {
        let src = "# This is a comment\n# Another comment\n\n# More comments\n";
        let results = extractor().extract(src, "py");
        assert!(results.is_empty());
    }

    #[test]
    fn typescript_comments_only() {
        let src = "// Single line comment\n/* Multi\nline\ncomment */\n";
        let results = extractor().extract(src, "ts");
        assert!(results.is_empty());
    }

    // ── Large file with many elements ───────────────────────

    #[test]
    fn rust_many_functions() {
        let mut src = String::new();
        for i in 0..50 {
            src.push_str(&format!("pub fn func_{}() {{}}\n", i));
        }
        let results = extractor().extract(&src, "rs");
        assert_eq!(
            results.len(),
            50,
            "Should extract all 50 functions, got {}",
            results.len()
        );
        for i in 0..50 {
            assert!(
                results.iter().any(|r| r.name == format!("func_{}", i)),
                "Missing func_{}",
                i
            );
        }
    }

    // ── Default trait impl ──────────────────────────────────

    #[test]
    fn code_anchor_extractor_default() {
        let ext = CodeAnchorExtractor::default();
        let results = ext.extract("pub fn test_fn() {}\n", "rs");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test_fn");
    }

    // ── Shell function anchors (parsing spec D6) ────────────

    #[test]
    fn shell_top_level_functions_filename_qualified() {
        let src = "#!/usr/bin/env bash\nusage() {\n  echo hi\n}\nfunction fail {\n  exit 1\n}\nsql_literal()   {\n  :\n}\n";
        let results = extract_shell_blocks(src, "scripts/seed.sh");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["seed.sh::usage", "seed.sh::fail", "seed.sh::sql_literal"]
        );
        assert!(results.iter().all(|r| r.kind == CodeElementKind::Function));
        assert_eq!(results[0].line_number, 2);
        assert_eq!(&src[results[0].byte_range.clone()], "usage() {");
    }

    #[test]
    fn shell_indented_and_duplicate_definitions_skipped() {
        let src = "outer() {\n  inner() {\n    :\n  }\n}\nouter() {\n  :\n}\n";
        let results = extract_shell_blocks(src, "x.sh");
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["x.sh::outer"]);
    }

    #[test]
    fn shell_non_definition_lines_ignored() {
        let src = "# usage() {\nif [ -n \"$1\" ]; then\n  echo hi\nfi\nVAR={a,b}\necho done\n";
        let results = extract_shell_blocks(src, "y.sh");
        assert!(results.is_empty(), "got {:?}", results);
    }
}
