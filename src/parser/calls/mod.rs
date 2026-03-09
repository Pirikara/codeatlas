mod go;
mod ruby;
mod typescript;

use super::Language;
use tree_sitter::{Node, Tree};

/// A raw function/method call extracted from source code.
#[derive(Debug, Clone)]
pub struct RawCall {
    /// Name of the function/method being called
    pub callee_name: String,
    /// The receiver expression if it's a method call (e.g., "userService" in userService.find())
    pub receiver: Option<String>,
    /// Name of the enclosing function/method that contains this call
    pub caller_name: Option<String>,
    /// Parent class/module of the caller (if any)
    pub caller_parent: Option<String>,
    #[allow(dead_code)]
    pub line: usize,
}

/// Extract function/method calls from a parsed tree.
pub fn extract_calls(language: Language, tree: &Tree, source: &[u8]) -> Vec<RawCall> {
    let root = tree.root_node();
    let mut calls = Vec::new();

    match language {
        Language::Ruby => ruby::extract_ruby_calls(&root, source, &mut calls),
        Language::Go => go::extract_go_calls(&root, source, &mut calls),
        Language::TypeScript => typescript::extract_ts_calls(&root, source, &mut calls),
    }

    calls
}

/// Walk up the AST to find the enclosing function/method name and its parent class.
pub(super) fn find_enclosing_function(node: &Node, source: &[u8]) -> (Option<String>, Option<String>) {
    let mut current = node.parent();
    let mut func_name = None;
    let mut class_name = None;

    while let Some(parent) = current {
        match parent.kind() {
            // Function-like nodes
            "function_declaration" | "method_definition" | "method" | "singleton_method"
            | "method_declaration" | "func_literal" => {
                if func_name.is_none() {
                    func_name = parent
                        .child_by_field_name("name")
                        .map(|n| node_text(&n, source));
                }
            }
            // Arrow functions / anonymous — use variable name if assigned
            "arrow_function" | "function" => {
                if func_name.is_none() {
                    // Check if parent is a variable_declarator
                    if let Some(gp) = parent.parent() {
                        if gp.kind() == "variable_declarator" {
                            func_name = gp
                                .child_by_field_name("name")
                                .map(|n| node_text(&n, source));
                        }
                    }
                }
            }
            // Class-like nodes
            "class_declaration" | "class" | "module" => {
                if class_name.is_none() {
                    class_name = parent
                        .child_by_field_name("name")
                        .map(|n| node_text(&n, source));
                }
            }
            _ => {}
        }
        current = parent.parent();
    }

    (func_name, class_name)
}

pub(super) fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}
