use tree_sitter::Node;
use super::{Symbol, SymbolKind, node_text, child_by_field};

pub(super) fn extract_typescript(
    node: &Node,
    source: &[u8],
    symbols: &mut Vec<Symbol>,
    parent: Option<&str>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let is_exported = is_ts_exported(&child);
                    symbols.push(Symbol::new(
                        name, SymbolKind::Function, &child, is_exported, parent.map(String::from),
                    ));
                }
            }
            "method_definition" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let kind = if name == "constructor" {
                        SymbolKind::Constructor
                    } else {
                        SymbolKind::Method
                    };
                    symbols.push(Symbol::new(
                        name.clone(), kind, &child, false, parent.map(String::from),
                    ));
                    // Extract constructor parameter property shorthands (e.g., `private service: UserService`)
                    if name == "constructor" {
                        if let Some(params) = child.child_by_field_name("parameters") {
                            extract_ts_constructor_params(&params, source, symbols, parent);
                        }
                    }
                }
            }
            // Class properties: public foo: string, private bar = 42
            "public_field_definition" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let mut sym = Symbol::new(
                        name, SymbolKind::Property, &child, false, parent.map(String::from),
                    );
                    sym.type_annotation = extract_ts_type_annotation(&child, source);
                    symbols.push(sym);
                }
            }
            // Interface property signatures — skip inline object types (return types etc.)
            "property_signature" => {
                if !is_inline_type_shape(&child) {
                    if let Some(name) = child_by_field(&child, "name", source) {
                        symbols.push(Symbol::new(
                            name, SymbolKind::Property, &child, false, parent.map(String::from),
                        ));
                    }
                }
            }
            "class_declaration" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let is_exported = is_ts_exported(&child);
                    let mut sym = Symbol::new(
                        name.clone(), SymbolKind::Class, &child, is_exported, parent.map(String::from),
                    );

                    // Extract extends and implements from class heritage
                    let mut hc = child.walk();
                    for hchild in child.children(&mut hc) {
                        if hchild.kind() == "class_heritage" {
                            extract_ts_heritage(&hchild, source, &mut sym);
                        }
                    }

                    symbols.push(sym);
                    extract_typescript(&child, source, symbols, Some(&name));
                    continue;
                }
            }
            "interface_declaration" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let is_exported = is_ts_exported(&child);
                    let mut sym = Symbol::new(
                        name.clone(), SymbolKind::Interface, &child, is_exported, parent.map(String::from),
                    );

                    // Interface can also extend other interfaces
                    let mut hc = child.walk();
                    for hchild in child.children(&mut hc) {
                        if hchild.kind() == "extends_type_clause" {
                            let mut ec = hchild.walk();
                            for ext in hchild.children(&mut ec) {
                                if ext.kind() == "type_identifier" || ext.kind() == "identifier" {
                                    sym.interfaces.push(node_text(&ext, source));
                                }
                            }
                        }
                    }

                    symbols.push(sym);

                    // Extract interface method signatures as Method symbols
                    extract_ts_interface_methods(&child, source, symbols, &name);
                }
            }
            "enum_declaration" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let is_exported = is_ts_exported(&child);
                    symbols.push(Symbol::new(
                        name, SymbolKind::Enum, &child, is_exported, parent.map(String::from),
                    ));
                }
            }
            "type_alias_declaration" => {
                if let Some(name) = child_by_field(&child, "name", source) {
                    let is_exported = is_ts_exported(&child);
                    symbols.push(Symbol::new(
                        name, SymbolKind::Type, &child, is_exported, parent.map(String::from),
                    ));
                }
            }
            // const foo = () => {} / const bar = function() {}
            "lexical_declaration" | "variable_declaration" => {
                let is_exported = is_ts_exported(&child);
                let mut dc = child.walk();
                for decl in child.children(&mut dc) {
                    if decl.kind() == "variable_declarator" {
                        if let Some(name) = child_by_field(&decl, "name", source) {
                            if let Some(value) = decl.child_by_field_name("value") {
                                match value.kind() {
                                    "arrow_function" | "function_expression" => {
                                        symbols.push(Symbol::new(
                                            name, SymbolKind::Function, &decl, is_exported, parent.map(String::from),
                                        ));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                // Still recurse for nested content
                extract_typescript(&child, source, symbols, parent);
                continue;
            }
            "export_statement" => {
                extract_typescript(&child, source, symbols, parent);
                continue;
            }
            _ => {}
        }
        if child.child_count() > 0 && child.kind() != "class_declaration" {
            extract_typescript(&child, source, symbols, parent);
        }
    }
}

/// Extract simple type annotation from a node's "type" field child.
/// Returns the type name for simple types (e.g., "UserService"), None for generics/unions.
fn extract_ts_type_annotation(node: &Node, source: &[u8]) -> Option<String> {
    let type_node = node.child_by_field_name("type")?;
    // type_annotation node wraps the actual type — find the type_identifier or identifier inside
    let mut cursor = type_node.walk();
    for child in type_node.children(&mut cursor) {
        if child.kind() == "type_identifier" || child.kind() == "identifier" {
            return Some(node_text(&child, source));
        }
    }
    // Fallback: if the type node itself is a type_identifier
    if type_node.kind() == "type_identifier" || type_node.kind() == "identifier" {
        return Some(node_text(&type_node, source));
    }
    None
}

/// Check whether a node has an accessibility modifier child (public/private/protected/readonly).
fn has_accessibility_modifier(node: &Node) -> bool {
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if matches!(child.kind(), "accessibility_modifier" | "readonly") {
            return true;
        }
    }
    false
}

/// Extract constructor parameter property shorthands as Property symbols.
/// `constructor(private service: UserService)` emits a Property "service" with type_annotation "UserService".
fn extract_ts_constructor_params(
    params_node: &Node,
    source: &[u8],
    symbols: &mut Vec<Symbol>,
    parent: Option<&str>,
) {
    let mut cursor = params_node.walk();
    for param in params_node.children(&mut cursor) {
        if matches!(param.kind(), "required_parameter" | "optional_parameter")
            && has_accessibility_modifier(&param)
        {
            // Get parameter name (identifier or destructuring pattern)
            let name = param.child_by_field_name("name")
                .map(|n| node_text(&n, source))
                .or_else(|| {
                    // Fallback: first identifier child
                    for i in 0..param.child_count() {
                        let c = param.child(i).unwrap();
                        if c.kind() == "identifier" {
                            return Some(node_text(&c, source));
                        }
                    }
                    None
                });
            if let Some(name) = name {
                let mut sym = Symbol::new(
                    name, SymbolKind::Property, &param, false, parent.map(String::from),
                );
                sym.type_annotation = extract_ts_type_annotation(&param, source);
                symbols.push(sym);
            }
        }
    }
}

fn extract_ts_heritage(heritage_node: &Node, source: &[u8], sym: &mut Symbol) {
    let mut cursor = heritage_node.walk();
    for child in heritage_node.children(&mut cursor) {
        match child.kind() {
            "extends_clause" => {
                let mut ec = child.walk();
                for ext_child in child.children(&mut ec) {
                    if ext_child.kind() == "identifier" || ext_child.kind() == "type_identifier" {
                        sym.superclass = Some(node_text(&ext_child, source));
                    }
                }
            }
            "implements_clause" => {
                let mut ic = child.walk();
                for impl_child in child.children(&mut ic) {
                    if impl_child.kind() == "type_identifier" || impl_child.kind() == "identifier" {
                        sym.interfaces.push(node_text(&impl_child, source));
                    }
                    if impl_child.kind() == "generic_type" {
                        if let Some(name_node) = impl_child.child(0) {
                            sym.interfaces.push(node_text(&name_node, source));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract method signatures from interface bodies (for cross-class delegation resolution).
fn extract_ts_interface_methods(iface_node: &Node, source: &[u8], symbols: &mut Vec<Symbol>, iface_name: &str) {
    let mut cursor = iface_node.walk();
    for child in iface_node.children(&mut cursor) {
        if child.kind() == "interface_body" || child.kind() == "object_type" {
            let mut bc = child.walk();
            for member in child.children(&mut bc) {
                if member.kind() == "method_signature" {
                    if let Some(name) = child_by_field(&member, "name", source) {
                        symbols.push(Symbol::new(
                            name, SymbolKind::Method, &member, false, Some(iface_name.to_string()),
                        ));
                    }
                }
            }
        }
    }
}

/// Check if a property_signature is inside an inline object type (not an interface body).
/// Returns true for properties in return type annotations, parameter types, etc.
fn is_inline_type_shape(node: &Node) -> bool {
    let mut current = node.parent();
    while let Some(p) = current {
        match p.kind() {
            // Inside an interface body — this is a real property signature
            "interface_body" => return false,
            // Inside an object_type that's NOT part of an interface — inline shape
            "object_type" => {
                let gp = p.parent();
                if gp.as_ref().map_or(false, |g| g.kind() == "interface_body") {
                    return false;
                }
                return true;
            }
            _ => {}
        }
        current = p.parent();
    }
    false
}

fn is_ts_exported(node: &Node) -> bool {
    node.parent()
        .map_or(false, |p| p.kind() == "export_statement")
}

#[cfg(test)]
mod tests {
    use crate::parser::extract::extract_symbols;
    use crate::parser::extract::{SymbolKind, Symbol};
    use crate::parser::Language;
    use tree_sitter::Parser;

    fn parse_and_extract(source: &str) -> Vec<Symbol> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_symbols(Language::TypeScript, &tree, source.as_bytes())
    }

    fn names_of_kind(symbols: &[Symbol], kind: SymbolKind) -> Vec<String> {
        symbols.iter().filter(|s| s.kind == kind).map(|s| s.name.clone()).collect()
    }

    #[test]
    fn function_declaration() {
        let src = r#"export function greet(name: string): string { return name; }"#;
        let symbols = parse_and_extract(src);
        let fns = names_of_kind(&symbols, SymbolKind::Function);
        assert_eq!(fns, vec!["greet"]);
        assert!(symbols.iter().find(|s| s.name == "greet").unwrap().is_exported);
    }

    #[test]
    fn arrow_function() {
        let src = r#"export const add = (a: number, b: number) => a + b;"#;
        let symbols = parse_and_extract(src);
        let fns = names_of_kind(&symbols, SymbolKind::Function);
        assert_eq!(fns, vec!["add"]);
    }

    #[test]
    fn class_with_heritage() {
        let src = "export class Admin extends User implements Serializable {\n  constructor(private name: string) {}\n  greet(): string { return this.name; }\n}";
        let symbols = parse_and_extract(src);
        let classes = names_of_kind(&symbols, SymbolKind::Class);
        assert_eq!(classes, vec!["Admin"]);
        let admin = symbols.iter().find(|s| s.name == "Admin").unwrap();
        assert_eq!(admin.superclass.as_deref(), Some("User"));
        assert_eq!(admin.interfaces, vec!["Serializable"]);

        let methods = names_of_kind(&symbols, SymbolKind::Method);
        assert!(methods.contains(&"greet".to_string()));

        let ctors = names_of_kind(&symbols, SymbolKind::Constructor);
        assert_eq!(ctors, vec!["constructor"]);
    }

    #[test]
    fn interface_and_type() {
        let src = "export interface Repository<T> {\n  findById(id: string): T;\n  save(entity: T): T;\n}\nexport type UserId = string;\nexport enum Role { Admin, User }";
        let symbols = parse_and_extract(src);
        assert!(names_of_kind(&symbols, SymbolKind::Interface).contains(&"Repository".to_string()));
        assert!(names_of_kind(&symbols, SymbolKind::Type).contains(&"UserId".to_string()));
        assert!(names_of_kind(&symbols, SymbolKind::Enum).contains(&"Role".to_string()));
        // Interface method signatures should be extracted as Method symbols
        let iface_methods: Vec<_> = symbols.iter()
            .filter(|s| s.kind == SymbolKind::Method && s.parent_name.as_deref() == Some("Repository"))
            .map(|s| s.name.clone())
            .collect();
        assert!(iface_methods.contains(&"findById".to_string()), "interface methods: {:?}", iface_methods);
        assert!(iface_methods.contains(&"save".to_string()));
    }

    #[test]
    fn interface_method_with_generics() {
        // Mimics types/index.ts with multiple interfaces
        let src = r#"export interface Entity {
  id: string;
}

export interface Repository<T extends Entity> {
  findById(id: string): Promise<T | null>;
  findAll(): Promise<T[]>;
  save(entity: T): Promise<T>;
  delete(id: string): Promise<void>;
}

export interface Response {
  status(code: number): Response;
  json(data: unknown): void;
}"#;
        let symbols = parse_and_extract(src);
        let all: Vec<_> = symbols.iter().map(|s| format!("{}:{}(parent={:?})", s.kind, s.name, s.parent_name)).collect();
        let repo_methods: Vec<_> = symbols.iter()
            .filter(|s| s.kind == SymbolKind::Method && s.parent_name.as_deref() == Some("Repository"))
            .map(|s| s.name.clone())
            .collect();
        assert!(repo_methods.contains(&"findById".to_string()), "repo methods: {:?}, all: {:?}", repo_methods, all);
        assert!(repo_methods.contains(&"save".to_string()));
        // Response interface methods
        let resp_methods: Vec<_> = symbols.iter()
            .filter(|s| s.kind == SymbolKind::Method && s.parent_name.as_deref() == Some("Response"))
            .map(|s| s.name.clone())
            .collect();
        assert!(resp_methods.contains(&"status".to_string()), "response methods: {:?}", resp_methods);
    }

    #[test]
    fn not_exported() {
        let src = r#"function helper() {} class Internal {}"#;
        let symbols = parse_and_extract(src);
        assert!(!symbols.iter().find(|s| s.name == "helper").unwrap().is_exported);
        assert!(!symbols.iter().find(|s| s.name == "Internal").unwrap().is_exported);
    }
}
