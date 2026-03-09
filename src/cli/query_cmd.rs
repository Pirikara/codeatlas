use crate::cli::{QueryArgs, SearchMode};
use crate::embedder::Embedder;
use crate::query::QueryEngine;
use crate::storage::Database;
use anyhow::Result;

pub fn run(args: QueryArgs, json: bool) -> Result<()> {
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let db = Database::open(&db_path)?;
    let engine = QueryEngine::new(db.conn());

    if args.grouped {
        let result = engine.search_grouped(&args.query, args.limit)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            let n_proc = result.processes.len();
            let n_def = result.definitions.len();
            println!(
                "Query results (grouped): \"{}\" — {} symbol(s) in {} process(es), {} definition(s)\n",
                args.query, result.total, n_proc, n_def
            );
            for pg in &result.processes {
                println!("Process: {} ({})", pg.label, pg.process_type);
                for ps in &pg.matched_symbols {
                    println!(
                        "  {} {} — {} (score: {:.4}, step {})",
                        ps.symbol.kind,
                        ps.symbol.name,
                        ps.symbol.file_path,
                        ps.score,
                        ps.step_index,
                    );
                }
            }
            println!("\nDefinitions:");
            if result.definitions.is_empty() {
                println!("  (none)");
            } else {
                for r in &result.definitions {
                    println!(
                        "  {} {} — {} (score: {:.4})",
                        r.symbol.kind, r.symbol.name, r.symbol.file_path, r.score
                    );
                }
            }
        }
        return Ok(());
    }

    let (results, mode_label) = match args.mode {
        SearchMode::Bm25 => {
            let r = engine.search(&args.query, args.limit)?;
            (r, "[BM25]")
        }
        SearchMode::Vector | SearchMode::Hybrid => {
            if !engine.has_embeddings()? {
                eprintln!(
                    "No embeddings found. Run `codeatlas embed` first, then retry."
                );
                eprintln!("Falling back to BM25.");
                let r = engine.search(&args.query, args.limit)?;
                (r, "[BM25 (fallback: no embeddings)]")
            } else {
                let embedder = Embedder::new()?;
                match args.mode {
                    SearchMode::Vector => {
                        let r = engine.search_vector_only(&args.query, args.limit, &embedder)?;
                        (r, "[Vector]")
                    }
                    _ => {
                        let r = engine.search_hybrid(&args.query, args.limit, &embedder)?;
                        (r, "[Hybrid (BM25+Vector/RRF)]")
                    }
                }
            }
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    if results.is_empty() {
        println!("No results for '{}'", args.query);
        return Ok(());
    }

    println!("Search results {} for '{}':\n", mode_label, args.query);
    for (i, r) in results.iter().enumerate() {
        let parent = r
            .symbol
            .parent_name
            .as_deref()
            .map(|p| format!("{}.", p))
            .unwrap_or_default();
        println!(
            "  {}. {} {}{} ({})",
            i + 1,
            r.symbol.kind,
            parent,
            r.symbol.name,
            r.symbol.file_path,
        );
        println!(
            "     line {}-{}  score: {:.4}",
            r.symbol.start_line, r.symbol.end_line, r.score
        );
    }

    Ok(())
}
