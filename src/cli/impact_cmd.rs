use crate::cli::ImpactArgs;
use crate::query::QueryEngine;
use crate::storage::Database;
use anyhow::Result;

pub fn run(args: ImpactArgs, json: bool) -> Result<()> {
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let db = Database::open(&db_path)?;
    let engine = QueryEngine::new(db.conn());

    let result = engine.impact(&args.name, &args.direction, args.depth, args.min_confidence, args.calls_only)?;
    let Some(result) = result else {
        if json {
            println!("null");
        } else {
            println!("Symbol '{}' not found.", args.name);
        }
        return Ok(());
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!(
        "Impact analysis: {} {} ({})\n",
        result.target.kind, result.target.name, result.direction
    );
    println!("Risk: {}", result.risk);
    println!("Summary: {}\n", result.summary);

    if !result.affected_modules.is_empty() {
        println!("Affected modules ({}):", result.affected_modules.len());
        for m in &result.affected_modules {
            println!("  - {}", m.label);
        }
        println!();
    }

    if !result.affected_processes.is_empty() {
        println!("Affected processes ({}):", result.affected_processes.len());
        for p in &result.affected_processes {
            println!("  - {} ({})", p.label, p.process_type);
        }
        println!();
    }

    if result.total_affected == 0 {
        println!("  No affected symbols found.");
        return Ok(());
    }

    println!("  Total affected: {}\n", result.total_affected);

    let mut depths: Vec<_> = result.by_depth.keys().collect();
    depths.sort();

    for depth in depths {
        let entries = &result.by_depth[depth];
        let risk_label = match depth {
            1 => "WILL BREAK",
            2 => "LIKELY AFFECTED",
            _ => "MAY NEED TESTING",
        };
        println!("  Depth {} — {} ({} symbols):", depth, risk_label, entries.len());

        for entry in entries {
            let parent = entry
                .symbol
                .parent_name
                .as_deref()
                .map(|p| format!("{}.", p))
                .unwrap_or_default();
            println!(
                "    {} {}{} — {}:{} (conf: {:.2})",
                entry.symbol.kind,
                parent,
                entry.symbol.name,
                entry.symbol.file_path,
                entry.symbol.start_line,
                entry.confidence,
            );
        }
        println!();
    }

    Ok(())
}
