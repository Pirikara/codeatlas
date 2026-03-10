use crate::parser::calls::RawCall;
use crate::parser::Symbol;

use super::super::SymbolIndex;
use super::{Relationship, RelationKind};

pub fn resolve_calls(
    file_path: &str,
    calls: &[RawCall],
    file_symbols: &[Symbol],
    symbol_index: &SymbolIndex,
    imported_files: &[String],
    relationships: &mut Vec<Relationship>,
) {
    for call in calls {
        let caller_uid = resolve_caller(file_path, call, file_symbols);
        let Some(caller_uid) = caller_uid else {
            continue;
        };

        let resolved = resolve_callee(file_path, call, file_symbols, symbol_index, imported_files);
        for target in resolved {
            relationships.push(Relationship {
                source_uid: caller_uid.clone(),
                target_uid: target.target_uid,
                kind: target.kind,
                confidence: target.confidence,
                reason: target.reason,
            });
        }
    }
}

/// A resolved callee target with its relationship kind.
pub struct ResolvedTarget {
    pub target_uid: String,
    pub confidence: f64,
    pub reason: String,
    pub kind: RelationKind,
}

/// Find the UID of the calling function.
fn resolve_caller(file_path: &str, call: &RawCall, file_symbols: &[Symbol]) -> Option<String> {
    if let Some(ref name) = call.caller_name {
        for sym in file_symbols {
            if sym.name == *name {
                if call.caller_parent.is_none() || sym.parent_name == call.caller_parent {
                    return Some(sym.uid());
                }
            }
        }
        for sym in file_symbols {
            if sym.name == *name && sym.file_path == file_path {
                return Some(sym.uid());
            }
        }
    }

    // Class-level calls (e.g., Ruby DSL: validates, has_many) — use parent class/module as caller
    if let Some(ref parent_name) = call.caller_parent {
        for sym in file_symbols {
            if sym.name == *parent_name
                && sym.file_path == file_path
                && matches!(
                    sym.kind,
                    crate::parser::SymbolKind::Class
                        | crate::parser::SymbolKind::Module
                        | crate::parser::SymbolKind::Struct
                )
            {
                return Some(sym.uid());
            }
        }
    }

    None
}

/// Resolve the callee — returns list of ResolvedTarget.
fn resolve_callee(
    file_path: &str,
    call: &RawCall,
    file_symbols: &[Symbol],
    symbol_index: &SymbolIndex,
    imported_files: &[String],
) -> Vec<ResolvedTarget> {
    let mut results = Vec::new();

    // Helper to create a resolved Calls target
    macro_rules! resolved {
        ($uid:expr, $conf:expr, $reason:expr) => {
            ResolvedTarget {
                target_uid: $uid,
                confidence: $conf,
                reason: $reason,
                kind: RelationKind::Calls,
            }
        };
    }

    // Strategy 1.0: TypeScript this.field.method() resolution via type annotations
    // When receiver is a lowercase field name and we're inside a class method,
    // look up the property type to find the actual method.
    if let Some(ref receiver) = call.receiver {
        let recv_is_field = receiver.chars().next().map_or(false, |c| c.is_lowercase())
            && receiver != "this" && receiver != "self";
        if recv_is_field {
            if let Some(ref caller_class) = call.caller_parent {
                let prop_key = format!("{}.{}", caller_class, receiver);
                if let Some(prop_refs) = symbol_index.get(&prop_key) {
                    // Prefer same-file property to avoid cross-file class name collisions.
                    let same_file: Vec<_> = prop_refs.iter()
                        .filter(|r| r.file_path == file_path && r.kind == "Property")
                        .collect();
                    let candidates = if same_file.is_empty() { prop_refs.iter().collect::<Vec<_>>() } else { same_file };
                    for prop_ref in candidates {
                        if prop_ref.kind == "Property" {
                            if let Some(ref type_name) = prop_ref.type_annotation {
                                let method_key = format!("{}.{}", type_name, call.callee_name);
                                if let Some(method_refs) = symbol_index.get(&method_key) {
                                    for sym_ref in method_refs {
                                        let confidence = if sym_ref.file_path == file_path {
                                            0.95
                                        } else {
                                            0.90
                                        };
                                        results.push(resolved!(
                                            sym_ref.uid.clone(),
                                            confidence,
                                            "field-type-annotation".to_string()
                                        ));
                                    }
                                    if !results.is_empty() {
                                        return results;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy 1: Qualified receiver match
    if let Some(ref receiver) = call.receiver {
        let effective_receiver = if receiver == "this" || receiver == "self" || receiver == "@" {
            call.caller_parent.as_deref()
        } else {
            Some(receiver.as_str())
        };

        if let Some(recv) = effective_receiver {
            // Strategy 1a: Direct qualified lookup
            let qualified = format!("{}.{}", recv, call.callee_name);
            if let Some(refs) = symbol_index.get(&qualified) {
                for sym_ref in refs {
                    let confidence = if sym_ref.file_path == file_path {
                        0.95
                    } else {
                        0.90
                    };
                    results.push(resolved!(
                        sym_ref.uid.clone(),
                        confidence,
                        "type-qualified-match".to_string()
                    ));
                }
                if !results.is_empty() {
                    return results;
                }
            }

            // Strategy 1b: Capitalize receiver and try
            let type_qualified = format!("{}.{}", capitalize_first(recv), call.callee_name);
            if type_qualified != qualified {
                if let Some(refs) = symbol_index.get(&type_qualified) {
                    for sym_ref in refs {
                        results.push(resolved!(
                            sym_ref.uid.clone(),
                            0.80,
                            "receiver-type-inference".to_string()
                        ));
                    }
                    if !results.is_empty() {
                        return results;
                    }
                }
            }

            // Strategy 1c: Go package-name receiver match
            if !imported_files.is_empty() {
                if let Some(refs) = symbol_index.get(&call.callee_name) {
                    for sym_ref in refs {
                        if sym_ref.is_exported && imported_files.iter().any(|f| {
                            std::path::Path::new(f.as_str())
                                .parent()
                                .and_then(|p| p.file_name())
                                .map_or(false, |dir| dir.to_string_lossy() == *recv)
                                && sym_ref.file_path == *f
                        }) {
                            results.push(resolved!(
                                sym_ref.uid.clone(),
                                0.90,
                                "go-package-match".to_string()
                            ));
                        }
                    }
                    if !results.is_empty() {
                        return results;
                    }
                }
            }
        }
    }

    // Strategy 2: Same-file name match
    for sym in file_symbols {
        if sym.name == call.callee_name {
            results.push(resolved!(sym.uid(), 0.90, "same-file-match".to_string()));
        }
    }
    if !results.is_empty() {
        return results;
    }

    // Strategy 3: Imported-file symbol match
    if !imported_files.is_empty() {
        if let Some(refs) = symbol_index.get(&call.callee_name) {
            for sym_ref in refs {
                if imported_files.contains(&sym_ref.file_path) && sym_ref.is_exported {
                    results.push(resolved!(
                        sym_ref.uid.clone(),
                        0.90,
                        "imported-file-match".to_string()
                    ));
                }
            }
            if !results.is_empty() {
                return results;
            }
        }
    }

    // Strategy 4: Global name match (exported symbols only)
    if let Some(refs) = symbol_index.get(&call.callee_name) {
        let exported: Vec<_> = refs.iter().filter(|r| r.is_exported).collect();
        if exported.len() == 1 {
            results.push(resolved!(
                exported[0].uid.clone(),
                0.80,
                "unique-global-match".to_string()
            ));
        } else if exported.len() >= 2 && exported.len() <= 5 {
            let confidence = 0.50 / exported.len() as f64;
            for r in &exported {
                results.push(resolved!(r.uid.clone(), confidence, "ambiguous-global-match".to_string()));
            }
        }
    }

    // Strategy 5: Ruby method_missing fallback (.rb files only)
    // When a call is unresolved and made via implicit self (no receiver, or self),
    // route it to the class's method_missing at low confidence.
    if results.is_empty() && file_path.ends_with(".rb") {
        let is_implicit_self = call.receiver.is_none()
            || call.receiver.as_deref() == Some("self");
        if is_implicit_self {
            if let Some(ref caller_class) = call.caller_parent {
                let mm_key = format!("{}.method_missing", caller_class);
                if let Some(mm_refs) = symbol_index.get(&mm_key) {
                    for r in mm_refs {
                        results.push(resolved!(r.uid.clone(), 0.30, "method-missing-fallback".to_string()));
                    }
                }
            }
        }
    }

    // Strategy 6: Unresolved fallback — emit CALLS_UNRESOLVED or CALLS_EXTERNAL
    if results.is_empty() {
        let name = match &call.receiver {
            Some(r) => format!("{}.{}", r, call.callee_name),
            None => call.callee_name.clone(),
        };
        // Conservative heuristic: only "::" in receiver → CALLS_EXTERNAL
        let is_external = call.receiver.as_ref().map_or(false, |r| r.contains("::"));
        let kind = if is_external { RelationKind::CallsExternal } else { RelationKind::CallsUnresolved };
        let target_uid = format!("External:<external>:{}:0", name);
        results.push(ResolvedTarget {
            target_uid,
            confidence: 0.0,
            reason: "unresolved".to_string(),
            kind,
        });
    }

    results
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
