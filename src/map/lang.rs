//! Language registry: extension → grammar, definition query, import query.
//!
//! Symbol *kind* is derived from the node kind of the `@def` capture rather
//! than from capture names. That keeps the queries short and means a grammar
//! that renames a capture convention cannot silently drop symbols — an unknown
//! node kind is visible as an unmapped kind instead.

use tree_sitter::{Language, Query};

/// The three queries keel runs over every file.
pub struct Compiled {
    pub defs: Query,
    pub imports: Query,
    pub refs: Query,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    Java,
    CSharp,
}

impl Lang {
    pub fn name(&self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::JavaScript => "javascript",
            Lang::TypeScript => "typescript",
            Lang::Tsx => "tsx",
            Lang::Go => "go",
            Lang::Java => "java",
            Lang::CSharp => "csharp",
        }
    }

    pub fn from_path(path: &std::path::Path) -> Option<Lang> {
        let ext = path.extension()?.to_str()?;
        Some(match ext {
            "rs" => Lang::Rust,
            "py" | "pyi" => Lang::Python,
            "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
            "ts" | "mts" | "cts" => Lang::TypeScript,
            "tsx" => Lang::Tsx,
            "go" => Lang::Go,
            "java" => Lang::Java,
            "cs" => Lang::CSharp,
            _ => return None,
        })
    }

    pub fn all() -> &'static [Lang] {
        &[Lang::Rust, Lang::Python, Lang::JavaScript, Lang::TypeScript, Lang::Tsx, Lang::Go, Lang::Java, Lang::CSharp]
    }

    pub fn language(&self) -> Language {
        match self {
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
            Lang::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        }
    }

    /// Query capturing `@def` (the definition node) and `@name` (its identifier).
    pub fn def_query_source(&self) -> &'static str {
        match self {
            Lang::Rust => RUST_DEFS,
            Lang::Python => PYTHON_DEFS,
            Lang::JavaScript => JS_DEFS,
            Lang::TypeScript | Lang::Tsx => TS_DEFS,
            Lang::Go => GO_DEFS,
            Lang::Java => JAVA_DEFS,
            Lang::CSharp => CSHARP_DEFS,
        }
    }

    /// Query capturing `@ref` — every identifier *used* in the file.
    ///
    /// Deliberately over-broad: it captures definition names too, which the
    /// extractor then subtracts. Trying to exclude them in the query means one
    /// negative pattern per definition form per language, and a grammar change
    /// would silently start reporting definitions as references.
    pub fn ref_query_source(&self) -> &'static str {
        match self {
            Lang::Rust => RUST_REFS,
            Lang::Python => PYTHON_REFS,
            Lang::JavaScript => JS_REFS,
            Lang::TypeScript | Lang::Tsx => TS_REFS,
            Lang::Go => GO_REFS,
            Lang::Java => JAVA_REFS,
            Lang::CSharp => CSHARP_REFS,
        }
    }

    /// Query capturing `@path` — the raw, unresolved import string.
    pub fn import_query_source(&self) -> &'static str {
        match self {
            Lang::Rust => RUST_IMPORTS,
            Lang::Python => PYTHON_IMPORTS,
            Lang::JavaScript | Lang::TypeScript | Lang::Tsx => JS_IMPORTS,
            Lang::Go => GO_IMPORTS,
            Lang::Java => JAVA_IMPORTS,
            Lang::CSharp => CSHARP_IMPORTS,
        }
    }

    /// Node kind → the short symbol kind shown in maps. `None` means "indexed
    /// but not worth a line in a budget-fitted map".
    pub fn symbol_kind(&self, node_kind: &str) -> Option<&'static str> {
        Some(match node_kind {
            // rust
            "function_item" | "function_signature_item" => "fn",
            "struct_item" => "struct",
            "enum_item" => "enum",
            "trait_item" => "trait",
            "mod_item" => "mod",
            "type_item" => "type",
            "const_item" => "const",
            "static_item" => "static",
            "macro_definition" => "macro",
            "impl_item" => "impl",
            // python
            "function_definition" => "fn",
            "class_definition" => "class",
            // js / ts
            "function_declaration" | "generator_function_declaration" => "fn",
            "class_declaration" | "abstract_class_declaration" => "class",
            "method_definition" => "method",
            "interface_declaration" => "interface",
            "type_alias_declaration" => "type",
            "enum_declaration" => "enum",
            "variable_declarator" => "const",
            // go
            "method_declaration" => "method",
            "type_spec" => "type",
            // java
            "record_declaration" => "record",
            "constructor_declaration" => "ctor",
            // c#
            "struct_declaration" => "struct",
            "property_declaration" => "property",
            "namespace_declaration" => "namespace",
            "delegate_declaration" => "delegate",
            _ => return None,
        })
    }

    /// Compile both queries. Returns `Err` only on a genuine grammar mismatch,
    /// which the caller downgrades to "this language is unavailable this run"
    /// rather than aborting the map (P4: the index is an accelerator).
    pub fn compile(&self) -> Result<Compiled, tree_sitter::QueryError> {
        let l = self.language();
        Ok(Compiled {
            defs: Query::new(&l, self.def_query_source())?,
            imports: Query::new(&l, self.import_query_source())?,
            refs: Query::new(&l, self.ref_query_source())?,
        })
    }
}

const RUST_REFS: &str = r#"
(identifier) @ref
(type_identifier) @ref
(field_identifier) @ref
"#;

const PYTHON_REFS: &str = r#"
(identifier) @ref
"#;

const JS_REFS: &str = r#"
(identifier) @ref
(property_identifier) @ref
"#;

const TS_REFS: &str = r#"
(identifier) @ref
(type_identifier) @ref
(property_identifier) @ref
"#;

const GO_REFS: &str = r#"
(identifier) @ref
(type_identifier) @ref
(field_identifier) @ref
"#;

const JAVA_REFS: &str = r#"
(identifier) @ref
(type_identifier) @ref
"#;

const RUST_DEFS: &str = r#"
(function_item name: (identifier) @name) @def
(function_signature_item name: (identifier) @name) @def
(struct_item name: (type_identifier) @name) @def
(enum_item name: (type_identifier) @name) @def
(trait_item name: (type_identifier) @name) @def
(mod_item name: (identifier) @name) @def
(type_item name: (type_identifier) @name) @def
(const_item name: (identifier) @name) @def
(static_item name: (identifier) @name) @def
(macro_definition name: (identifier) @name) @def
(impl_item type: (type_identifier) @name) @def
(impl_item type: (generic_type type: (type_identifier) @name)) @def
"#;

const RUST_IMPORTS: &str = r#"
(use_declaration argument: (_) @path)
(mod_item name: (identifier) @path)
"#;

const PYTHON_DEFS: &str = r#"
(function_definition name: (identifier) @name) @def
(class_definition name: (identifier) @name) @def
"#;

const PYTHON_IMPORTS: &str = r#"
(import_statement name: (dotted_name) @path)
(import_statement name: (aliased_import name: (dotted_name) @path))
(import_from_statement module_name: (dotted_name) @path)
(import_from_statement module_name: (relative_import) @path)
"#;

const JS_DEFS: &str = r#"
(function_declaration name: (identifier) @name) @def
(generator_function_declaration name: (identifier) @name) @def
(class_declaration name: (identifier) @name) @def
(method_definition name: (property_identifier) @name) @def
"#;

const TS_DEFS: &str = r#"
(function_declaration name: (identifier) @name) @def
(generator_function_declaration name: (identifier) @name) @def
(class_declaration name: (type_identifier) @name) @def
(abstract_class_declaration name: (type_identifier) @name) @def
(method_definition name: (property_identifier) @name) @def
(interface_declaration name: (type_identifier) @name) @def
(type_alias_declaration name: (type_identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
"#;

const JS_IMPORTS: &str = r#"
(import_statement source: (string (string_fragment) @path))
(export_statement source: (string (string_fragment) @path))
(call_expression
  function: (identifier) @_fn
  arguments: (arguments (string (string_fragment) @path))
  (#eq? @_fn "require"))
"#;

const GO_DEFS: &str = r#"
(function_declaration name: (identifier) @name) @def
(method_declaration name: (field_identifier) @name) @def
(type_spec name: (type_identifier) @name) @def
"#;

const GO_IMPORTS: &str = r#"
(import_spec path: (interpreted_string_literal) @path)
"#;

const JAVA_DEFS: &str = r#"
(class_declaration name: (identifier) @name) @def
(interface_declaration name: (identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(record_declaration name: (identifier) @name) @def
(method_declaration name: (identifier) @name) @def
(constructor_declaration name: (identifier) @name) @def
"#;

const JAVA_IMPORTS: &str = r#"
(import_declaration (scoped_identifier) @path)
"#;

const CSHARP_REFS: &str = r#"
(identifier) @ref
"#;

const CSHARP_DEFS: &str = r#"
(class_declaration name: (identifier) @name) @def
(interface_declaration name: (identifier) @name) @def
(struct_declaration name: (identifier) @name) @def
(enum_declaration name: (identifier) @name) @def
(record_declaration name: (identifier) @name) @def
(delegate_declaration name: (identifier) @name) @def
(method_declaration name: (identifier) @name) @def
(constructor_declaration name: (identifier) @name) @def
(property_declaration name: (identifier) @name) @def
(namespace_declaration name: (identifier) @name) @def
(namespace_declaration name: (qualified_name) @name) @def
"#;

// `record struct Tiny(int N)` parses as a plain record_declaration, so it is
// already covered above rather than needing a pattern of its own.
const CSHARP_IMPORTS: &str = r#"
(using_directive (identifier) @path)
(using_directive (qualified_name) @path)
"#;

/// Map a stored kind string back to the fixed vocabulary.
///
/// Symbol kinds are a closed set, so a round trip through the database should
/// not turn `&'static str` into an owned string for the whole index.
pub fn intern_kind(kind: &str) -> &'static str {
    const KINDS: &[&str] = &[
        "fn", "struct", "enum", "trait", "mod", "type", "const", "static", "macro",
        "impl", "class", "method", "interface", "record", "ctor",
        "property", "namespace", "delegate",
    ];
    KINDS.iter().find(|k| **k == kind).copied().unwrap_or("symbol")
}

/// Languages whose queries will not compile against the linked grammars.
///
/// A grammar upgrade that renames a node kind degrades keel silently otherwise:
/// files still get indexed, they just stop yielding symbols. Naming the casualty
/// is the difference between "the map is thin" and "the map is wrong".
pub fn unavailable() -> Vec<(&'static str, String)> {
    Lang::all()
        .iter()
        .filter_map(|l| l.compile().err().map(|e| (l.name(), e.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_compiles_its_queries() {
        for lang in Lang::all() {
            lang.compile()
                .unwrap_or_else(|e| panic!("{} queries failed to compile: {e}", lang.name()));
        }
    }

    #[test]
    fn extension_mapping() {
        use std::path::Path;
        assert_eq!(Lang::from_path(Path::new("a/b.rs")), Some(Lang::Rust));
        assert_eq!(Lang::from_path(Path::new("a/b.tsx")), Some(Lang::Tsx));
        assert_eq!(Lang::from_path(Path::new("a/b.md")), None);
    }
}
