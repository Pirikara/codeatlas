use std::collections::HashMap;

use crate::parser::Symbol;
use crate::parser::SymbolKind;

use super::super::SymbolIndex;
use super::{Relationship, RelationKind};

pub fn resolve_inheritance(
    _file_path: &str,
    symbols: &[Symbol],
    symbol_index: &SymbolIndex,
    relationships: &mut Vec<Relationship>,
) {
    for sym in symbols {
        let source_uid = sym.uid();

        // EXTENDS: superclass
        if let Some(ref superclass) = sym.superclass {
            let superclass_name = if symbol_index.contains_key(superclass) {
                superclass.clone()
            } else {
                superclass
                    .rsplit("::")
                    .next()
                    .or_else(|| superclass.rsplit('.').next())
                    .unwrap_or(superclass)
                    .to_string()
            };
            if let Some(targets) = symbol_index.get(&superclass_name) {
                let best = targets
                    .iter()
                    .find(|t| t.kind == "Class" || t.kind == "Struct")
                    .or(targets.first());
                if let Some(target) = best {
                    relationships.push(Relationship {
                        source_uid: source_uid.clone(),
                        target_uid: target.uid.clone(),
                        kind: RelationKind::Extends,
                        confidence: 1.0,
                        reason: "explicit-extends".to_string(),
                    });
                }
            }
        }

        // IMPLEMENTS / EXTENDS for interfaces list
        let is_source_interface = sym.kind == SymbolKind::Interface;
        for iface_name in &sym.interfaces {
            // Try full name first; if no Interface/Module found, try short name
            let short_name = iface_name.rsplit("::").next()
                .or_else(|| iface_name.rsplit('.').next())
                .unwrap_or(iface_name);
            let full_targets = symbol_index.get(iface_name);
            let short_targets = if short_name != iface_name { symbol_index.get(short_name) } else { None };

            // Prefer Interface/Module match from either full or short name
            let best = full_targets.and_then(|ts| ts.iter().find(|t| t.kind == "Interface" || t.kind == "Module"))
                .or_else(|| short_targets.and_then(|ts| ts.iter().find(|t| t.kind == "Interface" || t.kind == "Module")))
                .or_else(|| full_targets.and_then(|ts| ts.first()))
                .or_else(|| short_targets.and_then(|ts| ts.first()));
            if let Some(target) = best {
                let (kind, reason) = if is_source_interface {
                    (RelationKind::Extends, "interface-extends".to_string())
                } else {
                    (RelationKind::Implements, "explicit-implements".to_string())
                };
                relationships.push(Relationship {
                    source_uid: source_uid.clone(),
                    target_uid: target.uid.clone(),
                    kind,
                    confidence: 1.0,
                    reason,
                });
            }
        }
    }
}

/// Detect Go implicit interface implementation via structural type matching.
/// If a struct implements all methods of an interface (name + param_count), create an IMPLEMENTS edge.
pub fn resolve_go_implicit_implements(
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Collect interface → method name → param_count
    let mut iface_methods: HashMap<String, HashMap<String, Option<usize>>> = HashMap::new();
    // Collect struct → method name → param_count
    let mut struct_methods: HashMap<String, HashMap<String, Option<usize>>> = HashMap::new();
    // Track UIDs
    let mut iface_uids: HashMap<String, String> = HashMap::new();
    let mut struct_uids: HashMap<String, String> = HashMap::new();

    for sym in all_symbols {
        match sym.kind {
            SymbolKind::Interface => {
                iface_uids.insert(sym.name.clone(), sym.uid());
                iface_methods.entry(sym.name.clone()).or_default();
            }
            SymbolKind::Struct => {
                struct_uids.insert(sym.name.clone(), sym.uid());
                struct_methods.entry(sym.name.clone()).or_default();
            }
            SymbolKind::Method => {
                if let Some(ref parent) = sym.parent_name {
                    if iface_methods.contains_key(parent) {
                        iface_methods.get_mut(parent).unwrap()
                            .insert(sym.name.clone(), sym.param_count);
                    } else {
                        struct_methods.entry(parent.clone()).or_default()
                            .insert(sym.name.clone(), sym.param_count);
                    }
                }
            }
            _ => {}
        }
    }

    // For each struct, check if it satisfies any interface (name + param_count match)
    for (struct_name, s_methods) in &struct_methods {
        for (iface_name, i_methods) in &iface_methods {
            if i_methods.is_empty() {
                continue;
            }
            let all_match = i_methods.iter().all(|(method_name, i_count)| {
                s_methods.get(method_name).map_or(false, |s_count| {
                    match (i_count, s_count) {
                        (Some(i), Some(s)) => i == s,
                        _ => true, // if either count is unknown, fall back to name-only
                    }
                })
            });
            if all_match {
                if let (Some(struct_uid), Some(iface_uid)) =
                    (struct_uids.get(struct_name), iface_uids.get(iface_name))
                {
                    relationships.push(Relationship {
                        source_uid: struct_uid.clone(),
                        target_uid: iface_uid.clone(),
                        kind: RelationKind::Implements,
                        confidence: 0.85,
                        reason: "go-implicit-implements".to_string(),
                    });
                }
            }
        }
    }
}
