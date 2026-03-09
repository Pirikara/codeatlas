use crate::analyzer::{self, FileAnalysis};
use crate::cli::IndexArgs;
use crate::parser::ParserPool;
use crate::scanner::{self, ScanResult};
use crate::storage::Database;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static PARSER_POOL: RefCell<Option<ParserPool>> = RefCell::new(None);
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RegistryEntry {
    name: String,
    path: String,
    indexed_at: String,
}

fn get_registry_path() -> Result<std::path::PathBuf> {
    if let Ok(p) = std::env::var("CODEATLAS_REGISTRY_PATH") {
        return Ok(std::path::PathBuf::from(p));
    }
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(home.join(".codeatlas").join("registry.json"))
}

fn resolve_unique_name(entries: &[RegistryEntry], base: &str) -> String {
    if !entries.iter().any(|e| e.name == base) {
        return base.to_string();
    }
    let mut n = 2usize;
    loop {
        let candidate = format!("{}-{}", base, n);
        if !entries.iter().any(|e| e.name == candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn register_repo(repo_path: &std::path::Path) -> Result<()> {
    let registry_path = get_registry_path()?;
    std::fs::create_dir_all(registry_path.parent().unwrap())?;

    let path_str = repo_path.to_string_lossy().to_string();
    let indexed_at = chrono::Utc::now().to_rfc3339();

    let mut entries: Vec<RegistryEntry> = if registry_path.exists() {
        let text = std::fs::read_to_string(&registry_path)?;
        match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Registry file {:?} is malformed ({}). Fix or delete it before re-indexing.",
                    registry_path,
                    e
                ));
            }
        }
    } else {
        vec![]
    };

    if let Some(entry) = entries.iter_mut().find(|e| e.path == path_str) {
        entry.indexed_at = indexed_at;
    } else {
        let base_name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "repo".into());
        let name = resolve_unique_name(&entries, &base_name);
        entries.push(RegistryEntry { name, path: path_str, indexed_at });
    }

    let tmp_path = registry_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, serde_json::to_string_pretty(&entries)?)?;
    std::fs::rename(&tmp_path, &registry_path)?;
    Ok(())
}

pub fn run(args: IndexArgs) -> Result<()> {
    let path = args.path.canonicalize()?;
    let collect_metrics = args.metrics;
    println!("Indexing: {}", path.display());

    let start = Instant::now();

    // Phase 1: Scan files
    let phase_start = Instant::now();
    let pb = create_spinner("Scanning files...");
    let scan_result = scanner::scan(&path)?;
    pb.finish_with_message(format!("Found {} files", scan_result.files.len()));
    let scan_duration = phase_start.elapsed();

    // Phase 2: Open (or create) database
    let db_path = path.join(".codeatlas").join("index.db");
    std::fs::create_dir_all(db_path.parent().unwrap())?;
    let db = Database::open(&db_path)?;

    // Phase 3: Clean up files that no longer exist on disk (always, including --force)
    let current_paths: std::collections::HashSet<String> = scan_result
        .files
        .iter()
        .map(|f| f.relative_path.clone())
        .collect();
    let indexed_paths = db.get_all_indexed_paths()?;
    let stale: Vec<String> = indexed_paths
        .into_iter()
        .filter(|p| !current_paths.contains(p))
        .collect();
    let cleaned = db.cleanup_deleted_files(&stale)?;
    if cleaned > 0 {
        println!("Removed {} deleted file(s) from index.", cleaned);
    }

    // Phase 4: Determine which files need (re-)indexing.
    // If files were deleted, re-index all remaining files so communities/processes
    // are recomputed with a consistent symbol set.
    let files_to_index = if args.force || cleaned > 0 {
        scan_result.files.clone()
    } else {
        filter_changed_files(&db, &scan_result)?
    };

    let total = files_to_index.len();
    if total == 0 {
        println!("Index is up to date. Nothing to do.");
        // Still register so registry stays consistent even if index was already current.
        if let Err(e) = register_repo(&path) {
            eprintln!("Warning: failed to update registry: {}", e);
        }
        return Ok(());
    }

    println!(
        "{} file(s) to index (out of {} total)",
        total,
        scan_result.files.len()
    );

    // Phase 5: Parse files — extract symbols, imports, and calls (parallel via rayon)
    let phase_start = Instant::now();
    let mut files_to_index = files_to_index;
    files_to_index.sort_by(|a, b| a.path.cmp(&b.path));

    let pb = create_progress_bar(total as u64, "Parsing");
    let pb2 = pb.clone();
    let parse_failures = std::sync::atomic::AtomicUsize::new(0);

    let parse_results: Vec<anyhow::Result<Option<FileAnalysis>>> = files_to_index
        .par_iter()
        .map(|file_info| {
            let source = std::fs::read(&file_info.path)?;
            let rel_path = file_info
                .path
                .strip_prefix(&path)
                .unwrap_or(&file_info.path)
                .to_string_lossy()
                .to_string();

            // Outer Result: ParserPool init failure → hard error (? propagates up).
            // Inner Result: parse_full failure → soft warning (handled below).
            let parse_result = PARSER_POOL.with(|cell| -> anyhow::Result<anyhow::Result<_>> {
                let mut opt = cell.borrow_mut();
                if opt.is_none() {
                    *opt = Some(ParserPool::new()?);
                }
                let pool = opt.as_mut().unwrap();
                Ok(pool.parse_full(file_info.language, &source))
            })?;

            pb2.inc(1);

            match parse_result {
                Ok((mut symbols, imports, calls)) => {
                    for sym in &mut symbols {
                        sym.file_path = rel_path.clone();
                    }
                    Ok(Some(FileAnalysis { file_path: rel_path, symbols, imports, calls }))
                }
                Err(e) => {
                    parse_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    eprintln!("Warning: failed to parse {}: {}", file_info.path.display(), e);
                    Ok(None)
                }
            }
        })
        .collect();

    pb.finish_and_clear();

    let parse_failure_count = parse_failures.into_inner();
    let mut analyses = Vec::new();
    for result in parse_results {
        if let Some(fa) = result? {
            analyses.push(fa);
        }
    }

    let all_symbols: Vec<_> = analyses.iter().flat_map(|a| a.symbols.iter().cloned()).collect();
    let files_parsed = analyses.len();

    println!(
        "Extracted {} symbols from {} files",
        all_symbols.len(),
        files_parsed
    );
    let parse_duration = phase_start.elapsed();

    // Phase 6: Resolve relationships (imports, calls, inheritance)
    let phase_start = Instant::now();
    let pb = create_spinner("Resolving relationships...");
    let relationships = analyzer::resolve_relationships(&analyses);
    pb.finish_with_message(format!("Resolved {} relationships", relationships.len()));
    let resolve_duration = phase_start.elapsed();

    // Free analyses — no longer needed after relationship resolution
    drop(analyses);

    // Phase 7: Community detection
    let phase_start = Instant::now();
    let pb = create_spinner("Detecting communities...");
    let call_edges: Vec<(String, String)> = relationships
        .iter()
        .filter(|r| r.kind == analyzer::resolver::RelationKind::Calls)
        .map(|r| (r.source_uid.clone(), r.target_uid.clone()))
        .collect();

    let symbol_names: std::collections::HashMap<String, String> = all_symbols
        .iter()
        .map(|s| (s.uid(), s.name.clone()))
        .collect();

    let communities = analyzer::community::detect_communities(&call_edges, &symbol_names);
    pb.finish_with_message(format!("Found {} communities", communities.len()));
    let community_duration = phase_start.elapsed();

    // Phase 8: Process detection (execution flows)
    let phase_start = Instant::now();
    let pb = create_spinner("Detecting execution flows...");

    // Build community membership map (uid -> community_id)
    let mut community_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for comm in &communities {
        for member_uid in &comm.members {
            community_map.insert(member_uid.clone(), comm.id);
        }
    }

    // Build call edges with confidence for process detection
    let call_edges_with_conf: Vec<(String, String, f64)> = relationships
        .iter()
        .filter(|r| r.kind == analyzer::resolver::RelationKind::Calls)
        .map(|r| (r.source_uid.clone(), r.target_uid.clone(), r.confidence))
        .collect();

    let process_config = analyzer::process::ProcessConfig::for_symbol_count(all_symbols.len());
    let processes = analyzer::process::detect_processes(
        &call_edges_with_conf,
        &symbol_names,
        &community_map,
        &process_config,
    );
    pb.finish_with_message(format!("Found {} execution flows", processes.len()));
    let process_duration = phase_start.elapsed();

    // Phase 9: Store in database
    let phase_start = Instant::now();
    let pb = create_spinner("Storing...");
    db.store_symbols(&all_symbols, &files_to_index, &path)?;
    db.store_relationships(&relationships)?;
    db.store_communities(&communities)?;
    db.store_processes(&processes)?;
    pb.finish_with_message("Stored");
    let store_duration = phase_start.elapsed();

    let elapsed = start.elapsed();
    println!(
        "\nDone in {:.1}s — {} symbols, {} relationships, {} communities, {} flows",
        elapsed.as_secs_f64(),
        all_symbols.len(),
        relationships.len(),
        communities.len(),
        processes.len(),
    );

    // Print metrics if requested
    if collect_metrics {
        use super::metrics::{self, IndexMetrics};
        let m = IndexMetrics {
            scan_duration,
            parse_duration,
            resolve_duration,
            community_duration,
            process_duration,
            store_duration,
            total_duration: elapsed,
            files_scanned: scan_result.files.len(),
            files_parsed,
            parse_failures: parse_failure_count,
            symbol_count: all_symbols.len(),
            relationship_count: relationships.len(),
            community_count: communities.len(),
            process_count: processes.len(),
            peak_rss_bytes: metrics::peak_rss_bytes(),
        };
        m.print();
    }

    // Phase 10: Register in global registry (non-fatal on failure)
    if let Err(e) = register_repo(&path) {
        eprintln!("Warning: failed to update registry: {}", e);
    }

    Ok(())
}

fn filter_changed_files(
    db: &Database,
    scan_result: &ScanResult,
) -> Result<Vec<scanner::FileInfo>> {
    let mut changed = Vec::new();
    for file in &scan_result.files {
        let needs_update = match db.get_file_hash(&file.relative_path)? {
            Some(stored_hash) => stored_hash != file.content_hash,
            None => true,
        };
        if needs_update {
            changed.push(file.clone());
        }
    }
    Ok(changed)
}

fn create_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb
}

fn create_progress_bar(len: u64, prefix: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("━━╸"),
    );
    pb.set_prefix(prefix.to_string());
    pb
}
