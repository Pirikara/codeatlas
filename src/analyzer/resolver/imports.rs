use std::collections::HashMap;
use std::path::Path;

use crate::parser::imports::RawImport;

use super::super::SymbolIndex;
use super::{Relationship, RelationKind};

/// Get the list of file paths that a file imports from (for import-aware call resolution).
pub fn resolve_imported_files(
    file_path: &str,
    imports: &[RawImport],
    file_exports: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let known_files: Vec<&str> = file_exports.keys().map(|s| s.as_str()).collect();
    let mut imported = Vec::new();
    for import in imports {
        if let Some(target) = resolve_import_path(file_path, &import.source_path, &known_files) {
            imported.push(target);
        }
    }
    imported
}

pub fn resolve_imports(
    file_path: &str,
    imports: &[RawImport],
    file_exports: &HashMap<String, Vec<String>>,
    symbol_index: &SymbolIndex,
    relationships: &mut Vec<Relationship>,
) {
    let file_uid = format!("File:{}", file_path);
    let known_files: Vec<&str> = file_exports.keys().map(|s| s.as_str()).collect();

    for import in imports {
        let target_file =
            resolve_import_path(file_path, &import.source_path, &known_files);

        if let Some(ref target) = target_file {
            // File-to-file IMPORTS edge
            let target_uid = format!("File:{}", target);
            relationships.push(Relationship {
                source_uid: file_uid.clone(),
                target_uid,
                kind: RelationKind::Imports,
                confidence: 1.0,
                reason: "import-resolved".to_string(),
            });

            // Named import → specific symbol edges
            for name in &import.imported_names {
                if let Some(refs) = symbol_index.get(name) {
                    for sym_ref in refs {
                        if sym_ref.file_path == *target && sym_ref.is_exported {
                            relationships.push(Relationship {
                                source_uid: file_uid.clone(),
                                target_uid: sym_ref.uid.clone(),
                                kind: RelationKind::Imports,
                                confidence: 1.0,
                                reason: "named-import-resolved".to_string(),
                            });
                        }
                    }
                }
            }
            continue;
        }

        // Fuzzy match: find symbols matching imported names globally
        for name in &import.imported_names {
            if let Some(refs) = symbol_index.get(name) {
                let exported: Vec<_> = refs.iter().filter(|r| r.is_exported).collect();
                if exported.len() == 1 {
                    relationships.push(Relationship {
                        source_uid: file_uid.clone(),
                        target_uid: exported[0].uid.clone(),
                        kind: RelationKind::Imports,
                        confidence: 0.7,
                        reason: "fuzzy-name-match".to_string(),
                    });
                }
            }
        }
    }
}

/// Resolve an import path relative to the importing file.
fn resolve_import_path(from_file: &str, import_source: &str, known_files: &[&str]) -> Option<String> {
    if !import_source.starts_with('.') {
        return resolve_package_import(import_source, known_files);
    }

    let from_dir = Path::new(from_file).parent()?;
    let source_clean = import_source
        .trim_end_matches(".js")
        .trim_end_matches(".jsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".tsx");
    let resolved = from_dir.join(source_clean);
    let base = normalize_path(&resolved.to_string_lossy());

    let candidates = [
        base.clone(),
        format!("{}.ts", base),
        format!("{}.tsx", base),
        format!("{}.js", base),
        format!("{}.jsx", base),
        format!("{}.rb", base),
        format!("{}.go", base),
        format!("{}/index.ts", base),
        format!("{}/index.tsx", base),
        format!("{}/index.js", base),
    ];

    for candidate in &candidates {
        if known_files.contains(&candidate.as_str()) {
            return Some(candidate.clone());
        }
    }

    for &known in known_files {
        if known.ends_with(&format!("/{}.ts", import_source.trim_start_matches("./")))
            || known.ends_with(&format!("/{}.tsx", import_source.trim_start_matches("./")))
        {
            return Some(known.to_string());
        }
    }

    None
}

fn resolve_package_import(import_source: &str, known_files: &[&str]) -> Option<String> {
    let suffixes = [
        format!("{}.ts", import_source),
        format!("{}.js", import_source),
        format!("{}.rb", import_source),
        format!("{}/index.ts", import_source),
        format!("{}/index.js", import_source),
    ];

    for suffix in &suffixes {
        for &known in known_files {
            if known.ends_with(suffix.as_str()) {
                return Some(known.to_string());
            }
        }
    }

    let parts: Vec<&str> = import_source.split('/').collect();
    for start in 0..parts.len() {
        let suffix = parts[start..].join("/");
        let dir_prefix = format!("{}/", suffix);
        let mut matches: Vec<&str> = known_files
            .iter()
            .filter(|f| f.contains(&dir_prefix) && f.ends_with(".go"))
            .copied()
            .collect();
        if !matches.is_empty() {
            matches.sort();
            return Some(matches[0].to_string());
        }
    }

    for &known in known_files {
        if known.ends_with(&format!("{}.rb", import_source)) {
            return Some(known.to_string());
        }
    }

    None
}

fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut result = Vec::new();
    for part in parts {
        match part {
            "" | "." => continue,
            ".." => {
                result.pop();
            }
            _ => result.push(part),
        }
    }
    result.join("/")
}
