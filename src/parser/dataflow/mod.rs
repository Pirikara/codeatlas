mod go;
mod ruby;
mod typescript;

use super::Language;
use tree_sitter::Tree;

/// A raw data flow extracted from source code within a single function.
#[derive(Debug, Clone)]
pub struct RawFlow {
    /// The source expression text (e.g., variable name, parameter)
    pub source_expr: String,
    /// The sink expression text (e.g., function argument, assigned target)
    pub sink_expr: String,
    /// The kind of data flow
    pub flow_kind: FlowKind,
    /// Name of the enclosing function/method
    pub enclosing_function: Option<String>,
    /// Parent class/module of the enclosing function
    pub enclosing_parent: Option<String>,
    /// Start line (1-based) of the enclosing function (used for disambiguation)
    #[allow(dead_code)]
    pub enclosing_start_line: usize,
    /// Line of the source expression (1-based)
    pub source_line: usize,
    /// Line of the sink expression (1-based)
    pub sink_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowKind {
    /// x = expr
    Assignment,
    /// foo(x)
    Argument,
    /// `...${x}...` or "...#{x}..."
    StringInterp,
    /// return x
    Return,
    /// a.b.c
    FieldAccess,
}

impl std::fmt::Display for FlowKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowKind::Assignment => write!(f, "Assignment"),
            FlowKind::Argument => write!(f, "Argument"),
            FlowKind::StringInterp => write!(f, "StringInterp"),
            FlowKind::Return => write!(f, "Return"),
            FlowKind::FieldAccess => write!(f, "FieldAccess"),
        }
    }
}

/// Extract data flows from a parsed tree based on language.
pub fn extract_dataflows(language: Language, tree: &Tree, source: &[u8]) -> Vec<RawFlow> {
    let root = tree.root_node();
    let mut flows = Vec::new();

    match language {
        Language::TypeScript => typescript::extract_ts_dataflows(&root, source, &mut flows),
        Language::Ruby => ruby::extract_ruby_dataflows(&root, source, &mut flows),
        Language::Go => go::extract_go_dataflows(&root, source, &mut flows),
    }

    flows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(lang: Language, source: &str) -> Vec<RawFlow> {
        let bytes = source.as_bytes();
        let tree = match lang {
            Language::TypeScript => {
                let mut p = tree_sitter::Parser::new();
                p.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
                p.parse(bytes, None).unwrap()
            }
            Language::Ruby => {
                let mut p = tree_sitter::Parser::new();
                p.set_language(&tree_sitter_ruby::LANGUAGE.into()).unwrap();
                p.parse(bytes, None).unwrap()
            }
            Language::Go => {
                let mut p = tree_sitter::Parser::new();
                p.set_language(&tree_sitter_go::LANGUAGE.into()).unwrap();
                p.parse(bytes, None).unwrap()
            }
        };
        extract_dataflows(lang, &tree, bytes)
    }

    fn has_flow(flows: &[RawFlow], kind: FlowKind, source: &str, sink: &str) -> bool {
        flows.iter().any(|f| f.flow_kind == kind && f.source_expr == source && f.sink_expr == sink)
    }

    fn has_flow_kind(flows: &[RawFlow], kind: FlowKind) -> bool {
        flows.iter().any(|f| f.flow_kind == kind)
    }

    // ── TypeScript tests ────────────────────────────────────────

    #[test]
    fn ts_assignment() {
        let flows = extract(Language::TypeScript, r#"
function handle(input: string) {
    const data = JSON.parse(input);
}
"#);
        assert!(has_flow(&flows, FlowKind::Assignment, "JSON.parse(input)", "data"));
    }

    #[test]
    fn ts_argument() {
        let flows = extract(Language::TypeScript, r#"
function handle(input: string) {
    console.log(input);
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::Argument));
    }

    #[test]
    fn ts_string_interp() {
        let flows = extract(Language::TypeScript, r#"
function greet(name: string) {
    const msg = `Hello, ${name}!`;
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::StringInterp));
    }

    #[test]
    fn ts_return() {
        let flows = extract(Language::TypeScript, r#"
function get(): number {
    const x = 42;
    return x;
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::Return));
    }

    #[test]
    fn ts_field_access() {
        let flows = extract(Language::TypeScript, r#"
function process(obj: any) {
    const val = obj.data.name;
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::FieldAccess));
    }

    // ── Ruby tests ──────────────────────────────────────────────

    #[test]
    fn ruby_assignment() {
        let flows = extract(Language::Ruby, r#"
def handle(input)
  data = JSON.parse(input)
end
"#);
        assert!(has_flow_kind(&flows, FlowKind::Assignment));
    }

    #[test]
    fn ruby_argument() {
        let flows = extract(Language::Ruby, r#"
def handle(input)
  puts(input)
end
"#);
        assert!(has_flow_kind(&flows, FlowKind::Argument));
    }

    #[test]
    fn ruby_string_interp() {
        let flows = extract(Language::Ruby, r#"
def greet(name)
  msg = "Hello, #{name}!"
end
"#);
        assert!(has_flow_kind(&flows, FlowKind::StringInterp));
    }

    #[test]
    fn ruby_return() {
        let flows = extract(Language::Ruby, r#"
def get
  x = 42
  return x
end
"#);
        assert!(has_flow_kind(&flows, FlowKind::Return));
    }

    #[test]
    fn ruby_field_access() {
        let flows = extract(Language::Ruby, r#"
def process(obj)
  val = obj.data.name
end
"#);
        assert!(has_flow_kind(&flows, FlowKind::FieldAccess));
    }

    // ── Go tests ────────────────────────────────────────────────

    #[test]
    fn go_assignment() {
        let flows = extract(Language::Go, r#"
package main

func handle(input string) {
    data := parse(input)
    _ = data
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::Assignment));
    }

    #[test]
    fn go_argument() {
        let flows = extract(Language::Go, r#"
package main

import "fmt"

func handle(input string) {
    fmt.Println(input)
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::Argument));
    }

    #[test]
    fn go_return() {
        let flows = extract(Language::Go, r#"
package main

func get() int {
    x := 42
    return x
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::Return));
    }

    #[test]
    fn go_field_access() {
        let flows = extract(Language::Go, r#"
package main

type Obj struct {
    Data struct {
        Name string
    }
}

func process(obj Obj) {
    val := obj.Data.Name
    _ = val
}
"#);
        assert!(has_flow_kind(&flows, FlowKind::FieldAccess));
    }
}
