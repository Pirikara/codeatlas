use crate::cli::EmbedArgs;
use crate::embedder::{Embedder, DIMS, MODEL_ID};
use crate::storage::write::EmbeddingRecord;
use crate::storage::Database;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

const BATCH_SIZE: usize = 64;

pub fn run(args: EmbedArgs) -> Result<()> {
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let db = Database::open(&db_path)?;

    let symbols = if args.force {
        db.get_all_symbols_for_embed()?
    } else {
        db.get_symbols_needing_embed(MODEL_ID, DIMS as i64)?
    };

    let total = symbols.len();
    if total == 0 {
        println!("Already up to date.");
        return Ok(());
    }

    println!("Loading embedding model…");
    let embedder = Embedder::new()?;

    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    for chunk in symbols.chunks(BATCH_SIZE) {
        let texts: Vec<String> = chunk
            .iter()
            .map(|(_, name, kind, file_path, parent_name, _)| {
                Embedder::make_text(name, kind, file_path, parent_name.as_deref())
            })
            .collect();

        let vecs = embedder.embed_batch(&texts)?;
        if vecs.len() != texts.len() {
            anyhow::bail!(
                "embed_batch returned {} vectors for {} texts",
                vecs.len(),
                texts.len()
            );
        }

        let records: Vec<EmbeddingRecord> = chunk
            .iter()
            .zip(vecs.into_iter())
            .map(|((symbol_id, _, _, _, _, content_hash), vec)| EmbeddingRecord {
                symbol_id: *symbol_id,
                model_id: MODEL_ID.to_string(),
                dims: DIMS,
                vector: vec,
                content_hash: *content_hash,
            })
            .collect();

        db.upsert_embeddings(&records)?;
        pb.inc(chunk.len() as u64);
    }

    pb.finish_with_message("Done");
    println!("Embedded {} symbols.", total);
    Ok(())
}
