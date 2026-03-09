mod go;
mod ruby;
mod typescript;

use super::Language;
use tree_sitter::Tree;

/// A raw import extracted from source code.
#[derive(Debug, Clone)]
pub struct RawImport {
    /// The import path as written in source (e.g., "./auth", "net/http", "bcrypt")
    pub source_path: String,
    /// Specific names imported (empty = wildcard/default import)
    pub imported_names: Vec<String>,
    #[allow(dead_code)]
    pub line: usize,
}

/// Extract import statements from a parsed tree.
pub fn extract_imports(language: Language, tree: &Tree, source: &[u8]) -> Vec<RawImport> {
    let root = tree.root_node();
    let mut imports = Vec::new();

    match language {
        Language::Ruby => ruby::extract_ruby_imports(&root, source, &mut imports),
        Language::Go => go::extract_go_imports(&root, source, &mut imports),
        Language::TypeScript => typescript::extract_ts_imports(&root, source, &mut imports),
    }

    imports
}

pub(super) fn node_text(node: &tree_sitter::Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}
