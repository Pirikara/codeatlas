use crate::cli::ClustersArgs;
use crate::storage::Database;
use anyhow::Result;

pub fn run(args: ClustersArgs, json: bool) -> Result<()> {
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let db = Database::open(&db_path)?;
    let communities = db.list_communities()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&communities)?);
        return Ok(());
    }

    if communities.is_empty() {
        println!("No communities detected.");
        return Ok(());
    }

    println!("Communities ({}):\n", communities.len());
    for comm in &communities {
        let label = comm.label.as_deref().unwrap_or("(unlabeled)");
        println!(
            "  #{}: {} — {} symbols, cohesion: {:.2}",
            comm.id, label, comm.symbol_count, comm.cohesion
        );
        if !comm.top_symbols.is_empty() {
            for sym in &comm.top_symbols {
                println!("    - {}", sym);
            }
        }
        println!();
    }

    Ok(())
}
