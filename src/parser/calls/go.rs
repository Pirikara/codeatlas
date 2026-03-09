use super::{find_enclosing_function, node_text, RawCall};
use tree_sitter::Node;

pub(super) fn extract_go_calls(node: &Node, source: &[u8], calls: &mut Vec<RawCall>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            let function = child.child_by_field_name("function");
            if let Some(func) = function {
                let (callee_name, receiver) = match func.kind() {
                    "selector_expression" => {
                        let field = func
                            .child_by_field_name("field")
                            .map(|n| node_text(&n, source));
                        let operand = func
                            .child_by_field_name("operand")
                            .map(|n| node_text(&n, source));
                        (field.unwrap_or_default(), operand)
                    }
                    "identifier" => (node_text(&func, source), None),
                    _ => (node_text(&func, source), None),
                };

                if !callee_name.is_empty() {
                    let (caller_name, caller_parent) = find_enclosing_function(&child, source);
                    calls.push(RawCall {
                        callee_name,
                        receiver,
                        caller_name,
                        caller_parent,
                        line: child.start_position().row + 1,
                    });
                }
            }
        }
        if child.child_count() > 0 {
            extract_go_calls(&child, source, calls);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{extract_calls, RawCall};
    use crate::parser::Language;
    use tree_sitter::Parser;

    fn parse_and_extract(source: &str) -> Vec<RawCall> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_calls(Language::Go, &tree, source.as_bytes())
    }

    #[test]
    fn go_function_call() {
        let src = "package main\nfunc main() { fmt.Println(\"hello\") }";
        let calls = parse_and_extract(src);
        assert!(calls
            .iter()
            .any(|c| c.callee_name == "Println" && c.receiver.as_deref() == Some("fmt")));
    }

    #[test]
    fn go_method_call() {
        let src = "package main\nfunc (h *Handler) Serve() { h.service.Run(\"task\") }";
        let calls = parse_and_extract(src);
        let run_call = calls.iter().find(|c| c.callee_name == "Run").unwrap();
        assert!(run_call.receiver.is_some());
        assert_eq!(run_call.caller_name.as_deref(), Some("Serve"));
    }

    #[test]
    fn go_plain_call() {
        let src = "package main\nfunc main() { Execute(args) }";
        let calls = parse_and_extract(src);
        assert!(calls
            .iter()
            .any(|c| c.callee_name == "Execute" && c.receiver.is_none()));
    }
}
