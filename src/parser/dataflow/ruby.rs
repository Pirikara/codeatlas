use tree_sitter::Node;

use super::super::calls::{find_enclosing_function_with_line, node_text};
use super::{FlowKind, RawFlow};

pub fn extract_ruby_dataflows(node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    extract_node(node, source, flows);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_ruby_dataflows(&child, source, flows);
    }
}

fn extract_node(node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    match node.kind() {
        // Assignment: x = expr
        "assignment" => {
            if let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                let sink = node_text(&left, source);
                let source_expr = node_text(&right, source);
                let (func, parent, start_line) = find_enclosing_function_with_line(node, source);
                flows.push(RawFlow {
                    source_expr: source_expr.clone(),
                    sink_expr: sink,
                    flow_kind: FlowKind::Assignment,
                    enclosing_function: func,
                    enclosing_parent: parent,
                    enclosing_start_line: start_line,
                    source_line: right.start_position().row + 1,
                    sink_line: left.start_position().row + 1,
                });

                // Check for method chain (field access) on the right side
                extract_field_access_chain(&right, node, source, flows);
            }
        }
        // Return statement
        "return" => {
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
        // Method call → arguments
        "call" => {
            let method_name = node.child_by_field_name("method")
                .map(|n| node_text(&n, source))
                .unwrap_or_default();
            let receiver = node.child_by_field_name("receiver")
                .map(|n| node_text(&n, source));
            let callee = match receiver {
                Some(ref r) => format!("{}.{}", r, method_name),
                None => method_name,
            };

            if let Some(args_node) = node.child_by_field_name("arguments") {
                let mut idx = 0;
                let mut cursor = args_node.walk();
                for arg in args_node.children(&mut cursor) {
                    if arg.kind() == "(" || arg.kind() == ")" || arg.kind() == ","
                        || arg.kind() == "argument_list" {
                        // For argument_list, recurse into it
                        if arg.kind() == "argument_list" {
                            let mut inner_cursor = arg.walk();
                            for inner_arg in arg.children(&mut inner_cursor) {
                                if inner_arg.kind() != "(" && inner_arg.kind() != ")"
                                    && inner_arg.kind() != "," {
                                    let source_expr = node_text(&inner_arg, source);
                                    let sink = format!("{}[arg{}]", callee, idx);
                                    let (func, parent, start_line) = find_enclosing_function_with_line(node, source);
                                    flows.push(RawFlow {
                                        source_expr,
                                        sink_expr: sink,
                                        flow_kind: FlowKind::Argument,
                                        enclosing_function: func,
                                        enclosing_parent: parent,
                                        enclosing_start_line: start_line,
                                        source_line: inner_arg.start_position().row + 1,
                                        sink_line: node.start_position().row + 1,
                                    });
                                    idx += 1;
                                }
                            }
                        }
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
                    idx += 1;
                }
            }
        }
        // String interpolation: "...#{expr}..."
        "string" | "string_content" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "interpolation" {
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() != "#{" && inner.kind() != "}"
                            && inner.kind() != "string_begin" && inner.kind() != "string_end" {
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
    if node.kind() == "call" {
        if let Some(receiver) = node.child_by_field_name("receiver") {
            let text = node_text(node, source);
            if text.contains('.') {
                let recv_text = node_text(&receiver, source);
                let (func, parent, start_line) = find_enclosing_function_with_line(context_node, source);
                flows.push(RawFlow {
                    source_expr: recv_text,
                    sink_expr: text,
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
