use super::{find_enclosing_function, node_text, RawCall};
use tree_sitter::Node;

pub(super) fn extract_ruby_calls(node: &Node, source: &[u8], calls: &mut Vec<RawCall>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Detect class references in constant array assignments (e.g., STEPS = [Services::CreateOrder, ...])
        if child.kind() == "assignment" {
            extract_ruby_array_refs(&child, source, calls);
        }
        if child.kind() == "call" {
            let method = child
                .child_by_field_name("method")
                .map(|n| node_text(&n, source));
            let receiver = child
                .child_by_field_name("receiver")
                .map(|n| node_text(&n, source))
                .filter(|r| !r.contains('\n') && r.len() <= 80);

            if let Some(name) = method {
                // Skip require/require_relative (already handled as imports)
                if name == "require" || name == "require_relative" {
                    // skip
                } else if name == "send" || name == "public_send" {
                    // Dynamic dispatch: extract the actual method name from the first argument
                    if let Some(args) = child.child_by_field_name("arguments") {
                        if let Some(first_arg) = first_named_child(&args) {
                            let actual_method = match first_arg.kind() {
                                "simple_symbol" => {
                                    let text = node_text(&first_arg, source);
                                    Some(text.trim_start_matches(':').to_string())
                                }
                                "string" => extract_string_content(&first_arg, source),
                                _ => None, // variable/expression → skip
                            };
                            if let Some(method_name) = actual_method {
                                let (caller_name, caller_parent) = find_enclosing_function(&child, source);
                                calls.push(RawCall {
                                    callee_name: method_name,
                                    receiver,
                                    caller_name,
                                    caller_parent,
                                    line: child.start_position().row + 1,
                                });
                            }
                        }
                    }
                    // Never emit a "send" call itself
                } else {
                    let (caller_name, caller_parent) = find_enclosing_function(&child, source);
                    calls.push(RawCall {
                        callee_name: name,
                        receiver,
                        caller_name,
                        caller_parent,
                        line: child.start_position().row + 1,
                    });
                }
            }
        }
        if child.child_count() > 0 {
            extract_ruby_calls(&child, source, calls);
        }
    }
}

/// Get the first named child of a node (without requiring a cursor borrow).
fn first_named_child<'a>(node: &'a tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if child.is_named() {
            return Some(child);
        }
    }
    None
}

/// Extract the text content from a Ruby string node (strips delimiters).
fn extract_string_content(str_node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    for i in 0..str_node.child_count() {
        let child = str_node.child(i).unwrap();
        if child.kind() == "string_content" {
            return Some(node_text(&child, source));
        }
    }
    None
}

/// Extract class references from Ruby constant array assignments (e.g., STEPS = [...])
/// These represent dynamic invocations (like interactor organizer patterns).
fn extract_ruby_array_refs(assignment: &Node, source: &[u8], calls: &mut Vec<RawCall>) {
    // Check: left side is a constant, right side is an array
    let left = assignment.child_by_field_name("left");
    let right = assignment.child_by_field_name("right");
    let is_const_left = left.as_ref().map_or(false, |n| n.kind() == "constant");
    let is_array_right = right.as_ref().map_or(false, |n| n.kind() == "array");

    if !is_const_left || !is_array_right {
        return;
    }

    let (caller_name, caller_parent) = find_enclosing_function(assignment, source);
    let array = right.unwrap();
    let mut cursor = array.walk();
    for elem in array.children(&mut cursor) {
        if matches!(elem.kind(), "scope_resolution" | "constant") {
            let text = node_text(&elem, source);
            if !text.is_empty() && text.chars().next().map_or(false, |c| c.is_uppercase()) {
                // Extract the short name (last segment) as callee, with the full name as receiver
                let short_name = text.rsplit("::").next().unwrap_or(&text);
                calls.push(RawCall {
                    callee_name: short_name.to_string(),
                    receiver: if text.contains("::") {
                        // Use the namespace as receiver for resolution
                        text.rsplit_once("::").map(|(ns, _)| ns.to_string())
                    } else {
                        None
                    },
                    caller_name: caller_name.clone(),
                    caller_parent: caller_parent.clone(),
                    line: elem.start_position().row + 1,
                });
            }
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
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source.as_bytes(), None).unwrap();
        extract_calls(Language::Ruby, &tree, source.as_bytes())
    }

    #[test]
    fn ruby_method_call() {
        let src = "class App\n  def run\n    @service.create(\"user\")\n  end\nend";
        let calls = parse_and_extract(src);
        assert!(calls.iter().any(|c| c.callee_name == "create"));
    }

    #[test]
    fn ruby_require_skipped() {
        let src = "require \"json\"\nrequire_relative \"helper\"";
        let calls = parse_and_extract(src);
        assert!(calls
            .iter()
            .all(|c| c.callee_name != "require" && c.callee_name != "require_relative"));
    }

    #[test]
    fn ruby_caller_context() {
        let src = "class Order\n  def process\n    validate\n    save\n  end\nend";
        let calls = parse_and_extract(src);
        for call in &calls {
            assert_eq!(call.caller_name.as_deref(), Some("process"));
        }
    }
}
