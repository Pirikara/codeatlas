use super::{find_enclosing_function, node_text, RawCall};
use tree_sitter::Node;

pub(super) fn extract_ts_calls(node: &Node, source: &[u8], calls: &mut Vec<RawCall>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            let function = child.child_by_field_name("function");
            if let Some(func) = function {
                let (callee_name, receiver) = match func.kind() {
                    "member_expression" => {
                        let property = func
                            .child_by_field_name("property")
                            .map(|n| node_text(&n, source));
                        let object = func.child_by_field_name("object");
                        // For chained calls like this.repository.findById(),
                        // extract "repository" as receiver instead of "this.repository"
                        let receiver = object.map(|obj| {
                            if obj.kind() == "member_expression" {
                                let inner_obj = obj
                                    .child_by_field_name("object")
                                    .map(|n| node_text(&n, source));
                                if matches!(inner_obj.as_deref(), Some("this") | Some("self")) {
                                    return obj
                                        .child_by_field_name("property")
                                        .map(|n| node_text(&n, source))
                                        .unwrap_or_else(|| node_text(&obj, source));
                                }
                            }
                            node_text(&obj, source)
                        });
                        (property.unwrap_or_default(), receiver)
                    }
                    "identifier" => (node_text(&func, source), None),
                    _ => (node_text(&func, source), None),
                };

                if !callee_name.is_empty() {
                    // Skip require() — handled as import
                    if callee_name == "require" && receiver.is_none() {
                        // skip
                    } else {
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
        }
        // Also handle: new Foo() — as a call to Foo's constructor
        if child.kind() == "new_expression" {
            let constructor = child.child_by_field_name("constructor");
            if let Some(ctor) = constructor {
                let name = node_text(&ctor, source);
                if !name.is_empty() {
                    let (caller_name, caller_parent) = find_enclosing_function(&child, source);
                    calls.push(RawCall {
                        callee_name: name,
                        receiver: None,
                        caller_name,
                        caller_parent,
                        line: child.start_position().row + 1,
                    });
                }
            }
        }
        if child.child_count() > 0 {
            extract_ts_calls(&child, source, calls);
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
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_calls(Language::TypeScript, &tree, source.as_bytes())
    }

    #[test]
    fn ts_simple_call() {
        let src = "function main() { greet('hello'); }";
        let calls = parse_and_extract(src);
        assert!(calls
            .iter()
            .any(|c| c.callee_name == "greet" && c.receiver.is_none()));
    }

    #[test]
    fn ts_method_call() {
        let src = "function main() { this.service.findById('1'); }";
        let calls = parse_and_extract(src);
        assert!(calls
            .iter()
            .any(|c| c.callee_name == "findById" && c.receiver.is_some()));
    }

    #[test]
    fn ts_new_expression() {
        let src = "function main() { const svc = new UserService(); }";
        let calls = parse_and_extract(src);
        assert!(calls.iter().any(|c| c.callee_name == "UserService"));
    }

    #[test]
    fn ts_require_skipped() {
        let src = r#"const fs = require("fs");"#;
        let calls = parse_and_extract(src);
        assert!(calls.iter().all(|c| c.callee_name != "require"));
    }

    #[test]
    fn ts_caller_context() {
        let src = "class UserController {\n  create() { this.service.save(); }\n}";
        let calls = parse_and_extract(src);
        let save_call = calls.iter().find(|c| c.callee_name == "save").unwrap();
        assert_eq!(save_call.caller_name.as_deref(), Some("create"));
        assert_eq!(save_call.caller_parent.as_deref(), Some("UserController"));
    }
}
