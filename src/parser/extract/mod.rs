mod ruby;
mod go;
mod typescript;

use super::Language;
use tree_sitter::{Node, Tree};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Module,
    Interface,
    Struct,
    Enum,
    Constant,
    Type,
    Property,
    Constructor,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "Function"),
            SymbolKind::Method => write!(f, "Method"),
            SymbolKind::Class => write!(f, "Class"),
            SymbolKind::Module => write!(f, "Module"),
            SymbolKind::Interface => write!(f, "Interface"),
            SymbolKind::Struct => write!(f, "Struct"),
            SymbolKind::Enum => write!(f, "Enum"),
            SymbolKind::Constant => write!(f, "Constant"),
            SymbolKind::Type => write!(f, "Type"),
            SymbolKind::Property => write!(f, "Property"),
            SymbolKind::Constructor => write!(f, "Constructor"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: String, // filled in later by the caller
    pub start_line: usize,
    pub end_line: usize,
    pub is_exported: bool,
    pub parent_name: Option<String>,
    /// Superclass name (Ruby: `< Base`, TS: `extends Base`)
    pub superclass: Option<String>,
    /// Implemented interfaces (TS: `implements Foo, Bar`)
    pub interfaces: Vec<String>,
    /// TypeScript: class field / constructor parameter type name e.g. "UserService" (in-memory only)
    pub type_annotation: Option<String>,
    /// Go: method parameter count for interface compatibility checks (in-memory only)
    pub param_count: Option<usize>,
}

impl Symbol {
    pub fn uid(&self) -> String {
        format!("{}:{}:{}:{}", self.kind, self.file_path, self.name, self.start_line)
    }

    pub(super) fn new(name: String, kind: SymbolKind, node: &Node, is_exported: bool, parent_name: Option<String>) -> Self {
        Self {
            name,
            kind,
            file_path: String::new(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            is_exported,
            parent_name,
            superclass: None,
            interfaces: vec![],
            type_annotation: None,
            param_count: None,
        }
    }
}

/// Extract symbols from a parsed tree based on language.
pub fn extract_symbols(language: Language, tree: &Tree, source: &[u8]) -> Vec<Symbol> {
    let root = tree.root_node();
    let mut symbols = Vec::new();

    match language {
        Language::Ruby => ruby::extract_ruby(&root, source, &mut symbols, None),
        Language::Go => go::extract_go(&root, source, &mut symbols),
        Language::TypeScript => typescript::extract_typescript(&root, source, &mut symbols, None),
    }

    symbols
}

// ─── Shared Helpers ─────────────────────────────────────────────

pub(super) fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

pub(super) fn child_by_field(node: &Node, field: &str, source: &[u8]) -> Option<String> {
    node.child_by_field_name(field)
        .map(|n| node_text(&n, source))
}
