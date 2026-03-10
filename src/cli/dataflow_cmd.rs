use crate::cli::DataflowArgs;
use crate::query::QueryEngine;
use crate::storage::Database;
use anyhow::Result;

pub fn run(args: DataflowArgs, json: bool) -> Result<()> {
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    let db = Database::open(&db_path)?;
    let engine = QueryEngine::new(db.conn());

    let result = engine.dataflow(
        args.name.as_deref(),
        args.file.as_deref(),
        args.uid.as_deref(),
    )?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Data flows for {} ({}):", result.symbol.name, result.symbol.kind);
        println!("  File: {}:{}-{}", result.symbol.file_path, result.symbol.start_line, result.symbol.end_line);
        println!();
        if result.flows.is_empty() {
            println!("  No data flows detected.");
        } else {
            for flow in &result.flows {
                println!(
                    "  L{}-L{} [{}] {} → {}",
                    flow.source_line, flow.sink_line, flow.flow_kind, flow.source_expr, flow.sink_expr
                );
            }
            println!("\n  Total: {} flows", result.flows.len());
        }
    }

    Ok(())
}
