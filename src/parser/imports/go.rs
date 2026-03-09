use super::{node_text, RawImport};
use tree_sitter::Node;

pub(super) fn extract_go_imports(node: &Node, source: &[u8], imports: &mut Vec<RawImport>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            let mut ic = child.walk();
            for spec in child.children(&mut ic) {
                match spec.kind() {
                    "import_spec" => {
                        if let Some(path) = spec.child_by_field_name("path") {
                            let text = node_text(&path, source);
                            let text = text.trim_matches('"').to_string();
                            let alias = spec
                                .child_by_field_name("name")
                                .map(|n| node_text(&n, source));
                            imports.push(RawImport {
                                source_path: text,
                                imported_names: alias.into_iter().collect(),
                                line: spec.start_position().row + 1,
                            });
                        }
                    }
                    "import_spec_list" => {
                        let mut lc = spec.walk();
                        for item in spec.children(&mut lc) {
                            if item.kind() == "import_spec" {
                                if let Some(path) = item.child_by_field_name("path") {
                                    let text = node_text(&path, source);
                                    let text = text.trim_matches('"').to_string();
                                    let alias = item
                                        .child_by_field_name("name")
                                        .map(|n| node_text(&n, source));
                                    imports.push(RawImport {
                                        source_path: text,
                                        imported_names: alias.into_iter().collect(),
                                        line: item.start_position().row + 1,
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
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
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_imports(Language::Go, &tree, source.as_bytes())
    }

    #[test]
    fn go_single_import() {
        let src = "package main\nimport \"fmt\"";
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "fmt");
    }

    #[test]
    fn go_grouped_import() {
        let src = "package main\nimport (\n\t\"fmt\"\n\t\"net/http\"\n)";
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 2);
        let paths: Vec<&str> = imports.iter().map(|i| i.source_path.as_str()).collect();
        assert!(paths.contains(&"fmt"));
        assert!(paths.contains(&"net/http"));
    }

    #[test]
    fn go_aliased_import() {
        let src = "package main\nimport (\n\tpb \"google/protobuf\"\n)";
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "google/protobuf");
        assert!(imports[0].imported_names.contains(&"pb".to_string()));
    }
}
