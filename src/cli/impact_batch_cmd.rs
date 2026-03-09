use crate::cli::ImpactBatchArgs;
use crate::query::{ImpactBatchResult, QueryEngine, VALID_SYMBOL_KINDS};
use crate::storage::Database;
use anyhow::Result;
use std::collections::HashSet;

pub fn run(args: ImpactBatchArgs, json: bool) -> Result<()> {
    if args.symbols.is_none() && args.ranges.is_none() {
        anyhow::bail!("at least one of --symbols or --ranges must be provided");
    }

    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let direction_str = match args.direction {
        crate::cli::ImpactDirection::Upstream => "upstream",
        crate::cli::ImpactDirection::Downstream => "downstream",
    };

    // Build kind filter
    let kind_filter: Option<HashSet<&str>> = if args.all_kinds {
        None
    } else {
        match &args.kinds {
            None => Some(["Function", "Method"].iter().copied().collect()),
            Some(kinds) if kinds.is_empty() => None,
            Some(kinds) => {
                for kind in kinds {
                    if !VALID_SYMBOL_KINDS.contains(&kind.as_str()) {
                        anyhow::bail!(
                            "invalid symbol kind: '{}'. Valid kinds: {}",
                            kind,
                            VALID_SYMBOL_KINDS.join(", ")
                        );
                    }
                }
                Some(kinds.iter().map(|s| s.as_str()).collect())
            }
        }
    };

    let db = Database::open(&db_path)?;
    let engine = QueryEngine::new(db.conn());

    let mut seen_ids: HashSet<i64> = HashSet::new();
    let mut collected: Vec<(String, i64, crate::query::SymbolInfo)> = Vec::new();

    // --symbols: [{"id": 123}, {"name": "Execute", "file": "cmd/root.go"}]
    if let Some(ref symbols_json) = args.symbols {
        let entries: Vec<serde_json::Value> = serde_json::from_str(symbols_json)
            .map_err(|e| anyhow::anyhow!("invalid --symbols JSON: {}", e))?;

        for entry in &entries {
            if let Some(id) = entry.get("id").and_then(|v| v.as_i64()) {
                // id-based lookup
                if let Some(sym) = engine.get_symbol_by_id_pub(id)? {
                    if let Some(ref filter) = kind_filter {
                        if !filter.contains(sym.kind.as_str()) {
                            continue;
                        }
                    }
                    if seen_ids.insert(id) {
                        collected.push((sym.file_path.clone(), id, sym));
                    }
                }
            } else {
                let name = entry.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    anyhow::anyhow!("symbol entry must have 'id' or 'name': {}", entry)
                })?;
                let file = entry.get("file").and_then(|v| v.as_str()).ok_or_else(|| {
                    anyhow::anyhow!("symbol entry with 'name' must also have 'file': {}", entry)
                })?;
                if let Some((id, sym)) = engine.symbol_by_name_file(name, file)? {
                    if let Some(ref filter) = kind_filter {
                        if !filter.contains(sym.kind.as_str()) {
                            continue;
                        }
                    }
                    if seen_ids.insert(id) {
                        collected.push((sym.file_path.clone(), id, sym));
                    }
                }
            }
        }
    }

    // --ranges: [{"file": "cmd/root.go", "start": 13, "end": 15}]
    if let Some(ref ranges_json) = args.ranges {
        let entries: Vec<serde_json::Value> = serde_json::from_str(ranges_json)
            .map_err(|e| anyhow::anyhow!("invalid --ranges JSON: {}", e))?;

        for entry in &entries {
            let file = entry
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("range entry must have 'file': {}", entry))?;
            let start = entry
                .get("start")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("range entry must have 'start': {}", entry))?;
            let end = entry
                .get("end")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("range entry must have 'end': {}", entry))?;

            let symbols = engine.symbols_in_range(file, start, end)?;
            for (id, sym) in symbols {
                if let Some(ref filter) = kind_filter {
                    if !filter.contains(sym.kind.as_str()) {
                        continue;
                    }
                }
                if seen_ids.insert(id) {
                    collected.push((file.to_string(), id, sym));
                }
            }
        }
    }

    // Sort by file_path ASC, start_line ASC, id ASC
    collected.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.2.start_line.cmp(&b.2.start_line))
            .then(a.1.cmp(&b.1))
    });

    let total = collected.len();
    let truncated = total > args.max_symbols;
    if truncated {
        collected.truncate(args.max_symbols);
    }

    let mut results = Vec::new();
    for (file, id, sym) in collected {
        let impact =
            engine.impact_by_id(id, direction_str, args.depth, args.min_confidence, args.calls_only)?;
        results.push(crate::query::ChangedSymbolEntry { file, symbol: sym, impact });
    }

    let result = ImpactBatchResult { results, total, truncated };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Text output
    println!("Impact batch — {} symbol(s) found", result.total);
    if result.truncated {
        println!("  (showing {} of {})", result.results.len(), result.total);
    }
    if result.results.is_empty() {
        println!("  No matching symbols found.");
        return Ok(());
    }
    for entry in &result.results {
        let impact_count = entry.impact.as_ref().map(|i| i.total_affected).unwrap_or(0);
        println!(
            "  {} {} — {}:{} ({} impact: {})",
            entry.symbol.kind,
            entry.symbol.name,
            entry.file,
            entry.symbol.start_line,
            direction_str,
            impact_count,
        );
    }

    Ok(())
}
