use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use crate::cli::{EvalArgs, SearchMode};
use crate::embedder::Embedder;
use crate::eval::{self, EvalMode, EvalReport};
use crate::query::QueryEngine;
use crate::storage::Database;

pub fn run(args: EvalArgs, json: bool) -> Result<()> {
    // 1. fixture JSON 読み込み
    let fixture_path = args.fixture.canonicalize().with_context(|| {
        format!("fixture file not found: {}", args.fixture.display())
    })?;
    let raw = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("failed to read fixture: {}", fixture_path.display()))?;
    let mut fixture: eval::EvalFixture =
        serde_json::from_str(&raw).context("failed to parse fixture JSON")?;

    // 2. fixture_dir 解決
    let fixture_dir = resolve_fixture_dir(&fixture.fixture_dir, &fixture_path)?;

    // fixture フィールドにディレクトリ名を補完
    if fixture.fixture.is_empty() {
        fixture.fixture = fixture_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| fixture.fixture_dir.clone());
    }

    // 3. DB パス確認
    let db_path = fixture_dir.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        bail!(
            "No index found at {}. Run `codeatlas index` on the fixture directory first.",
            fixture_dir.display()
        );
    }

    // 4. DB + QueryEngine
    let db = Database::open(&db_path)?;
    let engine = QueryEngine::new(db.conn());

    // 5. 実行するモードを決定
    let modes: Vec<EvalMode> = if args.grouped {
        vec![EvalMode::Grouped]
    } else if args.all {
        vec![EvalMode::Bm25, EvalMode::Vector, EvalMode::Hybrid]
    } else {
        let m = args.mode.unwrap_or(SearchMode::Bm25);
        vec![cli_to_eval_mode(m)]
    };

    // 6. Vector/Hybrid が必要なら埋め込みチェック
    let needs_embeddings = modes.iter().any(|m| {
        matches!(m, EvalMode::Vector | EvalMode::Hybrid)
    });

    let embedder = if needs_embeddings {
        if !engine.has_embeddings()? {
            eprintln!("warn: no embeddings found; skipping Vector/Hybrid modes. Run `codeatlas embed` first.");
            None
        } else {
            Some(Embedder::new()?)
        }
    } else {
        None
    };

    // 7. 各モードを評価
    let mut reports: Vec<EvalReport> = Vec::new();

    for mode in &modes {
        if matches!(mode, EvalMode::Vector | EvalMode::Hybrid) && embedder.is_none() {
            continue;
        }

        let mut report = if matches!(mode, EvalMode::Grouped) {
            eval::eval_grouped(&engine, &fixture.queries, args.k)?
        } else {
            eval::eval_mode(
                &engine,
                mode.clone(),
                &fixture.queries,
                args.k,
                embedder.as_ref(),
            )?
        };
        report.fixture = fixture.fixture.clone();
        reports.push(report);
    }

    // primary_report はチェック用に参照する（grouped or BM25）
    let primary_report_idx = if args.grouped {
        reports.iter().position(|r| r.mode == "Grouped")
    } else {
        reports.iter().position(|r| r.mode == "BM25")
    };

    // 8. output_file があれば JSON 書き出し
    if let Some(out_path) = &args.output_file {
        let json_str = serde_json::to_string_pretty(&reports)?;
        std::fs::write(out_path, &json_str)
            .with_context(|| format!("failed to write output file: {}", out_path.display()))?;
    }

    // 9. 出力
    if json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        for report in &reports {
            print_report(report);
        }
    }

    // 10. quality gate チェック
    if args.min_recall.is_some() || args.min_mrr.is_some() {
        if let Some(idx) = primary_report_idx {
            let r = &reports[idx];
            if let Some(threshold) = args.min_recall {
                if r.recall_at_k < threshold {
                    bail!(
                        "{} Recall@{} = {:.3} is below threshold {:.3}",
                        r.mode, args.k, r.recall_at_k, threshold
                    );
                }
            }
            if let Some(threshold) = args.min_mrr {
                if r.mrr < threshold {
                    bail!(
                        "{} MRR = {:.3} is below threshold {:.3}",
                        r.mode, r.mrr, threshold
                    );
                }
            }
        } else {
            eprintln!("warn: --min-recall/--min-mrr specified but primary mode was not evaluated");
        }
    }

    if let Some(threshold) = args.min_process_hit {
        if let Some(idx) = primary_report_idx {
            let r = &reports[idx];
            match r.routing_accuracy {
                Some(acc) if acc < threshold => bail!(
                    "Grouped routing_accuracy = {:.3} is below threshold {:.3}", acc, threshold
                ),
                None => eprintln!("warn: --min-process-hit specified but no in_process annotations in fixture"),
                _ => {}
            }
        }
    }

    Ok(())
}

fn cli_to_eval_mode(m: SearchMode) -> EvalMode {
    match m {
        SearchMode::Bm25 => EvalMode::Bm25,
        SearchMode::Vector => EvalMode::Vector,
        SearchMode::Hybrid => EvalMode::Hybrid,
    }
}

/// fixture_dir を解決する:
///   1. 絶対パスならそのまま
///   2. 相対パスなら fixture JSON の親ディレクトリを基準に結合
///   3. 存在しなければ CARGO_MANIFEST_DIR を基準に再試行
fn resolve_fixture_dir(fixture_dir: &str, fixture_json: &std::path::Path) -> Result<PathBuf> {
    let dir = PathBuf::from(fixture_dir);

    if dir.is_absolute() {
        if dir.exists() {
            return Ok(dir);
        }
        bail!("fixture_dir not found: {}", dir.display());
    }

    // fixture JSON の親ディレクトリを基準
    if let Some(parent) = fixture_json.parent() {
        let candidate = parent.join(&dir);
        if candidate.exists() {
            return Ok(candidate.canonicalize()?);
        }
    }

    // CARGO_MANIFEST_DIR フォールバック
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let candidate = PathBuf::from(manifest).join(&dir);
        if candidate.exists() {
            return Ok(candidate.canonicalize()?);
        }
    }

    bail!("fixture_dir '{}' not found (tried relative to fixture JSON and CARGO_MANIFEST_DIR)", fixture_dir);
}

fn print_report(r: &EvalReport) {
    println!();
    println!("Eval: {}  [{}]  k={}", r.fixture, r.mode, r.k);
    let mut summary = format!("  Recall@{}: {:.3}   MRR: {:.3}", r.k, r.recall_at_k, r.mrr);
    if let Some(pr) = r.process_recall {
        summary.push_str(&format!("   ProcessRecall: {:.3}", pr));
    }
    if let Some(ra) = r.routing_accuracy {
        summary.push_str(&format!("   RoutingAcc: {:.3}", ra));
    }
    println!("{}", summary);
    println!("  By category:");

    let print_cat = |label: &str, m: &Option<eval::CategoryMetrics>| {
        if let Some(m) = m {
            println!(
                "    {:<8} ({:>2}): R@{}={:.3}  MRR={:.3}",
                label, m.count, r.k, m.recall_at_k, m.mrr
            );
        }
    };
    print_cat("exact", &r.by_category.exact);
    print_cat("semantic", &r.by_category.semantic);
    print_cat("mixed", &r.by_category.mixed);

    println!("  Per-query:");
    for q in &r.per_query {
        println!(
            "    {:<8} {:<10} {:<30} hits={}/{}  R@{}={:.3}  RR={:.3}",
            q.id,
            q.category,
            truncate(&q.query, 30),
            q.hit_count,
            q.relevant_count,
            r.k,
            q.recall,
            q.reciprocal_rank,
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        format!("{:<width$}", s, width = max)
    } else {
        format!("{:.width$}", s, width = max - 1)
    }
}
