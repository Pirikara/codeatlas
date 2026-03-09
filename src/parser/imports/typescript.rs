use super::{node_text, RawImport};
use tree_sitter::Node;

pub(super) fn extract_ts_imports(node: &Node, source: &[u8], imports: &mut Vec<RawImport>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import_statement" {
            let source_node = child.child_by_field_name("source");
            let source_path = source_node
                .map(|n| {
                    let text = node_text(&n, source);
                    text.trim_matches(|c| c == '\'' || c == '"').to_string()
                })
                .unwrap_or_default();

            if source_path.is_empty() {
                continue;
            }

            // Extract named imports: import { Foo, Bar } from "./mod"
            let mut names = Vec::new();
            let mut ic = child.walk();
            for c in child.children(&mut ic) {
                if c.kind() == "import_clause" {
                    collect_ts_import_names(&c, source, &mut names);
                }
            }

            imports.push(RawImport {
                source_path,
                imported_names: names,
                line: child.start_position().row + 1,
            });
        }
        // Also handle: const foo = require("./mod")
        if child.kind() == "lexical_declaration" || child.kind() == "variable_declaration" {
            let mut dc = child.walk();
            for decl in child.children(&mut dc) {
                if decl.kind() == "variable_declarator" {
                    if let Some(value) = decl.child_by_field_name("value") {
                        if value.kind() == "call_expression" {
                            let func = value.child_by_field_name("function");
                            if func.map(|f| node_text(&f, source)).as_deref() == Some("require") {
                                if let Some(args) = value.child_by_field_name("arguments") {
                                    let mut ac = args.walk();
                                    for arg in args.children(&mut ac) {
                                        if arg.kind() == "string" {
                                            let text = node_text(&arg, source);
                                            let text = text
                                                .trim_matches(|c| c == '\'' || c == '"')
                                                .to_string();
                                            if !text.is_empty() {
                                                let name = decl
                                                    .child_by_field_name("name")
                                                    .map(|n| node_text(&n, source));
                                                imports.push(RawImport {
                                                    source_path: text,
                                                    imported_names: name.into_iter().collect(),
                                                    line: child.start_position().row + 1,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn collect_ts_import_names(node: &Node, source: &[u8], names: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                names.push(node_text(&child, source));
            }
            "import_specifier" => {
                // import { Foo as Bar } — we want the local name
                let local = child
                    .child_by_field_name("alias")
                    .or_else(|| child.child_by_field_name("name"));
                if let Some(n) = local {
                    names.push(node_text(&n, source));
                }
            }
            "named_imports" | "namespace_import" => {
                collect_ts_import_names(&child, source, names);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{extract_imports, RawImport};
    use crate::parser::Language;
    use tree_sitter::Parser;

    fn parse_and_extract(source: &str) -> Vec<RawImport> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_imports(Language::TypeScript, &tree, source.as_bytes())
    }

    #[test]
    fn ts_named_import() {
        let src = r#"import { UserService, Config } from "./services";"#;
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "./services");
        assert!(imports[0].imported_names.contains(&"UserService".to_string()));
        assert!(imports[0].imported_names.contains(&"Config".to_string()));
    }

    #[test]
    fn ts_default_import() {
        let src = r#"import express from "express";"#;
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "express");
        assert!(imports[0].imported_names.contains(&"express".to_string()));
    }

    #[test]
    fn ts_namespace_import() {
        let src = r#"import * as path from "path";"#;
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "path");
    }

    #[test]
    fn ts_require_import() {
        let src = r#"const fs = require("fs");"#;
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "fs");
        assert!(imports[0].imported_names.contains(&"fs".to_string()));
    }
}
