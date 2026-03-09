use tree_sitter::Node;
use super::{Symbol, SymbolKind, node_text, child_by_field};

pub(super) fn extract_ruby(node: &Node, source: &[u8], symbols: &mut Vec<Symbol>, parent: Option<&str>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let superclass = child
                        .child_by_field_name("superclass")
                        .map(|n| {
                            let text = node_text(&n, source);
                            text.trim_start_matches('<').trim().to_string()
                        })
                        .filter(|s| !s.is_empty());
                    let mut sym = Symbol::new(
                        name.clone(), SymbolKind::Class, &child, true, parent.map(String::from),
                    );
                    sym.superclass = superclass;
                    symbols.push(sym);
                    extract_ruby(&child, source, symbols, Some(&name));
                }
            }
            "module" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    symbols.push(Symbol::new(
                        name.clone(), SymbolKind::Module, &child, true, parent.map(String::from),
                    ));
                    extract_ruby(&child, source, symbols, Some(&name));
                }
            }
            "method" | "singleton_method" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let kind = if name == "initialize" {
                        SymbolKind::Constructor
                    } else if parent.is_some() {
                        SymbolKind::Method
                    } else {
                        SymbolKind::Function
                    };
                    symbols.push(Symbol::new(
                        name, kind, &child, true, parent.map(String::from),
                    ));
                }
            }
            // attr_accessor, include/extend/prepend, and other DSL calls
            "call" if parent.is_some() => {
                let method = child
                    .child_by_field_name("method")
                    .map(|n| node_text(&n, source));
                if matches!(method.as_deref(), Some("attr_accessor") | Some("attr_reader") | Some("attr_writer")) {
                    if let Some(args) = child.child_by_field_name("arguments") {
                        let mut ac = args.walk();
                        for arg in args.children(&mut ac) {
                            if arg.kind() == "simple_symbol" {
                                let name = node_text(&arg, source)
                                    .trim_start_matches(':')
                                    .to_string();
                                if !name.is_empty() {
                                    symbols.push(Symbol::new(
                                        name, SymbolKind::Property, &arg, true, parent.map(String::from),
                                    ));
                                }
                            }
                        }
                    }
                } else if matches!(method.as_deref(), Some("include") | Some("extend") | Some("prepend")) {
                    // include/extend/prepend → populate parent's interfaces for IMPLEMENTS edges
                    if let Some(args) = child.child_by_field_name("arguments") {
                        let mut ac = args.walk();
                        for arg in args.children(&mut ac) {
                            if matches!(arg.kind(), "constant" | "scope_resolution") {
                                let mod_name = node_text(&arg, source);
                                if !mod_name.is_empty() {
                                    // Find the parent symbol and add to its interfaces
                                    if let Some(parent_sym) = symbols.iter_mut().rev().find(|s| {
                                        s.name == parent.unwrap()
                                            && matches!(s.kind, SymbolKind::Class | SymbolKind::Module)
                                    }) {
                                        parent_sym.interfaces.push(mod_name);
                                    }
                                }
                            }
                        }
                    }
                    extract_ruby(&child, source, symbols, parent);
                } else {
                    extract_ruby(&child, source, symbols, parent);
                }
            }
            // Constant assignment (e.g., STEPS = [...])
            "casgn" => {
                let text = node_text(&child, source);
                if !text.is_empty() && text.chars().next().map_or(false, |c| c.is_uppercase()) {
                    symbols.push(Symbol::new(
                        text, SymbolKind::Constant, &child, true, parent.map(String::from),
                    ));
                }
            }
            // Bare constant reference — skip if part of scope_resolution or class/module name
            "constant" => {
                let dominated = child.parent().map_or(false, |p| {
                    matches!(p.kind(), "scope_resolution" | "class" | "module" | "superclass")
                });
                if !dominated {
                    let text = node_text(&child, source);
                    if !text.is_empty() && text.chars().next().map_or(false, |c| c.is_uppercase()) {
                        symbols.push(Symbol::new(
                            text, SymbolKind::Constant, &child, true, parent.map(String::from),
                        ));
                    }
                }
            }
            // Qualified constant (e.g., Services::CreateOrder) — emit as single Constant
            // Skip if it's being used as a class/module name or superclass (already handled)
            "scope_resolution" => {
                let is_name_or_super = child.parent().map_or(false, |p| {
                    matches!(p.kind(), "class" | "module" | "superclass")
                });
                if !is_name_or_super {
                    let text = node_text(&child, source);
                    if !text.is_empty() {
                        symbols.push(Symbol::new(
                            text, SymbolKind::Constant, &child, true, parent.map(String::from),
                        ));
                    }
                }
            }
            _ => {
                extract_ruby(&child, source, symbols, parent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::extract::extract_symbols;
    use crate::parser::extract::{SymbolKind, Symbol};
    use crate::parser::Language;
    use tree_sitter::Parser;

    fn parse_and_extract(source: &str) -> Vec<Symbol> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_ruby::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_symbols(Language::Ruby, &tree, source.as_bytes())
    }

    fn names_of_kind(symbols: &[Symbol], kind: SymbolKind) -> Vec<String> {
        symbols.iter().filter(|s| s.kind == kind).map(|s| s.name.clone()).collect()
    }

    #[test]
    fn class_with_superclass() {
        let src = "class User < Base\n  def full_name\n    name\n  end\nend";
        let symbols = parse_and_extract(src);
        let classes = names_of_kind(&symbols, SymbolKind::Class);
        assert_eq!(classes, vec!["User"]);
        let user = symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user.superclass.as_deref(), Some("Base"));
        let methods = names_of_kind(&symbols, SymbolKind::Method);
        assert_eq!(methods, vec!["full_name"]);
        assert_eq!(
            symbols.iter().find(|s| s.name == "full_name").unwrap().parent_name.as_deref(),
            Some("User")
        );
    }

    #[test]
    fn module_and_singleton_method() {
        let src = "module Auth\n  def self.verify(token)\n    true\n  end\n  def check\n    false\n  end\nend";
        let symbols = parse_and_extract(src);
        assert!(names_of_kind(&symbols, SymbolKind::Module).contains(&"Auth".to_string()));
        let methods = names_of_kind(&symbols, SymbolKind::Method);
        assert!(methods.contains(&"verify".to_string()));
        assert!(methods.contains(&"check".to_string()));
    }

    #[test]
    fn attr_accessor() {
        let src = "class User\n  attr_reader :name, :email\n  attr_accessor :role\nend";
        let symbols = parse_and_extract(src);
        let props = names_of_kind(&symbols, SymbolKind::Property);
        assert!(props.contains(&"name".to_string()));
        assert!(props.contains(&"email".to_string()));
        assert!(props.contains(&"role".to_string()));
    }

    #[test]
    fn include_populates_interfaces() {
        let src = "class User\n  include Concerns::Validatable\n  extend Concerns::Trackable\nend";
        let symbols = parse_and_extract(src);
        let user = symbols.iter().find(|s| s.name == "User" && s.kind == SymbolKind::Class).unwrap();
        assert!(user.interfaces.contains(&"Concerns::Validatable".to_string()));
        assert!(user.interfaces.contains(&"Concerns::Trackable".to_string()));
    }

    #[test]
    fn scope_resolution_not_duplicated() {
        let src = "module Services\n  class Checkout\n    STEPS = [Services::CreateOrder, Services::SendReceipt]\n    include Concerns::Trackable\n  end\nend";
        let symbols = parse_and_extract(src);
        let constants = names_of_kind(&symbols, SymbolKind::Constant);
        // STEPS is a constant assignment, scope_resolution refs are single entries
        assert!(constants.contains(&"STEPS".to_string()));
        assert!(constants.contains(&"Services::CreateOrder".to_string()));
        assert!(constants.contains(&"Services::SendReceipt".to_string()));
        assert!(constants.contains(&"Concerns::Trackable".to_string()));
        // Individual parts (Services, CreateOrder, etc.) should NOT appear as separate Constants
        assert!(!constants.iter().any(|c| c == "CreateOrder"));
        assert!(!constants.iter().any(|c| c == "SendReceipt"));
    }

    #[test]
    fn constructor() {
        let src = "class User\n  def initialize(name)\n    @name = name\n  end\nend";
        let symbols = parse_and_extract(src);
        let ctors = names_of_kind(&symbols, SymbolKind::Constructor);
        assert_eq!(ctors, vec!["initialize"]);
    }
}
