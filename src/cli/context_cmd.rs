use crate::cli::ContextArgs;
use crate::query::{ContextResponse, QueryEngine};
use crate::storage::Database;
use anyhow::Result;

pub fn run(args: ContextArgs, json: bool) -> Result<()> {
    // Validate: need name or uid
    if args.name.is_none() && args.uid.is_none() {
        anyhow::bail!("Either a symbol name or --uid must be provided.");
    }

    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let db = Database::open(&db_path)?;
    let engine = QueryEngine::new(db.conn());

    let response = engine.context_resolved(
        args.name.as_deref(),
        args.uid.as_deref(),
        args.file.as_deref(),
    )?;

    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    match response {
        ContextResponse::NotFound { message } => {
            println!("{}", message);
        }
        ContextResponse::Ambiguous { message, candidates } => {
            println!("Ambiguous: {}", message);
            println!("\nCandidates:");
            println!("  {:<6}  {:<12}  {:<8}  {}", "line", "kind", "exported", "uid");
            println!("  {}", "-".repeat(80));
            for c in &candidates {
                println!(
                    "  {:<6}  {:<12}  {:<8}  {}",
                    c.start_line,
                    c.kind,
                    if c.is_exported { "yes" } else { "no" },
                    c.uid,
                );
            }
        }
        ContextResponse::Found { symbol, incoming, outgoing } => {
            let parent = symbol
                .parent_name
                .as_deref()
                .map(|p| format!(" (in {})", p))
                .unwrap_or_default();
            println!(
                "{} {}{} — {}:{}",
                symbol.kind, symbol.name, parent, symbol.file_path, symbol.start_line
            );
            if symbol.is_exported {
                println!("  [exported]");
            }
            println!("  uid: {}", symbol.uid);

            // Incoming
            if !incoming.is_empty() {
                println!("\n  Incoming ({}):", incoming.len());
                for rel in &incoming {
                    let parent = rel
                        .symbol
                        .parent_name
                        .as_deref()
                        .map(|p| format!("{}.", p))
                        .unwrap_or_default();
                    println!(
                        "    {} {} {}{} — {}:{} (conf: {:.2}, {})",
                        rel.kind,
                        rel.symbol.kind,
                        parent,
                        rel.symbol.name,
                        rel.symbol.file_path,
                        rel.symbol.start_line,
                        rel.confidence,
                        rel.reason,
                    );
                }
            }

            // Outgoing
            if !outgoing.is_empty() {
                println!("\n  Outgoing ({}):", outgoing.len());
                for rel in &outgoing {
                    let parent = rel
                        .symbol
                        .parent_name
                        .as_deref()
                        .map(|p| format!("{}.", p))
                        .unwrap_or_default();
                    println!(
                        "    {} {} {}{} — {}:{} (conf: {:.2}, {})",
                        rel.kind,
                        rel.symbol.kind,
                        parent,
                        rel.symbol.name,
                        rel.symbol.file_path,
                        rel.symbol.start_line,
                        rel.confidence,
                        rel.reason,
                    );
                }
            }

            if incoming.is_empty() && outgoing.is_empty() {
                println!("\n  No relationships found.");
            }
        }
    }

    Ok(())
}
