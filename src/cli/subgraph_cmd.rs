use crate::cli::SubgraphArgs;
use crate::query::QueryEngine;
use crate::storage::Database;
use anyhow::Result;

pub fn run(args: SubgraphArgs, json: bool) -> Result<()> {
    if args.name.is_none() && args.uid.is_none() && args.id.is_none() {
        anyhow::bail!("Provide a symbol name, --uid, or --id");
    }

    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let db = Database::open(&db_path)?;
    let engine = QueryEngine::new(db.conn());

    let result = engine.subgraph(
        args.name.as_deref(),
        args.uid.as_deref(),
        args.id,
        args.file.as_deref(),
        &args.direction,
        args.depth,
        &args.edge_types,
        args.max_nodes,
        args.max_edges,
    )?;

    let Some(result) = result else {
        if json {
            println!("null");
        } else {
            let label = if let Some(id) = args.id {
                format!("id={}", id)
            } else if let Some(uid) = &args.uid {
                format!("uid='{}'", uid)
            } else {
                format!("'{}'", args.name.as_deref().unwrap_or(""))
            };
            println!("Symbol {} not found.", label);
        }
        return Ok(());
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Human-readable output
    let label = if let Some(id) = args.id {
        format!("id={}", id)
    } else if let Some(uid) = &args.uid {
        format!("uid='{}'", uid)
    } else {
        args.name.as_deref().unwrap_or("").to_string()
    };
    println!(
        "Subgraph from '{}' (direction: {}, depth: {})\n",
        label, args.direction, args.depth
    );
    println!("Nodes ({}):", result.node_count);
    for node in &result.nodes {
        let parent = node.symbol.parent_name.as_deref()
            .map(|p| format!("{}.", p))
            .unwrap_or_default();
        println!(
            "  [{}] {} {}{} — {}:{}",
            node.id, node.symbol.kind, parent, node.symbol.name,
            node.symbol.file_path, node.symbol.start_line,
        );
    }

    println!("\nEdges ({}):", result.edge_count);
    for edge in &result.edges {
        println!(
            "  {} --[{}]--> {} (conf: {:.2})",
            edge.source_id, edge.kind, edge.target_id, edge.confidence
        );
    }

    if result.truncated {
        if let Some(reason) = &result.truncated_reason {
            println!("\n[TRUNCATED: {}]", reason);
        }
    }

    Ok(())
}
