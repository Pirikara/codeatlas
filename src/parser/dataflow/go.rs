use tree_sitter::Node;

use super::super::calls::{find_enclosing_function_with_line, node_text};
use super::{FlowKind, RawFlow};

pub fn extract_go_dataflows(node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    extract_node(node, source, flows);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_go_dataflows(&child, source, flows);
    }
}

fn extract_node(node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    match node.kind() {
        // Short variable declaration: x := expr
        "short_var_declaration" => {
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

                // Check for selector expression (field access) in the value
                extract_field_access_chain(&right, node, source, flows);
            }
        }
        // Assignment statement: x = expr
        "assignment_statement" => {
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
            // Return values are in expression_list child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "expression_list" {
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
                } else if child.kind() != "return" && !child.is_extra()
                    && child.kind() != "expression_list"
                {
                    // Single return value
                    let kind = child.kind();
                    if kind == "identifier" || kind == "call_expression"
                        || kind == "selector_expression" || kind == "int_literal"
                        || kind == "true" || kind == "false" || kind == "nil"
                    {
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
                    }
                }
            }
        }
        // Call expression → arguments
        "call_expression" => {
            let callee = node.child_by_field_name("function")
                .map(|n| node_text(&n, source))
                .unwrap_or_default();
            if let Some(args_node) = node.child_by_field_name("arguments") {
                let mut idx = 0;
                let mut cursor = args_node.walk();
                for arg in args_node.children(&mut cursor) {
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
                    idx += 1;
                }
            }
        }
        _ => {}
    }
}

fn extract_field_access_chain(node: &Node, context_node: &Node, source: &[u8], flows: &mut Vec<RawFlow>) {
    if node.kind() == "selector_expression" {
        let text = node_text(node, source);
        if text.contains('.') {
            if let Some(operand) = node.child_by_field_name("operand") {
                let recv_text = node_text(&operand, source);
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
    // Recurse to find nested selector expressions
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "selector_expression" {
            extract_field_access_chain(&child, context_node, source, flows);
        }
    }
}
