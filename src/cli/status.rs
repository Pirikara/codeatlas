use crate::cli::StatusArgs;
use crate::storage::Database;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
struct StatusJson {
    symbol_count: usize,
    relationship_count: usize,
    file_count: usize,
    community_count: usize,
    process_count: usize,
    last_indexed: Option<String>,
}

pub fn run(args: StatusArgs, json: bool) -> Result<()> {
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");

    if !db_path.exists() {
        if json {
            println!("null");
        } else {
            println!("No index found. Run `codeatlas index` first.");
        }
        return Ok(());
    }

    let db = Database::open(&db_path)?;
    let stats = db.stats()?;

    if json {
        let j = StatusJson {
            symbol_count: stats.symbol_count,
            relationship_count: stats.relationship_count,
            file_count: stats.file_count,
            community_count: stats.community_count,
            process_count: stats.process_count,
            last_indexed: stats.last_indexed.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&j)?);
        return Ok(());
    }

    println!("CodeAtlas index: {}", path.display());
    println!("  Symbols:       {}", stats.symbol_count);
    println!("  Relationships: {}", stats.relationship_count);
    println!("  Files indexed: {}", stats.file_count);
    println!("  Communities:   {}", stats.community_count);
    println!("  Processes:     {}", stats.process_count);

    if let Some(last) = stats.last_indexed {
        println!("  Last indexed:  {}", last);
    }

    Ok(())
}
