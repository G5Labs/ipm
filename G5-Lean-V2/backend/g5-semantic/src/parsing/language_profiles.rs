use tree_sitter::Language;

/// Trait for language-specific code parsing configuration.
pub trait LanguageProfile: Send + Sync {
    /// tree-sitter language grammar.
    fn language(&self) -> Language;
    /// S-expression query for extracting named code elements.
    fn anchors_query(&self) -> &str;
    /// File extensions this profile handles (without the leading dot).
    fn extensions(&self) -> &[&str];
}

// ── Rust ────────────────────────────────────────────────────

pub struct RustProfile;

impl LanguageProfile for RustProfile {
    fn language(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn anchors_query(&self) -> &str {
        r#"
(function_item name: (identifier) @name) @def
(struct_item name: (type_identifier) @name) @def
(enum_item name: (type_identifier) @name) @def
(trait_item name: (type_identifier) @name) @def
(impl_item type: (type_identifier) @name) @def
(const_item name: (identifier) @name) @def
(type_item name: (type_identifier) @name) @def
"#
    }

    fn extensions(&self) -> &[&str] {
        &["rs"]
    }
}

// ── Python ──────────────────────────────────────────────────

pub struct PythonProfile;

impl LanguageProfile for PythonProfile {
    fn language(&self) -> Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn anchors_query(&self) -> &str {
        r#"
(function_definition name: (identifier) @name) @def
(class_definition name: (identifier) @name) @def
"#
    }

    fn extensions(&self) -> &[&str] {
        &["py"]
    }
}

// ── TypeScript ──────────────────────────────────────────────

pub struct TypeScriptProfile;

fn typescript_anchors_query() -> &'static str {
    r#"
(program
  (function_declaration name: (identifier) @name) @def @function)
(program
  (export_statement
    declaration: (function_declaration name: (identifier) @name) @def @function))
(program
  (class_declaration name: (type_identifier) @name) @def @class)
(program
  (export_statement
    declaration: (class_declaration name: (type_identifier) @name) @def @class))
(program
  (interface_declaration name: (type_identifier) @name) @def @interface)
(program
  (export_statement
    declaration: (interface_declaration name: (type_identifier) @name) @def @interface))
(program
  (type_alias_declaration name: (type_identifier) @name) @def @type_alias)
(program
  (export_statement
    declaration: (type_alias_declaration name: (type_identifier) @name) @def @type_alias))
(program
  (enum_declaration name: (identifier) @name) @def @enum)
(program
  (export_statement
    declaration: (enum_declaration name: (identifier) @name) @def @enum))
(program
  (lexical_declaration
    (variable_declarator name: (identifier) @name)) @def @const)
(program
  (export_statement
    declaration: (lexical_declaration
      (variable_declarator name: (identifier) @name)) @def @const))
"#
}

impl LanguageProfile for TypeScriptProfile {
    fn language(&self) -> Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn anchors_query(&self) -> &str {
        typescript_anchors_query()
    }

    fn extensions(&self) -> &[&str] {
        &["ts"]
    }
}

pub struct TsxProfile;

impl LanguageProfile for TsxProfile {
    fn language(&self) -> Language {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    }

    fn anchors_query(&self) -> &str {
        typescript_anchors_query()
    }

    fn extensions(&self) -> &[&str] {
        &["tsx"]
    }
}

// ── JavaScript ──────────────────────────────────────────────

pub struct JavaScriptProfile;

impl LanguageProfile for JavaScriptProfile {
    fn language(&self) -> Language {
        tree_sitter_javascript::LANGUAGE.into()
    }

    fn anchors_query(&self) -> &str {
        r#"
(function_declaration name: (identifier) @name) @def
(class_declaration name: (identifier) @name) @def
(lexical_declaration
  (variable_declarator name: (identifier) @name)) @def
"#
    }

    fn extensions(&self) -> &[&str] {
        &["js", "jsx", "mjs"]
    }
}

// ── Go ──────────────────────────────────────────────────────

pub struct GoProfile;

impl LanguageProfile for GoProfile {
    fn language(&self) -> Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn anchors_query(&self) -> &str {
        r#"
(function_declaration name: (identifier) @name) @def
(method_declaration name: (field_identifier) @name) @def
(type_declaration (type_spec name: (type_identifier) @name)) @def
"#
    }

    fn extensions(&self) -> &[&str] {
        &["go"]
    }
}

// ── Java ────────────────────────────────────────────────────

pub struct JavaProfile;

impl LanguageProfile for JavaProfile {
    fn language(&self) -> Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn anchors_query(&self) -> &str {
        r#"
(method_declaration name: (identifier) @name) @def
(class_declaration name: (identifier) @name) @def
(interface_declaration name: (identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
"#
    }

    fn extensions(&self) -> &[&str] {
        &["java"]
    }
}

// ── C ───────────────────────────────────────────────────────

pub struct CProfile;

impl LanguageProfile for CProfile {
    fn language(&self) -> Language {
        tree_sitter_c::LANGUAGE.into()
    }

    fn anchors_query(&self) -> &str {
        r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def
(struct_specifier name: (type_identifier) @name) @def
(enum_specifier name: (type_identifier) @name) @def
(type_definition declarator: (type_identifier) @name) @def
"#
    }

    fn extensions(&self) -> &[&str] {
        &["c", "h"]
    }
}

// ── C++ ─────────────────────────────────────────────────────

pub struct CppProfile;

impl LanguageProfile for CppProfile {
    fn language(&self) -> Language {
        tree_sitter_cpp::LANGUAGE.into()
    }

    fn anchors_query(&self) -> &str {
        r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def
(class_specifier name: (type_identifier) @name) @def
(struct_specifier name: (type_identifier) @name) @def
(enum_specifier name: (type_identifier) @name) @def
(namespace_definition name: (namespace_identifier) @name) @def
"#
    }

    fn extensions(&self) -> &[&str] {
        &["cpp", "cc", "cxx", "hpp", "hxx"]
    }
}

/// Returns all built-in language profiles.
pub fn default_profiles() -> Vec<Box<dyn LanguageProfile>> {
    vec![
        Box::new(RustProfile),
        Box::new(PythonProfile),
        Box::new(TypeScriptProfile),
        Box::new(TsxProfile),
        Box::new(JavaScriptProfile),
        Box::new(GoProfile),
        Box::new(JavaProfile),
        Box::new(CProfile),
        Box::new(CppProfile),
    ]
}
