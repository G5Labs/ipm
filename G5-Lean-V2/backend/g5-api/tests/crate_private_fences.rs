//! Owner entry points that only `g5-api-core` may reach.
//!
//! `g5-api-routes` is a separate crate, so a crate-private item in core is a
//! compile-time fence against every route: a route cannot build a lifecycle
//! request, call a dispatcher method by name, open a raw VM stream, send a
//! host command or name the bootstrap graph authority. This test pins those
//! declarations so that widening one to `pub` fails here instead of quietly
//! reopening the bypass. Compile-fail doctests would state the same thing,
//! but CI does not gate on doctests.
//!
//! Every declaration with a pinned name counts, including cfg variants and
//! same-named items in inline modules; one that compiles into production must
//! not be `pub`. The test pins declarations, not every path: a new public
//! wrapper around a fenced item is still a review concern.

use crate::source_scan;

use std::path::{Path, PathBuf};
use tree_sitter::{Node, Tree};

/// Declarations in `g5-api-core/src` that must stay crate-private.
/// `Type::function` names an inherent associated function; a bare name is a
/// module-level item.
const CRATE_PRIVATE: &[(&str, &str, &str)] = &[
    (
        "services/workspace/workspace_runtime.rs",
        "WorkspaceRuntime::call",
        "calls any dispatcher method by name with the request's authority",
    ),
    (
        "services/workspace/workspace_runtime.rs",
        "RuntimeRequest::lifecycle",
        "lifecycle authority skips the facade's owner check",
    ),
    (
        "ws/preview_tunnel.rs",
        "HostTunnel::open_workspace_rpc",
        "opens a raw dispatcher stream without any check",
    ),
    (
        "ws/preview_tunnel.rs",
        "HostTunnel::open_workspace_stream",
        "opens a raw VM data stream without any check",
    ),
    (
        "ws/host_ctl.rs",
        "HostCommandClient::execute",
        "sends any host control command",
    ),
    (
        "ws/host_ctl.rs",
        "HostCommandClient::execute_with_retry",
        "sends any host control command",
    ),
    (
        "services/hosts/worker_host_service.rs",
        "WorkerHostService::deliver_recorded_drain",
        "sends host.drain for any host and drain id",
    ),
    (
        "services/hosts/worker_host_service.rs",
        "HostDrainRecord.host_id",
        "lets code outside core make a drain record and deliver a drain nobody recorded",
    ),
    (
        "services/hosts/worker_host_service.rs",
        "HostDrainRecord.drain_id",
        "lets a route change the drain id it delivers, so the host's id no longer \
         matches the recorded one",
    ),
    (
        "services/hosts/host_control_state.rs",
        "PostgresHostControlState::new",
        "applies any host fact for any host: a made-up exit releases a live VM",
    ),
    (
        "services/workspace/workspace_vm_service.rs",
        "WorkspaceVmService::stop_as_system",
        "stops a workspace without an owner check",
    ),
    (
        "services/hosts/workspace_vm_release.rs",
        "record_forced_vm_loss",
        "releases every VM on a host; forced removal goes through WorkerHostService",
    ),
];

/// A struct whose fields stay private, and the only functions that may build
/// it from outside the crate.
const SEALED_STRUCTS: &[(&str, &str, &[&str])] = &[(
    "services/workspace/workspace_runtime.rs",
    "RuntimeRequest",
    &["owner"],
)];

/// Test-support entries that reach a fenced operation directly. Each compiles
/// only with the `test-helpers` feature, and no production code outside core
/// names it: workspace test builds enable that feature for every crate.
const TEST_ONLY_ENTRIES: &[(&str, &str)] = &[
    (
        "services/workspace/workspace_runtime.rs",
        "WorkspaceRuntime::lifecycle_call_for_test",
    ),
    ("ws/host_ctl.rs", "HostCommandClient::execute_for_test"),
];

/// Crates whose production code must not name a test-only entry.
const OUTSIDE_CORE: &[&str] = &["g5-api-routes/src", "g5-api/src"];

#[derive(Debug)]
struct Declaration {
    /// `Type::function` (the impl's type without its path or generic
    /// arguments), `Type.field`, `Type.0` for a positional field, or an item
    /// name. Items in inline modules keep their bare names, so a same-named
    /// item there is checked too.
    name: String,
    /// The visibility modifier as written, without whitespace; empty when
    /// the declaration has none.
    visibility: String,
    attributes: Vec<String>,
    /// False when the declaration, or an item enclosing it, compiles only in
    /// tests, by `source_scan`'s production-build reading of `cfg`.
    production: bool,
}

impl Declaration {
    fn is_public(&self) -> bool {
        self.visibility == "pub"
    }
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).expect("UTF-8 source")
}

fn compact(node: Node<'_>, source: &str) -> String {
    text(node, source).split_whitespace().collect()
}

fn visibility(node: Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    let modifier = node
        .children(&mut cursor)
        .find(|child| child.kind() == "visibility_modifier")
        .map(|modifier| compact(modifier, source))
        .unwrap_or_default();
    modifier
}

fn attributes(node: Node<'_>, source: &str) -> Vec<String> {
    source_scan::item_attributes(node, source)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Whether `node` compiles into a production build, given whether the item
/// enclosing it does.
fn in_production(node: Node<'_>, source: &str, enclosing: bool) -> bool {
    enclosing && !source_scan::any_excludes_production(&source_scan::item_attributes(node, source))
}

fn name_of(node: Node<'_>, source: &str) -> String {
    text(
        node.child_by_field_name("name").expect("named declaration"),
        source,
    )
    .to_string()
}

/// The impl's self type without its path or generic arguments.
fn impl_owner<'a>(mut node: Node<'_>, source: &'a str) -> &'a str {
    loop {
        node = match node.kind() {
            "generic_type" => node.child_by_field_name("type").expect("generic base type"),
            "scoped_type_identifier" => node.child_by_field_name("name").expect("type name"),
            _ => return text(node, source),
        };
    }
}

/// The fields of one struct, named or positional.
fn fields(item: Node<'_>, source: &str, production: bool, found: &mut Vec<Declaration>) {
    let owner = name_of(item, source);
    let Some(body) = item.child_by_field_name("body") else {
        return;
    };
    let mut cursor = body.walk();
    let mut position = 0;
    let mut pending_visibility = String::new();
    for field in body.named_children(&mut cursor) {
        match (body.kind(), field.kind()) {
            ("field_declaration_list", "field_declaration") => found.push(Declaration {
                name: format!("{owner}.{}", name_of(field, source)),
                visibility: visibility(field, source),
                attributes: attributes(field, source),
                production: in_production(field, source, production),
            }),
            ("ordered_field_declaration_list", "visibility_modifier") => {
                pending_visibility = compact(field, source);
            }
            ("ordered_field_declaration_list", kind)
                if !matches!(kind, "attribute_item" | "line_comment" | "block_comment") =>
            {
                found.push(Declaration {
                    name: format!("{owner}.{position}"),
                    visibility: std::mem::take(&mut pending_visibility),
                    attributes: Vec::new(),
                    production,
                });
                position += 1;
            }
            _ => {}
        }
    }
}

/// Every item, inherent associated function and struct field of one file,
/// including cfg variants and items in inline modules.
fn declarations(source: &str, tree: &Tree) -> Vec<Declaration> {
    fn visit(node: Node<'_>, source: &str, enclosing: bool, found: &mut Vec<Declaration>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let production = in_production(child, source, enclosing);
            match child.kind() {
                "function_item" | "struct_item" | "enum_item" | "type_item" | "const_item"
                | "static_item" => {
                    found.push(Declaration {
                        name: name_of(child, source),
                        visibility: visibility(child, source),
                        attributes: attributes(child, source),
                        production,
                    });
                    if child.kind() == "struct_item" {
                        fields(child, source, production, found);
                    }
                }
                "mod_item" => {
                    if let Some(body) = child.child_by_field_name("body") {
                        visit(body, source, production, found);
                    }
                }
                "impl_item" if child.child_by_field_name("trait").is_none() => {
                    let owner = impl_owner(
                        child.child_by_field_name("type").expect("impl type"),
                        source,
                    );
                    let body = child.child_by_field_name("body").expect("impl body");
                    let mut items = body.walk();
                    for item in body.children(&mut items) {
                        if item.kind() == "function_item" {
                            found.push(Declaration {
                                name: format!("{owner}::{}", name_of(item, source)),
                                visibility: visibility(item, source),
                                attributes: attributes(item, source),
                                production: in_production(item, source, production),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    visit(tree.root_node(), source, true, &mut found);
    found
}

/// Whether some declaration of `name` compiles into production.
fn compiled(declarations: &[Declaration], name: &str) -> bool {
    declarations
        .iter()
        .any(|declaration| declaration.name == name && declaration.production)
}

/// Whether some declaration of `name` that compiles into production is `pub`.
fn exposed(declarations: &[Declaration], name: &str) -> bool {
    declarations.iter().any(|declaration| {
        declaration.name == name && declaration.production && declaration.is_public()
    })
}

fn backend() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api lives directly under backend")
        .to_path_buf()
}

/// The declarations of one file under `g5-api-core/src`, which must declare
/// each of `names` at least once.
fn core_declarations(file: &str, names: &[&str]) -> Vec<Declaration> {
    let path = backend().join("g5-api-core/src").join(file);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let found = declarations(&source, &source_scan::parse_rust(&source));
    for name in names {
        assert!(
            found.iter().any(|declaration| declaration.name == *name),
            "g5-api-core/src/{file} no longer declares {name}; it moved or was renamed, \
             so point {} at it",
            file!()
        );
    }
    found
}

/// Lines of one file's production code that name `identifier`, including
/// inside macro arguments; strings and comments do not count.
fn identifier_lines(source: &str, tree: &Tree, identifier: &str) -> Vec<usize> {
    fn visit(node: Node<'_>, source: &str, identifier: &str, lines: &mut Vec<usize>) {
        if source_scan::any_excludes_production(&source_scan::item_attributes(node, source)) {
            return;
        }
        if matches!(node.kind(), "identifier" | "field_identifier")
            && text(node, source) == identifier
        {
            lines.push(node.start_position().row + 1);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, source, identifier, lines);
        }
    }
    let mut lines = Vec::new();
    visit(tree.root_node(), source, identifier, &mut lines);
    lines
}

#[test]
fn owner_entry_points_stay_crate_private() {
    let mut problems = Vec::new();
    for (file, name, why) in CRATE_PRIVATE {
        let declarations = core_declarations(file, &[name]);
        if !compiled(&declarations, name) {
            problems.push(format!(
                "  {file}: {name} no longer compiles into production; point the entry at \
                 what replaced it"
            ));
        }
        if exposed(&declarations, name) {
            problems.push(format!("  {file}: {name} is pub; it {why}"));
        }
    }
    for (file, name, public_functions) in SEALED_STRUCTS {
        let declarations = core_declarations(file, &[name]);
        let (field_prefix, function_prefix) = (format!("{name}."), format!("{name}::"));
        for declaration in declarations
            .iter()
            .filter(|declaration| declaration.production)
        {
            if declaration.name == *name
                && declaration
                    .attributes
                    .iter()
                    .any(|attribute| attribute.contains("Deserialize"))
            {
                problems.push(format!(
                    "  {file}: {name} derives Deserialize, which would build any request"
                ));
            }
            if declaration.name.starts_with(&field_prefix) && !declaration.visibility.is_empty() {
                problems.push(format!(
                    "  {file}: field {} is {}; {name} is built only through its constructors",
                    declaration.name, declaration.visibility
                ));
            }
            if let Some(function) = declaration.name.strip_prefix(&function_prefix) {
                if declaration.is_public() && !public_functions.contains(&function) {
                    problems.push(format!(
                        "  {file}: {} is pub; only {public_functions:?} may build {name} \
                         outside the crate",
                        declaration.name
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "These declarations keep routes on the owners' operations and must stay \
         crate-private (see .g5/ontology/software_architecture/boundaries.g5.md, \
         g5.architecture.commands):\n{}\n",
        problems.join("\n")
    );
}

#[test]
fn test_only_entries_stay_out_of_production() {
    let roots: Vec<PathBuf> = OUTSIDE_CORE
        .iter()
        .map(|root| backend().join(root))
        .collect();
    for (file, name) in TEST_ONLY_ENTRIES {
        let declarations = core_declarations(file, &[name]);
        assert!(
            !compiled(&declarations, name),
            "{file}: {name} must compile only in tests, behind the test-helpers feature"
        );
        let function = name.rsplit("::").next().expect("function name");
        let uses: Vec<String> = source_scan::production_sources(&roots)
            .into_iter()
            .flat_map(|(path, source, tree)| {
                identifier_lines(&source, &tree, function)
                    .into_iter()
                    .map(move |line| format!("  {}:{line}", path.display()))
            })
            .collect();
        assert!(
            uses.is_empty(),
            "{name} is test support; production code must not call it:\n{}",
            uses.join("\n")
        );
    }
}

#[test]
fn declarations_record_visibility_and_production_status() {
    let source = r#"
        pub struct Sealed {
            private: u8,
            pub(crate) internal: u8,
            pub open: u8,
            #[cfg(test)]
            pub probe: u8,
        }
        pub struct Pair(u8, pub u8);
        impl Sealed {
            pub fn owner() -> Self { todo!() }
            pub(crate) fn lifecycle() -> Self { todo!() }
            fn helper(&self) {}
        }
        impl Clone for Sealed {
            fn clone(&self) -> Self { todo!() }
        }
        impl<T> Holder<T> {
            pub fn open() {}
        }
        impl super::Scoped {
            pub(crate) fn send() {}
        }
        #[cfg(feature = "test-helpers")]
        pub(in crate::services) fn scoped() {}
        mod inner {
            pub struct Nested;
        }
        #[cfg(test)]
        mod tests {
            impl Sealed {
                pub fn fixture() {}
            }
        }
    "#;
    let found: Vec<(String, String, bool)> = declarations(source, &source_scan::parse_rust(source))
        .into_iter()
        .map(|declaration| {
            (
                declaration.name,
                declaration.visibility,
                declaration.production,
            )
        })
        .collect();
    let expected = [
        ("Sealed", "pub", true),
        ("Sealed.private", "", true),
        ("Sealed.internal", "pub(crate)", true),
        ("Sealed.open", "pub", true),
        ("Sealed.probe", "pub", false),
        ("Pair", "pub", true),
        ("Pair.0", "", true),
        ("Pair.1", "pub", true),
        ("Sealed::owner", "pub", true),
        ("Sealed::lifecycle", "pub(crate)", true),
        ("Sealed::helper", "", true),
        ("Holder::open", "pub", true),
        ("Scoped::send", "pub(crate)", true),
        ("scoped", "pub(incrate::services)", false),
        ("Nested", "pub", true),
        ("Sealed::fixture", "pub", false),
    ]
    .map(|(name, visibility, production)| (name.to_string(), visibility.to_string(), production));
    assert_eq!(found, expected);
}

#[test]
fn every_production_variant_and_namesake_is_checked() {
    let source = r#"
        mod inner {
            impl Runtime {
                pub(crate) fn call() {}
            }
        }
        impl Runtime {
            #[cfg(test)]
            pub(crate) fn call() {}
            #[cfg(not(test))]
            pub fn call() {}
        }
        #[cfg(test)]
        mod tests {
            impl Runtime {
                pub fn send() {}
            }
        }
        impl Runtime {
            pub(crate) fn send() {}
        }
        #[cfg(feature = "test-helpers")]
        impl Runtime {
            pub fn call_for_test() {}
        }
    "#;
    let declarations = declarations(source, &source_scan::parse_rust(source));
    assert!(
        exposed(&declarations, "Runtime::call"),
        "a pub production variant counts even after a crate-private namesake and a \
         test-only variant"
    );
    assert!(
        compiled(&declarations, "Runtime::send") && !exposed(&declarations, "Runtime::send"),
        "a pub variant inside test-only code does not count"
    );
    assert!(
        !compiled(&declarations, "Runtime::call_for_test"),
        "an impl behind the test-helpers feature is test-only"
    );
}

#[test]
fn identifier_scan_skips_test_code_strings_and_comments() {
    let source = r#"
        async fn handler(runtime: &Runtime) {
            runtime.entry(1).await;
            let _ = tokio::join!(runtime.entry(2));
            let _ = "runtime.entry(3)";
            // runtime.entry(4)
        }
        #[cfg(test)]
        mod tests {
            fn fixture(runtime: &Runtime) {
                runtime.entry(5);
            }
        }
        #[cfg(feature = "test-helpers")]
        fn helper(runtime: &Runtime) {
            runtime.entry(6);
        }
    "#;
    assert_eq!(
        identifier_lines(source, &source_scan::parse_rust(source), "entry"),
        [3, 4]
    );
}
