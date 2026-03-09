use tree_sitter::Node;
use super::{Symbol, SymbolKind, node_text, child_by_field};

pub(super) fn extract_go(node: &Node, source: &[u8], symbols: &mut Vec<Symbol>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let is_exported = name.starts_with(|c: char| c.is_uppercase());
                    symbols.push(Symbol::new(name, SymbolKind::Function, &child, is_exported, None));
                }
            }
            "method_declaration" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let is_exported = name.starts_with(|c: char| c.is_uppercase());
                    let parent = extract_go_receiver(&child, source);
                    let mut sym = Symbol::new(name, SymbolKind::Method, &child, is_exported, parent);
                    sym.param_count = child.child_by_field_name("parameters")
                        .map(|p| count_actual_params(&p));
                    symbols.push(sym);
                }
            }
            "type_declaration" => {
                let mut inner_cursor = child.walk();
                for spec in child.children(&mut inner_cursor) {
                    if spec.kind() == "type_spec" {
                        if let Some(name) = child_by_field(&spec, "name", source) {
                            let is_exported = name.starts_with(|c: char| c.is_uppercase());
                            let type_node = spec.child_by_field_name("type");
                            let kind = match type_node.as_ref().map(|t| t.kind()) {
                                Some("struct_type") => SymbolKind::Struct,
                                Some("interface_type") => SymbolKind::Interface,
                                _ => SymbolKind::Type,
                            };
                            symbols.push(Symbol::new(name.clone(), kind, &spec, is_exported, None));

                            if let Some(ref tn) = type_node {
                                match tn.kind() {
                                    "struct_type" => {
                                        extract_go_struct_fields(tn, source, symbols, &name);
                                    }
                                    "interface_type" => {
                                        extract_go_interface_methods(tn, source, symbols, &name);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                extract_go(&child, source, symbols);
            }
        }
    }
}

/// Count the number of actual parameters in a Go parameter_list node.
/// Handles `(a, b int)` (2 names in one declaration) and `(a int, b int)` (separate declarations).
fn count_actual_params(param_list: &Node) -> usize {
    let mut count = 0;
    let mut cursor = param_list.walk();
    for child in param_list.children(&mut cursor) {
        match child.kind() {
            "parameter_declaration" => {
                // Count identifier children (parameter names); if none, still count as 1
                let mut id_count = 0;
                let mut cc = child.walk();
                for c in child.children(&mut cc) {
                    if c.kind() == "identifier" {
                        id_count += 1;
                    }
                }
                count += id_count.max(1);
            }
            "variadic_parameter_declaration" => {
                count += 1;
            }
            _ => {}
        }
    }
    count
}

fn extract_go_receiver(method_node: &Node, source: &[u8]) -> Option<String> {
    let receiver = method_node.child_by_field_name("receiver")?;
    let mut cursor = receiver.walk();
    let param = receiver
        .children(&mut cursor)
        .find(|n| n.kind() == "parameter_declaration")?;
    let type_node = param.child_by_field_name("type")?;
    let text = node_text(&type_node, source);
    Some(text.trim_start_matches('*').to_string())
}

/// Extract method signatures from interface definitions (for implicit implements detection).
fn extract_go_interface_methods(iface_node: &Node, source: &[u8], symbols: &mut Vec<Symbol>, iface_name: &str) {
    let mut cursor = iface_node.walk();
    for child in iface_node.children(&mut cursor) {
        if child.kind() == "method_elem" {
            // method_elem uses field_identifier as first child, not a "name" field
            let name = child.child(0).and_then(|n| {
                if n.kind() == "field_identifier" { Some(node_text(&n, source)) } else { None }
            });
            if let Some(name) = name {
                let is_exported = name.starts_with(|c: char| c.is_uppercase());
                let mut sym = Symbol::new(
                    name, SymbolKind::Method, &child, is_exported, Some(iface_name.to_string()),
                );
                // Find the parameter_list child of method_elem for param_count
                for i in 0..child.child_count() {
                    let c = child.child(i).unwrap();
                    if c.kind() == "parameter_list" {
                        sym.param_count = Some(count_actual_params(&c));
                        break;
                    }
                }
                symbols.push(sym);
            }
        }
    }
}

fn extract_go_struct_fields(struct_node: &Node, source: &[u8], symbols: &mut Vec<Symbol>, struct_name: &str) {
    let mut cursor = struct_node.walk();
    for child in struct_node.children(&mut cursor) {
        if child.kind() == "field_declaration_list" {
            let mut fc = child.walk();
            for field in child.children(&mut fc) {
                if field.kind() == "field_declaration" {
                    if let Some(name) = child_by_field(&field, "name", source) {
                        let is_exported = name.starts_with(|c: char| c.is_uppercase());
                        symbols.push(Symbol::new(
                            name, SymbolKind::Property, &field, is_exported, Some(struct_name.to_string()),
                        ));
                    }
                }
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
        parser.set_language(&tree_sitter_go::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_symbols(Language::Go, &tree, source.as_bytes())
    }

    fn names_of_kind(symbols: &[Symbol], kind: SymbolKind) -> Vec<String> {
        symbols.iter().filter(|s| s.kind == kind).map(|s| s.name.clone()).collect()
    }

    #[test]
    fn function_and_method() {
        let src = "package main\nfunc HandleHealth() {}\nfunc (s *Server) Start() {}";
        let symbols = parse_and_extract(src);
        let fns = names_of_kind(&symbols, SymbolKind::Function);
        assert_eq!(fns, vec!["HandleHealth"]);
        let methods = names_of_kind(&symbols, SymbolKind::Method);
        assert_eq!(methods, vec!["Start"]);
        let start = symbols.iter().find(|s| s.name == "Start").unwrap();
        assert_eq!(start.parent_name.as_deref(), Some("Server"));
    }

    #[test]
    fn struct_with_fields() {
        let src = "package main\ntype Config struct {\n\tTimeout int\n\tverbose bool\n}";
        let symbols = parse_and_extract(src);
        let structs = names_of_kind(&symbols, SymbolKind::Struct);
        assert_eq!(structs, vec!["Config"]);
        let props = names_of_kind(&symbols, SymbolKind::Property);
        assert!(props.contains(&"Timeout".to_string()));
        assert!(props.contains(&"verbose".to_string()));
        assert!(symbols.iter().find(|s| s.name == "Timeout").unwrap().is_exported);
        assert!(!symbols.iter().find(|s| s.name == "verbose").unwrap().is_exported);
    }

    #[test]
    fn interface() {
        let src = "package main\ntype Runner interface {\n\tRun(target string) error\n\tStatus() string\n}";
        let symbols = parse_and_extract(src);
        let ifaces = names_of_kind(&symbols, SymbolKind::Interface);
        assert_eq!(ifaces, vec!["Runner"]);
        // Interface method specs should be extracted as Method symbols
        let methods: Vec<_> = symbols.iter()
            .filter(|s| s.kind == SymbolKind::Method && s.parent_name.as_deref() == Some("Runner"))
            .map(|s| s.name.clone())
            .collect();
        assert!(methods.contains(&"Run".to_string()));
        assert!(methods.contains(&"Status".to_string()));
    }

    #[test]
    fn exported_detection() {
        let src = "package main\nfunc Public() {}\nfunc private() {}";
        let symbols = parse_and_extract(src);
        assert!(symbols.iter().find(|s| s.name == "Public").unwrap().is_exported);
        assert!(!symbols.iter().find(|s| s.name == "private").unwrap().is_exported);
    }
}
