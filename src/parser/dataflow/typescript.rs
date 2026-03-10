use tree_sitter::Node;

use super::super::calls::{find_enclosing_function_with_line, node_text};
use super::{FlowKind, RawFlow};

pub fn extract_ts_dataflows(node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    extract_node(node, source, flows);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_ts_dataflows(&child, source, flows);
    }
}

fn extract_node(node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    match node.kind() {
        // Assignment: variable_declarator with value (const x = expr)
        "variable_declarator" => {
            if let (Some(name_node), Some(value_node)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("value"),
            ) {
                let sink = node_text(&name_node, source);
                let source_expr = node_text(&value_node, source);
                let (func, parent, start_line) = find_enclosing_function_with_line(node, source);
                flows.push(RawFlow {
                    source_expr,
                    sink_expr: sink,
                    flow_kind: FlowKind::Assignment,
                    enclosing_function: func,
                    enclosing_parent: parent,
                    enclosing_start_line: start_line,
                    source_line: value_node.start_position().row + 1,
                    sink_line: name_node.start_position().row + 1,
                });

                // Check for field access in the value
                extract_field_access_chain(&value_node, node, source, flows);
            }
        }
        // Assignment expression: x = expr
        "assignment_expression" => {
            if let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                let sink = node_text(&left, source);
                let source_expr = node_text(&right, source);
                let (func, parent, start_line) = find_enclosing_function_with_line(node, source);
                flows.push(RawFlow {
                    source_expr,
                    sink_expr: sink,
                    flow_kind: FlowKind::Assignment,
                    enclosing_function: func,
                    enclosing_parent: parent,
                    enclosing_start_line: start_line,
                    source_line: right.start_position().row + 1,
                    sink_line: left.start_position().row + 1,
                });
            }
        }
        // Return statement
        "return_statement" => {
            // The return value is the first non-keyword child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "return" && !child.is_extra() {
                    let source_expr = node_text(&child, source);
                    let (func, parent, start_line) = find_enclosing_function_with_line(node, source);
                    flows.push(RawFlow {
                        source_expr,
                        sink_expr: "return".to_string(),
                        flow_kind: FlowKind::Return,
                        enclosing_function: func,
                        enclosing_parent: parent,
                        enclosing_start_line: start_line,
                        source_line: child.start_position().row + 1,
                        sink_line: node.start_position().row + 1,
                    });
                    break;
                }
            }
        }
        // Call expression → arguments are Argument flows
        "call_expression" => {
            if let Some(args_node) = node.child_by_field_name("arguments") {
                let callee = node.child_by_field_name("function")
                    .map(|n| node_text(&n, source))
                    .unwrap_or_default();
                let mut cursor = args_node.walk();
                for (idx, arg) in args_node.children(&mut cursor).enumerate() {
                    if arg.kind() == "(" || arg.kind() == ")" || arg.kind() == "," {
                        continue;
                    }
                    let source_expr = node_text(&arg, source);
                    let sink = format!("{}[arg{}]", callee, idx);
                    let (func, parent, start_line) = find_enclosing_function_with_line(node, source);
                    flows.push(RawFlow {
                        source_expr,
                        sink_expr: sink,
                        flow_kind: FlowKind::Argument,
                        enclosing_function: func,
                        enclosing_parent: parent,
                        enclosing_start_line: start_line,
                        source_line: arg.start_position().row + 1,
                        sink_line: node.start_position().row + 1,
                    });
                }
            }
        }
        // Template string with substitutions
        "template_string" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "template_substitution" {
                    // The substitution content is inside the ${}
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() != "${" && inner.kind() != "}" {
                            let source_expr = node_text(&inner, source);
                            let (func, parent, start_line) = find_enclosing_function_with_line(node, source);
                            flows.push(RawFlow {
                                source_expr,
                                sink_expr: "string_interpolation".to_string(),
                                flow_kind: FlowKind::StringInterp,
                                enclosing_function: func,
                                enclosing_parent: parent,
                                enclosing_start_line: start_line,
                                source_line: inner.start_position().row + 1,
                                sink_line: node.start_position().row + 1,
                            });
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn extract_field_access_chain(node: &Node, context_node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    if node.kind() == "member_expression" {
        let text = node_text(node, source);
        if text.contains('.') {
            let parts: Vec<&str> = text.splitn(2, '.').collect();
            if parts.len() == 2 {
                let (func, parent, start_line) = find_enclosing_function_with_line(context_node, source);
                flows.push(RawFlow {
                    source_expr: parts[0].to_string(),
                    sink_expr: text.clone(),
                    flow_kind: FlowKind::FieldAccess,
                    enclosing_function: func,
                    enclosing_parent: parent,
                    enclosing_start_line: start_line,
                    source_line: node.start_position().row + 1,
                    sink_line: node.start_position().row + 1,
                });
            }
        }
    }
}
