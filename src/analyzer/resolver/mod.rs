mod imports;
mod calls;
mod inheritance;

pub use imports::{resolve_imports, resolve_imported_files};
pub use calls::resolve_calls;
pub use inheritance::{resolve_inheritance, resolve_go_implicit_implements};

use crate::parser::Symbol;

#[derive(Debug, Clone)]
pub struct Relationship {
    pub source_uid: String,
    pub target_uid: String,
    pub kind: RelationKind,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Imports,
    Calls,
    CallsUnresolved,
    CallsExternal,
    Extends,
    Implements,
    Defines,
    Contains,
}

impl std::fmt::Display for RelationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationKind::Imports => write!(f, "IMPORTS"),
            RelationKind::Calls => write!(f, "CALLS"),
            RelationKind::CallsUnresolved => write!(f, "CALLS_UNRESOLVED"),
            RelationKind::CallsExternal => write!(f, "CALLS_EXTERNAL"),
            RelationKind::Extends => write!(f, "EXTENDS"),
            RelationKind::Implements => write!(f, "IMPLEMENTS"),
            RelationKind::Defines => write!(f, "DEFINES"),
            RelationKind::Contains => write!(f, "CONTAINS"),
        }
    }
}

/// Generate DEFINES edges: File → each symbol defined in that file.
pub fn resolve_defines(
    file_path: &str,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let file_uid = format!("File:{}", file_path);
    for sym in symbols {
        if sym.file_path == file_path {
            relationships.push(Relationship {
                source_uid: file_uid.clone(),
                target_uid: sym.uid(),
                kind: RelationKind::Defines,
                confidence: 1.0,
                reason: "file-defines-symbol".to_string(),
            });
        }
    }
}
