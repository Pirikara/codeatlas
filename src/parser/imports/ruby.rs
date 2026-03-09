use super::{node_text, RawImport};
use tree_sitter::Node;

pub(super) fn extract_ruby_imports(node: &Node, source: &[u8], imports: &mut Vec<RawImport>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // require "foo" / require_relative "foo"
        if child.kind() == "call" {
            let method = child
                .child_by_field_name("method")
                .map(|n| node_text(&n, source));
            if matches!(method.as_deref(), Some("require") | Some("require_relative")) {
                let arguments = child.child_by_field_name("arguments");
                if let Some(args) = arguments {
                    let mut ac = args.walk();
                    for arg in args.children(&mut ac) {
                        if arg.kind() == "string" || arg.kind() == "string_content" {
                            let text = string_content(&arg, source);
                            if !text.is_empty() {
                                imports.push(RawImport {
                                    source_path: text,
                                    imported_names: vec![],
                                    line: child.start_position().row + 1,
                                });
                            }
                        }
                    }
                }
            }
        }
        if child.child_count() > 0 {
            extract_ruby_imports(&child, source, imports);
        }
    }
}

fn string_content(node: &Node, source: &[u8]) -> String {
    let text = node_text(node, source);
    text.trim_matches(|c| c == '\'' || c == '"').to_string()
}

#[cfg(test)]
mod tests {
    use super::super::{extract_imports, RawImport};
    use crate::parser::Language;
    use tree_sitter::Parser;

    fn parse_and_extract(source: &str) -> Vec<RawImport> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_imports(Language::Ruby, &tree, source.as_bytes())
    }

    #[test]
    fn ruby_require() {
        let src = "require \"json\"";
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "json");
    }

    #[test]
    fn ruby_require_relative() {
        let src = "require_relative \"../models/user\"";
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source_path, "../models/user");
    }

    #[test]
    fn ruby_multiple_requires() {
        let src = "require \"json\"\nrequire \"net/http\"\nrequire_relative \"helper\"";
        let imports = parse_and_extract(src);
        assert_eq!(imports.len(), 3);
    }
}
