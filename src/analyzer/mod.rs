pub mod community;
pub mod process;
pub mod resolver;

use std::collections::HashMap;

use crate::parser::calls::RawCall;
use crate::parser::imports::RawImport;
use crate::parser::Symbol;

pub use community::Community;
pub use process::Process;
pub use resolver::Relationship;

/// All extracted data from a single file, ready for analysis.
#[derive(Debug)]
pub struct FileAnalysis {
    pub file_path: String,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<RawImport>,
    pub calls: Vec<RawCall>,
}

/// Resolve all cross-file relationships from file analyses.
pub fn resolve_relationships(analyses: &[FileAnalysis]) -> Vec<Relationship> {
    // Build lookup indexes
    let symbol_index = build_symbol_index(analyses);
    let file_exports = build_file_exports(analyses);

    let mut relationships = Vec::new();

    // First pass: resolve imports to build imported-files map
    let mut file_imported_files: HashMap<String, Vec<String>> = HashMap::new();
    for analysis in analyses {
        let imported = resolver::resolve_imported_files(
            &analysis.file_path,
            &analysis.imports,
            &file_exports,
        );
        file_imported_files.insert(analysis.file_path.clone(), imported);
    }

    for analysis in analyses {
        // Resolve imports → IMPORTS edges
        resolver::resolve_imports(
            &analysis.file_path,
            &analysis.imports,
            &file_exports,
            &symbol_index,
            &mut relationships,
        );

        // Resolve calls → CALLS edges (with import-aware priority)
        let imported_files = file_imported_files
            .get(&analysis.file_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        resolver::resolve_calls(
            &analysis.file_path,
            &analysis.calls,
            &analysis.symbols,
            &symbol_index,
            imported_files,
            &mut relationships,
        );

        // Detect inheritance → EXTENDS edges
        resolver::resolve_inheritance(
            &analysis.file_path,
            &analysis.symbols,
            &symbol_index,
            &mut relationships,
        );

        // File → Symbol DEFINES edges
        resolver::resolve_defines(
            &analysis.file_path,
            &analysis.symbols,
            &mut relationships,
        );
    }

    // Go implicit interface implementation (cross-file structural type matching)
    let all_symbols: Vec<&Symbol> = analyses.iter().flat_map(|a| &a.symbols).collect();
    let all_symbols_owned: Vec<Symbol> = all_symbols.into_iter().cloned().collect();
    resolver::resolve_go_implicit_implements(&all_symbols_owned, &mut relationships);

    // CONTAINS edges: Folder → File, Folder → Folder
    resolve_contains(analyses, &mut relationships);

    relationships
}

/// Build directory hierarchy CONTAINS edges.
fn resolve_contains(
    analyses: &[FileAnalysis],
    relationships: &mut Vec<resolver::Relationship>,
) {
    use std::collections::HashSet;
    use std::path::Path;

    let mut dirs_seen: HashSet<String> = HashSet::new();
    let mut file_paths: Vec<&str> = analyses.iter().map(|a| a.file_path.as_str()).collect();
    file_paths.sort();

    for &fp in &file_paths {
        let path = Path::new(fp);
        // Folder → File
        if let Some(parent) = path.parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if !parent_str.is_empty() {
                relationships.push(resolver::Relationship {
                    source_uid: format!("Folder:{}", parent_str),
                    target_uid: format!("File:{}", fp),
                    kind: resolver::RelationKind::Contains,
                    confidence: 1.0,
                    reason: "directory-contains-file".to_string(),
                });

                // Build parent chain: Folder → Folder
                let mut current = parent;
                while let Some(grandparent) = current.parent() {
                    let gp_str = grandparent.to_string_lossy().to_string();
                    let cur_str = current.to_string_lossy().to_string();
                    if gp_str.is_empty() || !dirs_seen.insert(cur_str.clone()) {
                        break;
                    }
                    relationships.push(resolver::Relationship {
                        source_uid: format!("Folder:{}", gp_str),
                        target_uid: format!("Folder:{}", cur_str),
                        kind: resolver::RelationKind::Contains,
                        confidence: 1.0,
                        reason: "directory-contains-directory".to_string(),
                    });
                    current = grandparent;
                }
            }
        }
    }
}

/// Index: symbol name → list of (uid, file_path, kind, parent_name)
pub type SymbolIndex = HashMap<String, Vec<SymbolRef>>;

#[derive(Debug, Clone)]
pub struct SymbolRef {
    pub uid: String,
    pub file_path: String,
    pub kind: String,
    #[allow(dead_code)]
    pub parent_name: Option<String>,
    pub is_exported: bool,
    /// TypeScript: type annotation for Property symbols (used for this.field resolution)
    pub type_annotation: Option<String>,
}

/// Index: file_path → list of exported symbol names
type FileExports = HashMap<String, Vec<String>>;

fn build_symbol_index(analyses: &[FileAnalysis]) -> SymbolIndex {
    let mut index: SymbolIndex = HashMap::new();
    for analysis in analyses {
        for sym in &analysis.symbols {
            let sym_ref = SymbolRef {
                uid: sym.uid(),
                file_path: sym.file_path.clone(),
                kind: sym.kind.to_string(),
                parent_name: sym.parent_name.clone(),
                is_exported: sym.is_exported,
                type_annotation: sym.type_annotation.clone(),
            };
            index.entry(sym.name.clone()).or_default().push(sym_ref);

            // Also index as "Parent.method" for method resolution
            if let Some(ref parent) = sym.parent_name {
                let qualified = format!("{}.{}", parent, sym.name);
                let sym_ref2 = SymbolRef {
                    uid: sym.uid(),
                    file_path: sym.file_path.clone(),
                    kind: sym.kind.to_string(),
                    parent_name: sym.parent_name.clone(),
                    is_exported: sym.is_exported,
                    type_annotation: sym.type_annotation.clone(),
                };
                index.entry(qualified).or_default().push(sym_ref2);
            }
        }
    }
    index
}

fn build_file_exports(analyses: &[FileAnalysis]) -> FileExports {
    let mut exports: FileExports = HashMap::new();
    for analysis in analyses {
        let mut names = Vec::new();
        for sym in &analysis.symbols {
            if sym.is_exported {
                names.push(sym.name.clone());
            }
        }
        exports.insert(analysis.file_path.clone(), names);
    }
    exports
}
