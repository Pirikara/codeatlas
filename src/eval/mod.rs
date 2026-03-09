use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::embedder::Embedder;
use crate::query::{GroupedQueryResult, QueryEngine, SearchResult};

// ─── Mode ────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum EvalMode {
    Bm25,
    Vector,
    Hybrid,
    Grouped,
}

impl EvalMode {
    pub fn label(&self) -> &'static str {
        match self {
            EvalMode::Bm25 => "BM25",
            EvalMode::Vector => "Vector",
            EvalMode::Hybrid => "Hybrid",
            EvalMode::Grouped => "Grouped",
        }
    }
}

// ─── Fixture format ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EvalFixture {
    pub fixture: String,
    pub fixture_dir: String,
    #[allow(dead_code)]
    pub description: String,
    pub queries: Vec<EvalQuery>,
}

#[derive(Deserialize)]
pub struct EvalQuery {
    pub id: String,
    pub query: String,
    pub category: QueryCategory,
    pub relevant: Vec<RelevantSymbol>,
    #[allow(dead_code)]
    pub notes: Option<String>,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum QueryCategory {
    Exact,
    Semantic,
    Mixed,
}

impl QueryCategory {
    fn label(&self) -> &'static str {
        match self {
            QueryCategory::Exact => "exact",
            QueryCategory::Semantic => "semantic",
            QueryCategory::Mixed => "mixed",
        }
    }
}

#[derive(Deserialize)]
pub struct RelevantSymbol {
    pub name: String,
    pub file_path: String,
    pub kind: String,
    #[serde(default)]
    pub in_process: Option<bool>,
}

// ─── Report types ────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct EvalReport {
    pub fixture: String,
    pub mode: String,
    pub k: usize,
    pub recall_at_k: f64,
    pub mrr: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_recall: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_accuracy: Option<f64>,
    pub per_query: Vec<QueryResult>,
    pub by_category: CategoryBreakdown,
}

#[derive(Serialize)]
pub struct QueryResult {
    pub id: String,
    pub query: String,
    pub category: String,
    pub relevant_count: usize,
    pub hit_count: usize,
    pub first_relevant_rank: Option<usize>,
    pub recall: f64,
    pub reciprocal_rank: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_recall: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_accuracy: Option<f64>,
}

#[derive(Serialize, Default)]
pub struct CategoryBreakdown {
    pub exact: Option<CategoryMetrics>,
    pub semantic: Option<CategoryMetrics>,
    pub mixed: Option<CategoryMetrics>,
}

#[derive(Serialize)]
pub struct CategoryMetrics {
    pub count: usize,
    pub recall_at_k: f64,
    pub mrr: f64,
}

// ─── Hit判定 ─────────────────────────────────────────────────────────

fn is_hit(result: &SearchResult, relevant: &[RelevantSymbol]) -> bool {
    relevant.iter().any(|r| {
        result.symbol.name == r.name
            && result.symbol.file_path == r.file_path
            && result.symbol.kind == r.kind
    })
}

// ─── 指標計算 ────────────────────────────────────────────────────────

/// Recall@k = (top-k 中のヒット数) / relevant.len()
pub fn recall_at_k(results: &[SearchResult], relevant: &[RelevantSymbol], k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let hits = results.iter().take(k).filter(|r| is_hit(r, relevant)).count();
    hits as f64 / relevant.len() as f64
}

/// Reciprocal Rank = 1 / (最初のヒット順位), なければ 0.0
pub fn reciprocal_rank(results: &[SearchResult], relevant: &[RelevantSymbol], k: usize) -> f64 {
    for (i, result) in results.iter().take(k).enumerate() {
        if is_hit(result, relevant) {
            return 1.0 / (i + 1) as f64;
        }
    }
    0.0
}

// ─── モード評価 ──────────────────────────────────────────────────────

pub fn eval_mode(
    engine: &QueryEngine,
    mode: EvalMode,
    queries: &[EvalQuery],
    k: usize,
    embedder: Option<&Embedder>,
) -> Result<EvalReport> {
    let fixture = String::new(); // filled by caller
    let mode_label = mode.label().to_string();

    let mut per_query: Vec<QueryResult> = Vec::new();

    for eq in queries {
        let results = match &mode {
            EvalMode::Bm25 => engine.search(&eq.query, k)?,
            EvalMode::Vector => {
                let emb = embedder.expect("embedder required for vector mode");
                engine.search_vector_only(&eq.query, k, emb)?
            }
            EvalMode::Hybrid => {
                let emb = embedder.expect("embedder required for hybrid mode");
                engine.search_hybrid(&eq.query, k, emb)?
            }
            EvalMode::Grouped => bail!("use eval_grouped() for Grouped mode"),
        };

        let recall = recall_at_k(&results, &eq.relevant, k);
        let rr = reciprocal_rank(&results, &eq.relevant, k);
        let hit_count = results.iter().take(k).filter(|r| is_hit(r, &eq.relevant)).count();
        let first_relevant_rank = results
            .iter()
            .take(k)
            .enumerate()
            .find(|(_, r)| is_hit(r, &eq.relevant))
            .map(|(i, _)| i + 1);

        per_query.push(QueryResult {
            id: eq.id.clone(),
            query: eq.query.clone(),
            category: eq.category.label().to_string(),
            relevant_count: eq.relevant.len(),
            hit_count,
            first_relevant_rank,
            recall,
            reciprocal_rank: rr,
            process_recall: None,
            routing_accuracy: None,
        });
    }

    // aggregate overall
    let overall_recall = if per_query.is_empty() {
        0.0
    } else {
        per_query.iter().map(|q| q.recall).sum::<f64>() / per_query.len() as f64
    };
    let overall_mrr = if per_query.is_empty() {
        0.0
    } else {
        per_query.iter().map(|q| q.reciprocal_rank).sum::<f64>() / per_query.len() as f64
    };

    // by category
    let by_category = compute_category_breakdown(&per_query, queries);

    Ok(EvalReport {
        fixture,
        mode: mode_label,
        k,
        recall_at_k: overall_recall,
        mrr: overall_mrr,
        process_recall: None,
        routing_accuracy: None,
        per_query,
        by_category,
    })
}

// ─── Grouped eval helpers ─────────────────────────────────────────

struct RoutingMetrics {
    process_recall: Option<f64>,
    routing_accuracy: Option<f64>,
}

fn flatten_grouped(grouped: &GroupedQueryResult) -> Vec<SearchResult> {
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let mut flat: Vec<SearchResult> = Vec::new();
    for pg in &grouped.processes {
        for ps in &pg.matched_symbols {
            let key = (ps.symbol.name.clone(), ps.symbol.file_path.clone(), ps.symbol.kind.clone());
            if seen.insert(key) {
                flat.push(SearchResult { symbol: ps.symbol.clone(), score: ps.score });
            }
        }
    }
    for sr in &grouped.definitions {
        let key = (sr.symbol.name.clone(), sr.symbol.file_path.clone(), sr.symbol.kind.clone());
        if seen.insert(key) {
            flat.push(sr.clone());
        }
    }
    flat
}

fn compute_routing_metrics(grouped: &GroupedQueryResult, relevant: &[RelevantSymbol]) -> RoutingMetrics {
    let annotated: Vec<(&RelevantSymbol, bool)> = relevant.iter()
        .filter_map(|r| r.in_process.map(|v| (r, v)))
        .collect();

    if annotated.is_empty() {
        return RoutingMetrics { process_recall: None, routing_accuracy: None };
    }

    let process_set: HashSet<(&str, &str, &str)> = grouped.processes.iter()
        .flat_map(|pg| &pg.matched_symbols)
        .map(|ps| (ps.symbol.name.as_str(), ps.symbol.file_path.as_str(), ps.symbol.kind.as_str()))
        .collect();

    let definition_set: HashSet<(&str, &str, &str)> = grouped.definitions.iter()
        .map(|sr| (sr.symbol.name.as_str(), sr.symbol.file_path.as_str(), sr.symbol.kind.as_str()))
        .collect();

    let in_proc_relevant: Vec<&&RelevantSymbol> = annotated.iter()
        .filter(|(_, v)| *v)
        .map(|(r, _)| r)
        .collect();

    let process_recall = if in_proc_relevant.is_empty() {
        None
    } else {
        let hits = in_proc_relevant.iter()
            .filter(|r| process_set.contains(&(r.name.as_str(), r.file_path.as_str(), r.kind.as_str())))
            .count();
        Some(hits as f64 / in_proc_relevant.len() as f64)
    };

    // routing_accuracy: in_process=true → must appear in process_set
    //                   in_process=false → must appear in definition_set (absent = wrong)
    let correct = annotated.iter().filter(|(r, expected_in_process)| {
        let key = (r.name.as_str(), r.file_path.as_str(), r.kind.as_str());
        if *expected_in_process {
            process_set.contains(&key)
        } else {
            definition_set.contains(&key)
        }
    }).count();
    let routing_accuracy = Some(correct as f64 / annotated.len() as f64);

    RoutingMetrics { process_recall, routing_accuracy }
}

fn avg_option(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

pub fn eval_grouped(engine: &QueryEngine, queries: &[EvalQuery], k: usize) -> Result<EvalReport> {
    let mode_label = EvalMode::Grouped.label().to_string();
    let mut per_query: Vec<QueryResult> = Vec::new();

    for eq in queries {
        // Fetch with incremental doubling until we have ≥k unique results or hit the cap.
        // This avoids under-evaluation when many results overlap between processes and definitions.
        const MAX_LIMIT: usize = 1000;
        let (grouped, flat) = {
            let mut limit = k * 3;
            loop {
                let g = engine.search_grouped(&eq.query, limit)?;
                let f = flatten_grouped(&g);
                if f.len() >= k || limit >= MAX_LIMIT {
                    break (g, f);
                }
                limit = (limit * 2).min(MAX_LIMIT);
            }
        };

        let recall = recall_at_k(&flat, &eq.relevant, k);
        let rr = reciprocal_rank(&flat, &eq.relevant, k);
        let hit_count = flat.iter().take(k).filter(|r| is_hit(r, &eq.relevant)).count();
        let first_relevant_rank = flat.iter().take(k).enumerate()
            .find(|(_, r)| is_hit(r, &eq.relevant))
            .map(|(i, _)| i + 1);

        let metrics = compute_routing_metrics(&grouped, &eq.relevant);

        per_query.push(QueryResult {
            id: eq.id.clone(),
            query: eq.query.clone(),
            category: eq.category.label().to_string(),
            relevant_count: eq.relevant.len(),
            hit_count,
            first_relevant_rank,
            recall,
            reciprocal_rank: rr,
            process_recall: metrics.process_recall,
            routing_accuracy: metrics.routing_accuracy,
        });
    }

    let overall_recall = if per_query.is_empty() {
        0.0
    } else {
        per_query.iter().map(|q| q.recall).sum::<f64>() / per_query.len() as f64
    };
    let overall_mrr = if per_query.is_empty() {
        0.0
    } else {
        per_query.iter().map(|q| q.reciprocal_rank).sum::<f64>() / per_query.len() as f64
    };
    let overall_process_recall = avg_option(&per_query.iter().filter_map(|q| q.process_recall).collect::<Vec<_>>());
    let overall_routing_accuracy = avg_option(&per_query.iter().filter_map(|q| q.routing_accuracy).collect::<Vec<_>>());

    let by_category = compute_category_breakdown(&per_query, queries);

    Ok(EvalReport {
        fixture: String::new(),
        mode: mode_label,
        k,
        recall_at_k: overall_recall,
        mrr: overall_mrr,
        process_recall: overall_process_recall,
        routing_accuracy: overall_routing_accuracy,
        per_query,
        by_category,
    })
}

fn compute_category_breakdown(
    results: &[QueryResult],
    queries: &[EvalQuery],
) -> CategoryBreakdown {
    let mut groups: HashMap<QueryCategory, Vec<(f64, f64)>> = HashMap::new();

    for (qr, eq) in results.iter().zip(queries.iter()) {
        groups
            .entry(eq.category.clone())
            .or_default()
            .push((qr.recall, qr.reciprocal_rank));
    }

    let to_metrics = |cat: &QueryCategory| -> Option<CategoryMetrics> {
        let pairs = groups.get(cat)?;
        let count = pairs.len();
        let recall = pairs.iter().map(|(r, _)| r).sum::<f64>() / count as f64;
        let mrr = pairs.iter().map(|(_, rr)| rr).sum::<f64>() / count as f64;
        Some(CategoryMetrics { count, recall_at_k: recall, mrr })
    };

    CategoryBreakdown {
        exact: to_metrics(&QueryCategory::Exact),
        semantic: to_metrics(&QueryCategory::Semantic),
        mixed: to_metrics(&QueryCategory::Mixed),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::SymbolInfo;

    fn make_result(name: &str, file_path: &str, kind: &str) -> SearchResult {
        SearchResult {
            symbol: SymbolInfo {
                uid: format!("{}:{}:{}:1", kind, file_path, name),
                name: name.to_string(),
                kind: kind.to_string(),
                file_path: file_path.to_string(),
                start_line: 1,
                end_line: 10,
                is_exported: true,
                parent_name: None,
            },
            score: 1.0,
        }
    }

    fn make_relevant(name: &str, file_path: &str, kind: &str) -> RelevantSymbol {
        RelevantSymbol {
            name: name.to_string(),
            file_path: file_path.to_string(),
            kind: kind.to_string(),
            in_process: None,
        }
    }

    #[test]
    fn recall_all_hits() {
        let results = vec![
            make_result("Foo", "a.ts", "Class"),
            make_result("Bar", "b.ts", "Function"),
        ];
        let relevant = vec![
            make_relevant("Foo", "a.ts", "Class"),
            make_relevant("Bar", "b.ts", "Function"),
        ];
        let r = recall_at_k(&results, &relevant, 5);
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn recall_partial_hits() {
        let results = vec![
            make_result("Foo", "a.ts", "Class"),
            make_result("Baz", "c.ts", "Method"),
        ];
        let relevant = vec![
            make_relevant("Foo", "a.ts", "Class"),
            make_relevant("Bar", "b.ts", "Function"),
        ];
        let r = recall_at_k(&results, &relevant, 5);
        assert!((r - 0.5).abs() < 1e-9);
    }

    #[test]
    fn recall_no_hits() {
        let results = vec![make_result("Baz", "c.ts", "Method")];
        let relevant = vec![make_relevant("Foo", "a.ts", "Class")];
        let r = recall_at_k(&results, &relevant, 5);
        assert!(r.abs() < 1e-9);
    }

    #[test]
    fn rr_first_position() {
        let results = vec![make_result("Foo", "a.ts", "Class")];
        let relevant = vec![make_relevant("Foo", "a.ts", "Class")];
        let rr = reciprocal_rank(&results, &relevant, 5);
        assert!((rr - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rr_second_position() {
        let results = vec![
            make_result("Other", "x.ts", "Method"),
            make_result("Foo", "a.ts", "Class"),
        ];
        let relevant = vec![make_relevant("Foo", "a.ts", "Class")];
        let rr = reciprocal_rank(&results, &relevant, 5);
        assert!((rr - 0.5).abs() < 1e-9);
    }

    #[test]
    fn rr_no_hit() {
        let results = vec![make_result("Other", "x.ts", "Method")];
        let relevant = vec![make_relevant("Foo", "a.ts", "Class")];
        let rr = reciprocal_rank(&results, &relevant, 5);
        assert!(rr.abs() < 1e-9);
    }

    #[test]
    fn hit_requires_all_three_fields() {
        // same name and kind but different file_path — not a hit
        let result = make_result("Foo", "other/a.ts", "Class");
        let relevant = vec![make_relevant("Foo", "a.ts", "Class")];
        assert!(!is_hit(&result, &relevant));
    }

    // ─── compute_routing_metrics tests ───────────────────────────────

    use crate::query::{GroupedQueryResult, ProcessGroup, ProcessedSymbol};

    fn make_grouped(process_names: &[(&str, &str, &str)], definition_names: &[(&str, &str, &str)]) -> GroupedQueryResult {
        let matched_symbols = process_names.iter().map(|(name, file, kind)| {
            ProcessedSymbol {
                symbol: SymbolInfo {
                    uid: format!("{}:{}:{}:1", kind, file, name),
                    name: name.to_string(),
                    kind: kind.to_string(),
                    file_path: file.to_string(),
                    start_line: 1,
                    end_line: 10,
                    is_exported: true,
                    parent_name: None,
                },
                score: 1.0,
                step_index: 0,
            }
        }).collect();

        let definitions = definition_names.iter().map(|(name, file, kind)| {
            make_result(name, file, kind)
        }).collect();

        GroupedQueryResult {
            processes: vec![ProcessGroup {
                id: 1,
                label: "test".to_string(),
                process_type: "execution".to_string(),
                matched_symbols,
            }],
            definitions,
            total: process_names.len() + definition_names.len(),
        }
    }

    #[test]
    fn routing_accuracy_absent_false_entry_is_wrong() {
        // in_process=false エントリが definitions に存在しない（absent）場合は不正解
        let grouped = make_grouped(&[("Foo", "a.ts", "Function")], &[]);
        // Bar は in_process=false だが definitions に現れていない → 不正解
        let relevant = vec![
            RelevantSymbol { name: "Foo".into(), file_path: "a.ts".into(), kind: "Function".into(), in_process: Some(true) },
            RelevantSymbol { name: "Bar".into(), file_path: "b.ts".into(), kind: "Function".into(), in_process: Some(false) },
        ];
        let m = compute_routing_metrics(&grouped, &relevant);
        // Foo: process_set に存在 → 正解, Bar: definition_set に存在しない → 不正解
        // routing_accuracy = 1/2 = 0.5
        assert!((m.routing_accuracy.unwrap() - 0.5).abs() < 1e-9,
            "absent in_process=false should be wrong, got {:?}", m.routing_accuracy);
    }

    #[test]
    fn routing_accuracy_false_entry_in_definitions_is_correct() {
        // in_process=false エントリが definitions に存在する場合は正解
        let grouped = make_grouped(&[("Foo", "a.ts", "Function")], &[("Bar", "b.ts", "Function")]);
        let relevant = vec![
            RelevantSymbol { name: "Foo".into(), file_path: "a.ts".into(), kind: "Function".into(), in_process: Some(true) },
            RelevantSymbol { name: "Bar".into(), file_path: "b.ts".into(), kind: "Function".into(), in_process: Some(false) },
        ];
        let m = compute_routing_metrics(&grouped, &relevant);
        assert!((m.routing_accuracy.unwrap() - 1.0).abs() < 1e-9,
            "both correctly routed, got {:?}", m.routing_accuracy);
    }

    #[test]
    fn routing_accuracy_no_annotations_returns_none() {
        let grouped = make_grouped(&[], &[]);
        let relevant = vec![make_relevant("Foo", "a.ts", "Function")];
        let m = compute_routing_metrics(&grouped, &relevant);
        assert!(m.process_recall.is_none());
        assert!(m.routing_accuracy.is_none());
    }
}
