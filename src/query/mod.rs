use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::embedder::{Embedder, DIMS, MODEL_ID};

/// f64 wrapper with total ordering (for BinaryHeap).
#[derive(Clone, Copy, PartialEq)]
struct FloatOrd(f64);

impl Eq for FloatOrd {}

impl PartialOrd for FloatOrd {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FloatOrd {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

// ─── Data Types ─────────────────────────────────────────────────────

/// Valid symbol kind strings stored in the DB.
pub const VALID_SYMBOL_KINDS: &[&str] = &[
    "Function", "Method", "Class", "Interface", "Type", "Enum",
    "Struct", "Field", "Property", "Variable", "Constant", "Constructor", "Module",
    "External",
];

#[derive(Debug, Serialize)]
pub struct SubgraphNode {
    pub id: i64,
    #[serde(flatten)]
    pub symbol: SymbolInfo,
}

#[derive(Debug, Serialize)]
pub struct EdgeEntry {
    pub source_id: i64,
    pub target_id: i64,
    pub kind: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SubgraphResult {
    pub start_id: i64,
    pub nodes: Vec<SubgraphNode>,
    pub edges: Vec<EdgeEntry>,
    pub node_count: usize,
    pub edge_count: usize,
    pub truncated: bool,
    pub truncated_reason: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SymbolInfo {
    pub uid: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub is_exported: bool,
    pub parent_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RelationInfo {
    pub symbol: SymbolInfo,
    pub kind: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SymbolContext {
    pub symbol: SymbolInfo,
    pub incoming: Vec<RelationInfo>,
    pub outgoing: Vec<RelationInfo>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum ContextResponse {
    #[serde(rename = "found")]
    Found {
        symbol: SymbolInfo,
        incoming: Vec<RelationInfo>,
        outgoing: Vec<RelationInfo>,
    },
    #[serde(rename = "ambiguous")]
    Ambiguous {
        message: String,
        candidates: Vec<SymbolInfo>,
    },
    #[serde(rename = "not_found")]
    NotFound {
        message: String,
    },
}

#[derive(Debug, Serialize, Clone)]
pub struct SearchResult {
    pub symbol: SymbolInfo,
    pub score: f64,
}

#[derive(Debug, Serialize)]
pub struct ProcessedSymbol {
    pub symbol: SymbolInfo,
    pub score: f64,
    pub step_index: i64,
}

#[derive(Debug, Serialize)]
pub struct ProcessGroup {
    pub id: i64,
    pub label: String,
    pub process_type: String,
    pub matched_symbols: Vec<ProcessedSymbol>,
}

#[derive(Debug, Serialize)]
pub struct GroupedQueryResult {
    pub processes: Vec<ProcessGroup>,
    pub definitions: Vec<SearchResult>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ImpactEntry {
    pub symbol: SymbolInfo,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct AffectedProcess {
    pub id: i64,
    pub label: String,
    pub process_type: String,
}

#[derive(Debug, Serialize)]
pub struct AffectedModule {
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct ImpactResult {
    pub target: SymbolInfo,
    pub direction: String,
    pub by_depth: BTreeMap<u32, Vec<ImpactEntry>>,
    pub total_affected: usize,
    pub risk: String,
    pub summary: String,
    pub affected_processes: Vec<AffectedProcess>,
    pub affected_modules: Vec<AffectedModule>,
}

#[derive(Debug, Serialize)]
pub struct ChangedSymbolEntry {
    pub file: String,
    pub symbol: SymbolInfo,
    pub impact: Option<ImpactResult>,
}

#[derive(Debug, Serialize)]
pub struct ImpactBatchResult {
    pub results: Vec<ChangedSymbolEntry>,
    pub total: usize,
    pub truncated: bool,
}

// ─── Query Engine ───────────────────────────────────────────────────

pub struct QueryEngine<'a> {
    conn: &'a Connection,
}

impl<'a> QueryEngine<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Hybrid search: BM25 top-50 union Vector top-50, merged with RRF.
    pub fn search_hybrid(
        &self,
        query: &str,
        limit: usize,
        embedder: &Embedder,
    ) -> Result<Vec<SearchResult>> {
        const BM25_K: usize = 50;
        const VEC_K: usize = 50;
        const RRF_K: usize = 60;

        let bm25_results = self.search_with_id(query, BM25_K)?;
        let query_vec = embedder.embed_query(query)?;
        let vec_results = self.search_vector_top_n(&query_vec, VEC_K)?;

        if bm25_results.is_empty() && vec_results.is_empty() {
            return Ok(vec![]);
        }

        if bm25_results.is_empty() {
            return Ok(vec_results
                .into_iter()
                .take(limit)
                .map(|(_id, (sym, score))| SearchResult { symbol: sym, score })
                .collect());
        }

        // Build rank maps (symbol id → 0-based rank)
        let bm25_rank: HashMap<i64, usize> = bm25_results
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i))
            .collect();
        let vec_rank: HashMap<i64, usize> = vec_results
            .iter()
            .enumerate()
            .map(|(i, (sym_id, _))| (*sym_id, i))
            .collect();

        // Union of all symbol ids
        let mut all_ids: Vec<i64> = bm25_results.iter().map(|(id, _)| *id).collect();
        for (sym_id, _) in &vec_results {
            if !bm25_rank.contains_key(sym_id) {
                all_ids.push(*sym_id);
            }
        }

        // Collect symbol info from both lists
        let bm25_map: HashMap<i64, SymbolInfo> = bm25_results
            .into_iter()
            .map(|(id, r)| (id, r.symbol))
            .collect();
        let vec_map: HashMap<i64, SymbolInfo> = vec_results
            .into_iter()
            .map(|(id, (sym, _))| (id, sym))
            .collect();

        let mut scored: Vec<(i64, f64)> = all_ids
            .into_iter()
            .map(|id| {
                let score = rrf_score(bm25_rank.get(&id).copied(), vec_rank.get(&id).copied(), RRF_K);
                (id, score)
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        scored.truncate(limit);

        let results = scored
            .into_iter()
            .filter_map(|(id, score)| {
                let sym = bm25_map.get(&id).or_else(|| vec_map.get(&id))?;
                Some(SearchResult {
                    symbol: sym.clone(),
                    score,
                })
            })
            .collect();

        Ok(results)
    }

    /// Vector-only search: cosine similarity over all embeddings.
    pub fn search_vector_only(
        &self,
        query: &str,
        limit: usize,
        embedder: &Embedder,
    ) -> Result<Vec<SearchResult>> {
        let query_vec = embedder.embed_query(query)?;
        let results = self.search_vector_top_n(&query_vec, limit)?;
        Ok(results
            .into_iter()
            .map(|(_id, (sym, score))| SearchResult { symbol: sym, score })
            .collect())
    }

    /// Returns true if at least one embedding exists for the current model.
    pub fn has_embeddings(&self) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM embeddings WHERE model_id = ? AND dims = ?",
            params![MODEL_ID, DIMS as i64],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Full-text search using FTS5 BM25 ranking.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.uid, s.name, s.kind, s.file_path, s.start_line, s.end_line,
                    s.is_exported, s.parent_name,
                    rank
             FROM symbols_fts fts
             JOIN symbols s ON s.id = fts.rowid
             WHERE symbols_fts MATCH ?1
               AND s.kind NOT IN ('File', 'Folder', 'External')
             ORDER BY rank, s.id ASC
             LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![query, limit as i64], |row| {
                Ok(SearchResult {
                    symbol: symbol_info_from_row(row)?,
                    score: row.get::<_, f64>("rank")?.abs(), // FTS5 rank is negative
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// BM25 search grouped by execution process.
    /// Symbols that belong to one or more processes appear in `processes`;
    /// symbols not in any process appear in `definitions`.
    pub fn search_grouped(&self, query: &str, limit: usize) -> Result<GroupedQueryResult> {
        let id_results = self.search_with_id(query, limit)?;
        let total = id_results.len();

        if id_results.is_empty() {
            return Ok(GroupedQueryResult {
                processes: vec![],
                definitions: vec![],
                total: 0,
            });
        }

        // Build lookup: symbol_id → (SearchResult, score)
        let mut result_map: HashMap<i64, SearchResult> = HashMap::new();
        let mut id_order: Vec<i64> = Vec::with_capacity(id_results.len());
        for (id, sr) in id_results {
            id_order.push(id);
            result_map.insert(id, sr);
        }

        // Query process membership for all found symbol ids
        let id_list: String = id_order.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT ps.symbol_id, p.id, p.label, p.process_type, ps.step_index
             FROM process_steps ps
             JOIN processes p ON p.id = ps.process_id
             WHERE ps.symbol_id IN ({})
             ORDER BY p.id ASC, ps.step_index ASC, ps.symbol_id ASC",
            id_list
        );

        // process_id → ProcessGroup (built incrementally)
        let mut process_map: HashMap<i64, ProcessGroup> = HashMap::new();
        // track which process_ids we've seen in order for stable output
        let mut process_order: Vec<i64> = Vec::new();
        // set of symbol_ids that appear in at least one process
        let mut in_process: HashSet<i64> = HashSet::new();

        {
            let mut stmt = self.conn.prepare(&sql)?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let symbol_id: i64 = row.get(0)?;
                let process_id: i64 = row.get(1)?;
                let label: String = row.get(2)?;
                let process_type: String = row.get(3)?;
                let step_index: i64 = row.get(4)?;

                in_process.insert(symbol_id);

                if !process_map.contains_key(&process_id) {
                    process_order.push(process_id);
                    process_map.insert(process_id, ProcessGroup {
                        id: process_id,
                        label,
                        process_type,
                        matched_symbols: vec![],
                    });
                }

                if let Some(sr) = result_map.get(&symbol_id) {
                    let pg = process_map.get_mut(&process_id).unwrap();
                    pg.matched_symbols.push(ProcessedSymbol {
                        symbol: sr.symbol.clone(),
                        score: sr.score,
                        step_index,
                    });
                }
            }
        }

        // Build processes vec in process_id ASC order (process_order is already ASC from SQL)
        let processes: Vec<ProcessGroup> = process_order
            .into_iter()
            .filter_map(|pid| process_map.remove(&pid))
            .collect();

        // Definitions: symbols not in any process, score DESC, id ASC
        let mut definitions: Vec<(i64, SearchResult)> = id_order
            .into_iter()
            .filter(|id| !in_process.contains(id))
            .filter_map(|id| result_map.remove(&id).map(|sr| (id, sr)))
            .collect();
        definitions.sort_by(|(id_a, sr_a), (id_b, sr_b)| {
            sr_b.score.partial_cmp(&sr_a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(id_a.cmp(id_b))
        });
        let definitions: Vec<SearchResult> = definitions.into_iter().map(|(_, sr)| sr).collect();

        Ok(GroupedQueryResult {
            processes,
            definitions,
            total,
        })
    }

    /// 360-degree context for a symbol: all incoming and outgoing relationships.
    pub fn context(&self, symbol_name: &str) -> Result<Option<SymbolContext>> {
        // Find the symbol
        let symbol = self.find_symbol(symbol_name)?;
        let Some(symbol) = symbol else {
            return Ok(None);
        };

        let symbol_id = self.get_symbol_id(&symbol.name, &symbol.file_path)?;
        let Some(symbol_id) = symbol_id else {
            return Ok(None);
        };

        // Incoming: other symbols → this symbol
        let incoming = self.get_relations_to(symbol_id)?;

        // Outgoing: this symbol → other symbols
        let outgoing = self.get_relations_from(symbol_id)?;

        Ok(Some(SymbolContext {
            symbol,
            incoming,
            outgoing,
        }))
    }

    /// Blast radius analysis: BFS from a symbol, grouped by depth.
    pub fn impact(
        &self,
        symbol_name: &str,
        direction: &str,
        max_depth: u32,
        min_confidence: f64,
        calls_only: bool,
    ) -> Result<Option<ImpactResult>> {
        let symbol = self.find_symbol(symbol_name)?;
        let Some(symbol) = symbol else {
            return Ok(None);
        };

        let start_id = self.get_symbol_id(&symbol.name, &symbol.file_path)?;
        let Some(start_id) = start_id else {
            return Ok(None);
        };

        self.impact_by_id(start_id, direction, max_depth, min_confidence, calls_only)
    }

    /// Blast radius analysis by symbol id (skips name resolution).
    pub fn impact_by_id(
        &self,
        symbol_id: i64,
        direction: &str,
        max_depth: u32,
        min_confidence: f64,
        calls_only: bool,
    ) -> Result<Option<ImpactResult>> {
        let target = self.get_symbol_by_id(symbol_id)?;
        let Some(target) = target else {
            return Ok(None);
        };

        let upstream = direction == "upstream";
        let mut by_depth: BTreeMap<u32, Vec<ImpactEntry>> = BTreeMap::new();
        let mut visited: HashSet<i64> = HashSet::new();
        visited.insert(symbol_id);

        let mut queue: VecDeque<(i64, u32)> = VecDeque::new();
        queue.push_back((symbol_id, 0));

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let neighbors = if upstream {
                self.get_callers(current_id, min_confidence, calls_only)?
            } else {
                self.get_callees(current_id, min_confidence, calls_only)?
            };

            for (neighbor_id, confidence, reason) in neighbors {
                if visited.contains(&neighbor_id) {
                    continue;
                }
                visited.insert(neighbor_id);

                let sym = self.get_symbol_by_id(neighbor_id)?;
                if let Some(sym) = sym {
                    by_depth
                        .entry(depth + 1)
                        .or_default()
                        .push(ImpactEntry {
                            symbol: sym,
                            confidence,
                            reason,
                        });
                    queue.push_back((neighbor_id, depth + 1));
                }
            }
        }

        let total_affected: usize = by_depth.values().map(|v| v.len()).sum();

        let mut affected_ids: Vec<i64> = visited
            .iter()
            .filter(|&&id| id != symbol_id)
            .copied()
            .collect();
        affected_ids.sort();

        let depth_1_count = by_depth.get(&1).map(|v| v.len()).unwrap_or(0);
        let has_exported_at_d1 = by_depth
            .get(&1)
            .map(|entries| entries.iter().any(|e| e.symbol.is_exported))
            .unwrap_or(false);
        let risk = compute_risk(has_exported_at_d1, total_affected, depth_1_count);
        let affected_processes = self.find_processes_for_symbols(&affected_ids)?;
        let affected_modules = self.find_communities_for_symbols(&affected_ids)?;
        let summary = build_impact_summary(
            &target.name,
            direction,
            total_affected,
            depth_1_count,
            &risk,
            affected_modules.len(),
            affected_processes.len(),
        );

        Ok(Some(ImpactResult {
            target,
            direction: direction.to_string(),
            by_depth,
            total_affected,
            risk,
            summary,
            affected_processes,
            affected_modules,
        }))
    }

    /// Symbols whose line range overlaps [range_start, range_end] in a given file.
    pub fn symbols_in_range(
        &self,
        file_path: &str,
        range_start: i64,
        range_end: i64,
    ) -> Result<Vec<(i64, SymbolInfo)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
             FROM symbols
             WHERE file_path = ?1 AND start_line <= ?2 AND end_line >= ?3
             ORDER BY start_line, end_line, id",
        )?;

        let results = stmt
            .query_map(params![file_path, range_end, range_start], |row| {
                Ok((row.get::<_, i64>(0)?, symbol_info_from_row(row)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    // ─── Internal helpers ───────────────────────────────────────────

    /// BM25 search that also returns the symbol row-id.
    fn search_with_id(&self, query: &str, limit: usize) -> Result<Vec<(i64, SearchResult)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.uid, s.name, s.kind, s.file_path, s.start_line, s.end_line,
                    s.is_exported, s.parent_name, rank
             FROM symbols_fts fts
             JOIN symbols s ON s.id = fts.rowid
             WHERE symbols_fts MATCH ?1
               AND s.kind NOT IN ('File', 'Folder', 'External')
             ORDER BY rank, s.id ASC
             LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![query, limit as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    SearchResult {
                        symbol: symbol_info_from_row(row)?,
                        score: row.get::<_, f64>("rank")?.abs(),
                    },
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Load all embeddings from DB filtered by current model. Returns (symbol_id, vector).
    #[allow(dead_code)]
    fn load_all_embeddings(&self) -> Result<Vec<(i64, Vec<f32>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT symbol_id, vector_blob, dims FROM embeddings WHERE model_id = ? AND dims = ?",
        )?;

        let mut out = Vec::new();
        let mut rows = stmt.query(params![MODEL_ID, DIMS as i64])?;
        while let Some(row) = rows.next()? {
            let symbol_id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let dims: i64 = row.get(2)?;

            if blob.len() % 4 != 0 {
                eprintln!("warn: blob for symbol {} has invalid length, skipping", symbol_id);
                continue;
            }
            let decoded_len = blob.len() / 4;
            if decoded_len != dims as usize {
                eprintln!("warn: blob dims mismatch for symbol {}, skipping", symbol_id);
                continue;
            }
            if dims as usize != DIMS {
                eprintln!("warn: unexpected dims {} for symbol {}, skipping", dims, symbol_id);
                continue;
            }
            let vec: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            out.push((symbol_id, vec));
        }
        Ok(out)
    }

    /// Load embeddings for specific symbol ids. Returns (symbol_id, vector).
    #[allow(dead_code)]
    fn load_embeddings_for_ids(&self, ids: &[i64]) -> Result<Vec<(i64, Vec<f32>)>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        // ids are i64 — safe to interpolate directly
        let id_list: String = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT symbol_id, vector_blob, dims FROM embeddings WHERE symbol_id IN ({})",
            id_list
        );
        let mut stmt = self.conn.prepare(&sql)?;

        let mut out = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let symbol_id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let dims: i64 = row.get(2)?;

            if blob.len() % 4 != 0 {
                eprintln!("warn: blob for symbol {} has invalid length, skipping", symbol_id);
                continue;
            }
            let decoded_len = blob.len() / 4;
            if decoded_len != dims as usize {
                eprintln!("warn: blob dims mismatch for symbol {}, skipping", symbol_id);
                continue;
            }
            if dims as usize != DIMS {
                eprintln!("warn: unexpected dims {} for symbol {}, skipping", dims, symbol_id);
                continue;
            }
            let vec: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            out.push((symbol_id, vec));
        }
        Ok(out)
    }

    /// Cosine similarity over all embeddings, returns (symbol_id, (SymbolInfo, score)) top-n.
    /// Uses a BinaryHeap to keep only top-n in memory instead of loading all embeddings.
    fn search_vector_top_n(
        &self,
        query_vec: &[f32],
        n: usize,
    ) -> Result<Vec<(i64, (SymbolInfo, f64))>> {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;

        let mut stmt = self.conn.prepare(
            "SELECT symbol_id, vector_blob, dims FROM embeddings WHERE model_id = ? AND dims = ?",
        )?;

        // Min-heap of (score, id) — keeps top-n by evicting the smallest score
        let mut heap: BinaryHeap<Reverse<(FloatOrd, i64)>> = BinaryHeap::new();

        let mut rows = stmt.query(params![MODEL_ID, DIMS as i64])?;
        while let Some(row) = rows.next()? {
            let symbol_id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let dims: i64 = row.get(2)?;

            if blob.len() % 4 != 0 || blob.len() / 4 != dims as usize || dims as usize != DIMS {
                continue;
            }
            let vec: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();

            let score = cosine_similarity(query_vec, &vec);
            heap.push(Reverse((FloatOrd(score), symbol_id)));
            if heap.len() > n {
                heap.pop(); // drop smallest
            }
        }

        if heap.is_empty() {
            return Ok(vec![]);
        }

        // Extract top-n sorted by (score DESC, id ASC)
        let mut top_n: Vec<(i64, f64)> = heap
            .into_iter()
            .map(|Reverse((FloatOrd(score), id))| (id, score))
            .collect();
        top_n.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

        let ids: Vec<i64> = top_n.iter().map(|(id, _)| *id).collect();
        let score_map: HashMap<i64, f64> = top_n.into_iter().collect();

        let mut sym_map = self.get_symbols_by_ids(&ids)?;
        let mut results: Vec<(i64, (SymbolInfo, f64))> = ids
            .iter()
            .filter_map(|&id| {
                let sym = sym_map.remove(&id)?;
                let score = *score_map.get(&id)?;
                Some((id, (sym, score)))
            })
            .collect();
        results.sort_by(|a, b| {
            b.1.1.partial_cmp(&a.1.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

        Ok(results)
    }

    fn get_symbols_by_ids(&self, ids: &[i64]) -> Result<HashMap<i64, SymbolInfo>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let id_list: String = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
             FROM symbols WHERE id IN ({})",
            id_list
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut map = HashMap::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            map.insert(id, symbol_info_from_row(row)?);
        }
        Ok(map)
    }

    fn find_symbol(&self, name: &str) -> Result<Option<SymbolInfo>> {
        // Try exact name match first, prefer exported
        let mut stmt = self.conn.prepare(
            "SELECT uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
             FROM symbols
             WHERE name = ?1
             ORDER BY is_exported DESC, id ASC
             LIMIT 1",
        )?;

        let result = stmt
            .query_row(params![name], |row| symbol_info_from_row(row))
            .optional()?;

        Ok(result)
    }

    /// Resolve a symbol by exact name + file_path match.
    /// Returns None if not found, Err if ambiguous (multiple matches).
    pub fn symbol_by_name_file(&self, name: &str, file_path: &str) -> Result<Option<(i64, SymbolInfo)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
             FROM symbols WHERE name = ?1 AND file_path = ?2
             ORDER BY id LIMIT 2",
        )?;
        let mut rows = stmt.query_map(params![name, file_path], |row| {
            Ok((row.get::<_, i64>(0)?, symbol_info_from_row(row)?))
        })?;
        let first = rows.next().transpose()?;
        let second = rows.next().transpose()?;
        if second.is_some() {
            anyhow::bail!("ambiguous symbol: '{}' in '{}' matches multiple entries", name, file_path);
        }
        Ok(first)
    }

    fn get_symbol_id(&self, name: &str, file_path: &str) -> Result<Option<i64>> {
        let result = self
            .conn
            .query_row(
                "SELECT id FROM symbols WHERE name = ?1 AND file_path = ?2 LIMIT 1",
                params![name, file_path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result)
    }

    pub fn get_symbol_by_id_pub(&self, id: i64) -> Result<Option<SymbolInfo>> {
        self.get_symbol_by_id(id)
    }

    fn get_symbol_by_id(&self, id: i64) -> Result<Option<SymbolInfo>> {
        let result = self
            .conn
            .query_row(
                "SELECT uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
                 FROM symbols WHERE id = ?1",
                params![id],
                |row| symbol_info_from_row(row),
            )
            .optional()?;
        Ok(result)
    }

    fn get_relations_to(&self, target_id: i64) -> Result<Vec<RelationInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.uid, s.name, s.kind, s.file_path, s.start_line, s.end_line,
                    s.is_exported, s.parent_name,
                    r.kind AS rel_kind, r.confidence, r.reason
             FROM relationships r
             JOIN symbols s ON s.id = r.source_id
             WHERE r.target_id = ?1
             ORDER BY r.confidence DESC, r.source_id ASC",
        )?;

        let results = stmt
            .query_map(params![target_id], |row| {
                Ok(RelationInfo {
                    symbol: symbol_info_from_row(row)?,
                    kind: row.get("rel_kind")?,
                    confidence: row.get("confidence")?,
                    reason: row.get("reason")?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    fn get_relations_from(&self, source_id: i64) -> Result<Vec<RelationInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.uid, s.name, s.kind, s.file_path, s.start_line, s.end_line,
                    s.is_exported, s.parent_name,
                    r.kind AS rel_kind, r.confidence, r.reason
             FROM relationships r
             JOIN symbols s ON s.id = r.target_id
             WHERE r.source_id = ?1
             ORDER BY r.confidence DESC, r.target_id ASC",
        )?;

        let results = stmt
            .query_map(params![source_id], |row| {
                Ok(RelationInfo {
                    symbol: symbol_info_from_row(row)?,
                    kind: row.get("rel_kind")?,
                    confidence: row.get("confidence")?,
                    reason: row.get("reason")?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    fn get_callers(
        &self,
        target_id: i64,
        min_confidence: f64,
        calls_only: bool,
    ) -> Result<Vec<(i64, f64, String)>> {
        let sql = if calls_only {
            "SELECT r.source_id, r.confidence, r.reason
             FROM relationships r
             WHERE r.target_id = ?1 AND r.confidence >= ?2 AND r.kind = 'CALLS'
             ORDER BY r.source_id, r.confidence DESC, r.kind ASC"
        } else {
            "SELECT r.source_id, r.confidence, r.reason
             FROM relationships r
             WHERE r.target_id = ?1 AND r.confidence >= ?2
             ORDER BY r.source_id, r.confidence DESC, r.kind ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let results = stmt
            .query_map(params![target_id, min_confidence], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(results)
    }

    fn get_callees(
        &self,
        source_id: i64,
        min_confidence: f64,
        calls_only: bool,
    ) -> Result<Vec<(i64, f64, String)>> {
        let sql = if calls_only {
            "SELECT r.target_id, r.confidence, r.reason
             FROM relationships r
             WHERE r.source_id = ?1 AND r.confidence >= ?2 AND r.kind = 'CALLS'
             ORDER BY r.target_id, r.confidence DESC, r.kind ASC"
        } else {
            "SELECT r.target_id, r.confidence, r.reason
             FROM relationships r
             WHERE r.source_id = ?1 AND r.confidence >= ?2
             ORDER BY r.target_id, r.confidence DESC, r.kind ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let results = stmt
            .query_map(params![source_id, min_confidence], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(results)
    }

    fn find_processes_for_symbols(&self, ids: &[i64]) -> Result<Vec<AffectedProcess>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_list: String = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT p.id, p.label, p.process_type
             FROM processes p
             JOIN process_steps ps ON ps.process_id = p.id
             WHERE ps.symbol_id IN ({})
             ORDER BY p.id ASC",
            id_list
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut results = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            results.push(AffectedProcess {
                id: row.get(0)?,
                label: row.get(1)?,
                process_type: row.get(2)?,
            });
        }
        Ok(results)
    }

    fn find_communities_for_symbols(&self, ids: &[i64]) -> Result<Vec<AffectedModule>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_list: String = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT DISTINCT c.id, c.label
             FROM communities c
             JOIN community_members cm ON cm.community_id = c.id
             WHERE cm.symbol_id IN ({})
             ORDER BY c.id ASC",
            id_list
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut results = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            results.push(AffectedModule {
                id: row.get(0)?,
                label: row.get(1)?,
            });
        }
        Ok(results)
    }

    // ─── Context resolution (P5.1) ──────────────────────────────────

    /// Returns all symbols matching name, ordered by is_exported DESC, id ASC.
    pub fn find_symbols_by_name(&self, name: &str, limit: usize) -> Result<Vec<SymbolInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
             FROM symbols WHERE name = ?1
             ORDER BY is_exported DESC, id ASC
             LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![name, limit as i64], |row| symbol_info_from_row(row))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(results)
    }

    /// Returns all symbols matching name in a specific file (may return >1).
    pub fn find_symbols_by_name_file(&self, name: &str, file_path: &str) -> Result<Vec<SymbolInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
             FROM symbols WHERE name = ?1 AND file_path = ?2
             ORDER BY id ASC",
        )?;
        let results = stmt
            .query_map(params![name, file_path], |row| symbol_info_from_row(row))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(results)
    }

    /// Zero-ambiguity lookup by UID.
    pub fn find_symbol_by_uid(&self, uid: &str) -> Result<Option<(i64, SymbolInfo)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
             FROM symbols WHERE uid = ?1",
        )?;
        let result = stmt
            .query_row(params![uid], |row| {
                Ok((row.get::<_, i64>(0)?, symbol_info_from_row(row)?))
            })
            .optional()?;
        Ok(result)
    }

    /// Resolved context lookup: handles Found / Ambiguous / NotFound.
    pub fn context_resolved(
        &self,
        name: Option<&str>,
        uid: Option<&str>,
        file: Option<&str>,
    ) -> Result<ContextResponse> {
        if let Some(uid) = uid {
            match self.find_symbol_by_uid(uid)? {
                Some((id, symbol)) => {
                    let incoming = self.get_relations_to(id)?;
                    let outgoing = self.get_relations_from(id)?;
                    Ok(ContextResponse::Found { symbol, incoming, outgoing })
                }
                None => Ok(ContextResponse::NotFound {
                    message: format!("No symbol found with uid '{}'", uid),
                }),
            }
        } else if let Some(name) = name {
            if let Some(file) = file {
                let candidates = self.find_symbols_by_name_file(name, file)?;
                match candidates.len() {
                    0 => Ok(ContextResponse::NotFound {
                        message: format!("Symbol '{}' not found in '{}'", name, file),
                    }),
                    1 => {
                        let sym = candidates.into_iter().next().unwrap();
                        let (id, _) = self
                            .find_symbol_by_uid(&sym.uid)?
                            .ok_or_else(|| anyhow::anyhow!("uid '{}' not found", sym.uid))?;
                        let incoming = self.get_relations_to(id)?;
                        let outgoing = self.get_relations_from(id)?;
                        Ok(ContextResponse::Found { symbol: sym, incoming, outgoing })
                    }
                    n => Ok(ContextResponse::Ambiguous {
                        message: format!(
                            "Symbol '{}' in '{}' matches {} entries. Use context --uid \"<uid>\" to select a specific symbol.",
                            name, file, n
                        ),
                        candidates,
                    }),
                }
            } else {
                const LIMIT: usize = 50;
                let candidates = self.find_symbols_by_name(name, LIMIT + 1)?;
                let truncated = candidates.len() > LIMIT;
                let candidates: Vec<SymbolInfo> = candidates.into_iter().take(LIMIT).collect();
                match candidates.len() {
                    0 => Ok(ContextResponse::NotFound {
                        message: format!("Symbol '{}' not found", name),
                    }),
                    1 => {
                        let sym = candidates.into_iter().next().unwrap();
                        let (id, _) = self
                            .find_symbol_by_uid(&sym.uid)?
                            .ok_or_else(|| anyhow::anyhow!("uid '{}' not found", sym.uid))?;
                        let incoming = self.get_relations_to(id)?;
                        let outgoing = self.get_relations_from(id)?;
                        Ok(ContextResponse::Found { symbol: sym, incoming, outgoing })
                    }
                    n => {
                        let message = if truncated {
                            format!(
                                "Symbol '{}' matches more than {} entries (showing first {}). Use --uid or --file for zero-ambiguity lookup.",
                                name, LIMIT, LIMIT
                            )
                        } else {
                            format!(
                                "Symbol '{}' matches {} entries. Use context --uid \"<uid>\" to select a specific symbol.",
                                name, n
                            )
                        };
                        Ok(ContextResponse::Ambiguous { message, candidates })
                    }
                }
            }
        } else {
            anyhow::bail!("Either name or --uid must be provided");
        }
    }

    // ─── Subgraph ────────────────────────────────────────────────────

    pub fn subgraph(
        &self,
        symbol_name: Option<&str>,
        symbol_uid: Option<&str>,
        symbol_id: Option<i64>,
        symbol_file: Option<&str>,
        direction: &str,
        max_depth: u32,
        edge_types: &[String],
        max_nodes: usize,
        max_edges: usize,
    ) -> Result<Option<SubgraphResult>> {
        let validated_edge_types = validate_edge_types(edge_types)?;

        // Resolve start symbol: priority = id → uid → name
        let start_id = if let Some(id) = symbol_id {
            let sym = self.get_symbol_by_id(id)?;
            if sym.is_none() { return Ok(None); }
            id
        } else if let Some(uid) = symbol_uid {
            let found = self.find_symbol_by_uid(uid)?;
            let Some((id, _)) = found else { return Ok(None) };
            id
        } else {
            let Some(name) = symbol_name else {
                anyhow::bail!("symbol_name required when id/uid not given");
            };
            if let Some(file) = symbol_file {
                let id = self.get_symbol_id(name, file)?;
                let Some(id) = id else { return Ok(None) };
                let sym = self.get_symbol_by_id(id)?;
                if sym.is_none() { return Ok(None) };
                id
            } else {
                let sym = self.find_symbol(name)?;
                let Some(sym) = sym else { return Ok(None) };
                let id = self.get_symbol_id(&sym.name, &sym.file_path)?;
                let Some(id) = id else { return Ok(None) };
                id
            }
        };

        // BFS reachability
        // reached[0] = start_id at depth 0; BFS fills the rest up to max_nodes.
        let mut reached: Vec<(i64, u32)> = vec![(start_id, 0)];
        let bfs_capacity = max_nodes.saturating_sub(1); // slots available beyond start
        let bfs_capped;

        match direction {
            "outgoing" => {
                let (bfs, capped) = self.run_reachability_bfs(start_id, true, max_depth, &validated_edge_types, bfs_capacity)?;
                reached.extend(bfs);
                bfs_capped = capped;
            }
            "incoming" => {
                let (bfs, capped) = self.run_reachability_bfs(start_id, false, max_depth, &validated_edge_types, bfs_capacity)?;
                reached.extend(bfs);
                bfs_capped = capped;
            }
            "both" => {
                let (out, capped_out) = self.run_reachability_bfs(start_id, true, max_depth, &validated_edge_types, bfs_capacity)?;
                let (inc, capped_inc) = self.run_reachability_bfs(start_id, false, max_depth, &validated_edge_types, bfs_capacity)?;
                bfs_capped = capped_out || capped_inc;
                // Merge using minimum depth per node to guarantee shortest-path ordering
                let mut depth_map: HashMap<i64, u32> = HashMap::new();
                for (id, depth) in out.iter().chain(inc.iter()) {
                    depth_map.entry(*id)
                        .and_modify(|d| *d = (*d).min(*depth))
                        .or_insert(*depth);
                }
                for (id, depth) in depth_map {
                    reached.push((id, depth));
                }
            }
            other => {
                anyhow::bail!("invalid direction '{}': must be 'outgoing', 'incoming', or 'both'", other);
            }
        }

        // Sort by depth ASC, id ASC
        reached.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        // Truncate nodes; also truncated if BFS hit capacity limit
        let mut truncated = false;
        let mut truncated_reason: Option<String> = None;
        if reached.len() > max_nodes {
            reached.truncate(max_nodes);
            truncated = true;
            truncated_reason = Some(format!("node limit ({}) reached", max_nodes));
        } else if bfs_capped {
            truncated = true;
            truncated_reason = Some(format!("node limit ({}) reached", max_nodes));
        }

        // Fetch symbol info for all node ids
        let node_ids: Vec<i64> = reached.iter().map(|(id, _)| *id).collect();
        let sym_map = self.get_symbols_by_ids(&node_ids)?;

        let nodes: Vec<SubgraphNode> = reached
            .iter()
            .filter_map(|(id, _)| {
                sym_map.get(id).map(|sym| SubgraphNode {
                    id: *id,
                    symbol: sym.clone(),
                })
            })
            .collect();

        // Collect edges between reached nodes
        let mut edges = self.collect_edges(&node_ids, &validated_edge_types)?;

        // Sort edges: source_id ASC, target_id ASC, kind ASC
        edges.sort_by(|a, b| {
            a.source_id.cmp(&b.source_id)
                .then(a.target_id.cmp(&b.target_id))
                .then(a.kind.cmp(&b.kind))
        });

        if edges.len() > max_edges {
            edges.truncate(max_edges);
            if !truncated {
                truncated = true;
                truncated_reason = Some(format!("edge limit ({}) reached", max_edges));
            } else {
                truncated_reason = truncated_reason.map(|r| format!("{}; edge limit ({}) reached", r, max_edges));
            }
        }

        let node_count = nodes.len();
        let edge_count = edges.len();

        Ok(Some(SubgraphResult {
            start_id,
            nodes,
            edges,
            node_count,
            edge_count,
            truncated,
            truncated_reason,
        }))
    }

    /// Depth-limited BFS: returns `(nodes, was_capped)`.
    /// `nodes` = (node_id, depth) pairs reachable from start (excluding start itself).
    /// `was_capped` = true when BFS stopped due to `capacity` limit (more nodes may exist).
    fn run_reachability_bfs(
        &self,
        start_id: i64,
        outgoing: bool,
        max_depth: u32,
        edge_types: &[String],
        capacity: usize,
    ) -> Result<(Vec<(i64, u32)>, bool)> {
        let mut visited: HashSet<i64> = HashSet::new();
        visited.insert(start_id);
        let mut frontier: Vec<i64> = vec![start_id];
        let mut result: Vec<(i64, u32)> = Vec::new();
        let mut capped = false;

        for depth in 1..=max_depth {
            if frontier.is_empty() {
                break;
            }
            if result.len() >= capacity {
                capped = true;
                break;
            }

            let next = self.expand_frontier(&frontier, &visited, outgoing, edge_types)?;
            if next.is_empty() {
                frontier = vec![];
                continue;
            }

            let remaining = capacity - result.len();
            if next.len() > remaining {
                capped = true;
                frontier = next[..remaining].to_vec();
            } else {
                frontier = next;
            }

            for &id in &frontier {
                visited.insert(id);
                result.push((id, depth));
            }
        }

        Ok((result, capped))
    }

    /// One-hop expansion: returns new node ids reachable from frontier, not in visited.
    fn expand_frontier(
        &self,
        frontier: &[i64],
        visited: &HashSet<i64>,
        outgoing: bool,
        edge_types: &[String],
    ) -> Result<Vec<i64>> {
        if frontier.is_empty() {
            return Ok(vec![]);
        }

        // i64 ids are safe to interpolate directly
        let frontier_list: String = frontier.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let visited_list: String = visited.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");

        let (src_col, tgt_col) = if outgoing {
            ("source_id", "target_id")
        } else {
            ("target_id", "source_id")
        };

        let kind_filter = if edge_types.is_empty() {
            String::new()
        } else {
            let placeholders: String = (1..=edge_types.len()).map(|i| format!("?{}", i)).collect::<Vec<_>>().join(",");
            format!(" AND r.kind IN ({})", placeholders)
        };

        let sql = format!(
            "SELECT DISTINCT r.{tgt} FROM relationships r \
             WHERE r.{src} IN ({frontier}) \
             AND r.{tgt} NOT IN ({visited}){kind_filter}",
            tgt = tgt_col,
            src = src_col,
            frontier = frontier_list,
            visited = visited_list,
            kind_filter = kind_filter,
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let ids: Vec<i64> = if edge_types.is_empty() {
            stmt.query_map([], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            let params: Vec<&dyn rusqlite::types::ToSql> = edge_types.iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            stmt.query_map(params.as_slice(), |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        Ok(ids)
    }

    /// Collect all edges between given node ids, filtered by edge_types.
    fn collect_edges(&self, node_ids: &[i64], edge_types: &[String]) -> Result<Vec<EdgeEntry>> {
        if node_ids.is_empty() {
            return Ok(vec![]);
        }

        let id_list: String = node_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        let kind_filter = if edge_types.is_empty() {
            String::new()
        } else {
            let placeholders: String = (1..=edge_types.len()).map(|i| format!("?{}", i)).collect::<Vec<_>>().join(",");
            format!(" AND r.kind IN ({})", placeholders)
        };

        let sql = format!(
            "SELECT r.source_id, r.target_id, r.kind, r.confidence, r.reason \
             FROM relationships r \
             WHERE r.source_id IN ({ids}) AND r.target_id IN ({ids}){kind_filter} \
             ORDER BY r.source_id, r.target_id, r.kind",
            ids = id_list,
            kind_filter = kind_filter,
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let entries: Vec<EdgeEntry> = if edge_types.is_empty() {
            stmt.query_map([], |row| {
                Ok(EdgeEntry {
                    source_id: row.get(0)?,
                    target_id: row.get(1)?,
                    kind: row.get(2)?,
                    confidence: row.get(3)?,
                    reason: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            let params: Vec<&dyn rusqlite::types::ToSql> = edge_types.iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            stmt.query_map(params.as_slice(), |row| {
                Ok(EdgeEntry {
                    source_id: row.get(0)?,
                    target_id: row.get(1)?,
                    kind: row.get(2)?,
                    confidence: row.get(3)?,
                    reason: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };

        Ok(entries)
    }

    /// Query data flows for a symbol (by name, optionally filtered by file or uid).
    pub fn dataflow(
        &self,
        name: Option<&str>,
        file: Option<&str>,
        uid: Option<&str>,
    ) -> Result<DataflowResult> {
        // Resolve symbol
        let (_sym_id, sym_info) = if let Some(uid_val) = uid {
            self.find_symbol_by_uid(uid_val)?
                .ok_or_else(|| anyhow::anyhow!("symbol not found for uid: {}", uid_val))?
        } else {
            let name_val = name.ok_or_else(|| anyhow::anyhow!("name or uid required"))?;
            if let Some(file_val) = file {
                self.symbol_by_name_file(name_val, file_val)?
                    .ok_or_else(|| anyhow::anyhow!("symbol '{}' not found in file '{}'", name_val, file_val))?
            } else {
                let candidates = self.find_symbols_by_name(name_val, 10)?;
                if candidates.is_empty() {
                    anyhow::bail!("symbol not found: {}", name_val);
                }
                if candidates.len() > 1 {
                    // Try to find function/method specifically
                    let funcs: Vec<_> = candidates.iter()
                        .filter(|s| s.kind == "Function" || s.kind == "Method")
                        .collect();
                    if funcs.len() == 1 {
                        let id = self.get_symbol_id_by_uid(&funcs[0].uid)?;
                        (id, funcs[0].clone())
                    } else {
                        anyhow::bail!(
                            "ambiguous symbol '{}' ({} matches). Use --file or --uid to disambiguate.",
                            name_val, candidates.len()
                        );
                    }
                } else {
                    let id = self.get_symbol_id_by_uid(&candidates[0].uid)?;
                    (id, candidates[0].clone())
                }
            }
        };

        // Query data flows by function UID
        let function_uid = sym_info.uid.clone();
        let mut stmt = self.conn.prepare(
            "SELECT source_expr, sink_expr, flow_kind, source_line, sink_line
             FROM data_flows WHERE function_uid = ?1 ORDER BY source_line",
        )?;
        let flows = stmt.query_map(params![function_uid], |row| {
            Ok(DataflowEntry {
                source_expr: row.get(0)?,
                sink_expr: row.get(1)?,
                flow_kind: row.get(2)?,
                source_line: row.get(3)?,
                sink_line: row.get(4)?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(DataflowResult {
            symbol: sym_info,
            flows,
        })
    }

    fn get_symbol_id_by_uid(&self, uid: &str) -> Result<i64> {
        let id = self.conn.query_row(
            "SELECT id FROM symbols WHERE uid = ?1",
            params![uid],
            |row| row.get(0),
        )?;
        Ok(id)
    }
}

#[derive(Debug, Serialize)]
pub struct DataflowEntry {
    pub source_expr: String,
    pub sink_expr: String,
    pub flow_kind: String,
    pub source_line: i64,
    pub sink_line: i64,
}

#[derive(Debug, Serialize)]
pub struct DataflowResult {
    pub symbol: SymbolInfo,
    pub flows: Vec<DataflowEntry>,
}

// ─── Free functions ──────────────────────────────────────────────────

const VALID_EDGE_KINDS: &[&str] = &["CALLS", "CALLS_UNRESOLVED", "CALLS_EXTERNAL", "IMPORTS", "EXTENDS", "IMPLEMENTS", "DEFINES", "CONTAINS"];

fn validate_edge_types(types: &[String]) -> Result<Vec<String>> {
    for t in types {
        if !VALID_EDGE_KINDS.contains(&t.as_str()) {
            anyhow::bail!("invalid edge type: {}", t);
        }
    }
    Ok(types.to_vec())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        (dot / (norm_a * norm_b)) as f64
    }
}

fn compute_risk(has_exported_at_d1: bool, total_affected: usize, depth_1_count: usize) -> String {
    if has_exported_at_d1 || total_affected >= 10 {
        "high".to_string()
    } else if depth_1_count > 0 {
        "medium".to_string()
    } else {
        "low".to_string()
    }
}

fn build_impact_summary(
    name: &str,
    direction: &str,
    total: usize,
    depth1: usize,
    risk: &str,
    mod_count: usize,
    proc_count: usize,
) -> String {
    let mut parts = format!(
        "Changing {} ({}) affects {} symbol(s) ({} direct)",
        name, direction, total, depth1
    );
    if mod_count > 0 && proc_count > 0 {
        parts.push_str(&format!(" in {} module(s) and {} process(es).", mod_count, proc_count));
    } else if mod_count > 0 {
        parts.push_str(&format!(" in {} module(s).", mod_count));
    } else if proc_count > 0 {
        parts.push_str(&format!(" in {} process(es).", proc_count));
    } else {
        parts.push('.');
    }
    parts.push_str(&format!(" Risk: {}.", risk));
    parts
}

fn rrf_score(bm25_rank: Option<usize>, vec_rank: Option<usize>, k: usize) -> f64 {
    let mut score = 0.0f64;
    if let Some(r) = bm25_rank {
        score += 1.0 / (k + r + 1) as f64;
    }
    if let Some(r) = vec_rank {
        score += 1.0 / (k + r + 1) as f64;
    }
    score
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical() {
        let v = vec![1.0f32, 0.0, 0.0];
        let score = cosine_similarity(&v, &v);
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let score = cosine_similarity(&a, &b);
        assert!(score.abs() < 1e-6);
    }

    #[test]
    fn rrf_both_rank() {
        // rank 0 in both → 1/61 + 1/61 = 2/61
        let score = rrf_score(Some(0), Some(0), 60);
        let expected = 2.0 / 61.0;
        assert!((score - expected).abs() < 1e-9);
    }

    #[test]
    fn rrf_one_only() {
        // only vec rank 0 → 1/61
        let score = rrf_score(None, Some(0), 60);
        let expected = 1.0 / 61.0;
        assert!((score - expected).abs() < 1e-9);
    }

    #[test]
    fn compute_risk_boundaries() {
        // low: depth_1_count == 0 かつ total < 10
        assert_eq!(compute_risk(false, 0, 0), "low");
        assert_eq!(compute_risk(false, 9, 0), "low");
        // medium: depth_1_count > 0 かつ high 条件を満たさない
        assert_eq!(compute_risk(false, 1, 1), "medium");
        assert_eq!(compute_risk(false, 9, 1), "medium");
        // high: total_affected == 10 (境界値, depth_1_count == 0 でも high)
        assert_eq!(compute_risk(false, 10, 0), "high");
        assert_eq!(compute_risk(false, 11, 0), "high");
        // high: has_exported_at_d1 (total が少なくても high)
        assert_eq!(compute_risk(true, 1, 1), "high");
        assert_eq!(compute_risk(true, 0, 0), "high");
    }

    /// Verify that SQLite UNION-based CTE produces the same reachable set as BFS.
    /// This test documents why the CTE cannot fully replace BFS:
    /// UNION deduplicates by row equality, so depth tracking is lost,
    /// and mid-traversal capacity capping is not possible with LIMIT-only SQL.
    #[test]
    fn subgraph_cte_matches_bfs() {
        use rusqlite::Connection;

        // Build an in-memory graph: 1 → 2 → 3, 1 → 3 (diamond + shared edge)
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE symbols(id INTEGER PRIMARY KEY, uid TEXT, name TEXT, kind TEXT,
                file_path TEXT, start_line INT, end_line INT, is_exported INT, parent_name TEXT);
            CREATE TABLE relationships(source_id INT, target_id INT, kind TEXT, confidence REAL, reason TEXT);
            INSERT INTO symbols VALUES(1,'u1','A','Function','f.go',1,1,0,NULL);
            INSERT INTO symbols VALUES(2,'u2','B','Function','f.go',2,2,0,NULL);
            INSERT INTO symbols VALUES(3,'u3','C','Function','f.go',3,3,0,NULL);
            INSERT INTO relationships VALUES(1,2,'CALLS',1.0,'');
            INSERT INTO relationships VALUES(2,3,'CALLS',1.0,'');
            INSERT INTO relationships VALUES(1,3,'CALLS',1.0,'');
        ").unwrap();

        // CTE reachable set (UNION deduplicates → no cycle risk, but no depth tracking)
        let cte_sql = "
            WITH RECURSIVE reachable(id) AS (
                SELECT 1
                UNION
                SELECT r.target_id FROM relationships r
                JOIN reachable c ON c.id = r.source_id
            )
            SELECT id FROM reachable WHERE id != 1 ORDER BY id
        ";
        let mut stmt = conn.prepare(cte_sql).unwrap();
        let cte_ids: Vec<i64> = stmt.query_map([], |row| row.get(0)).unwrap()
            .map(|r| r.unwrap())
            .collect();

        // BFS via QueryEngine
        let engine = QueryEngine::new(&conn);
        let (bfs_nodes, _capped) = engine.run_reachability_bfs(1, true, 10, &[], usize::MAX).unwrap();
        let mut bfs_ids: Vec<i64> = bfs_nodes.iter().map(|(id, _)| *id).collect();
        bfs_ids.sort();

        assert_eq!(cte_ids, bfs_ids,
            "CTE reachable set should equal BFS reachable set (both find {{2,3}})");
        // Note: CTE cannot track per-node depth or enforce mid-traversal capacity caps,
        // so BFS is retained as the production implementation.

        // --- Cycle case: add back-edge 3→1 to create A→B→C→A ---
        conn.execute("INSERT INTO relationships VALUES(3,1,'CALLS',1.0,'')", []).unwrap();

        // CTE with UNION terminates because UNION deduplicates by row value;
        // the recursive term never produces a new (previously-unseen) row.
        let mut stmt2 = conn.prepare(cte_sql).unwrap();
        let cte_cycle_ids: Vec<i64> = stmt2.query_map([], |row| row.get(0)).unwrap()
            .map(|r| r.unwrap())
            .collect();

        // BFS with HashSet visited also terminates correctly.
        let (bfs_cycle_nodes, _) = engine.run_reachability_bfs(1, true, 10, &[], usize::MAX).unwrap();
        let mut bfs_cycle_ids: Vec<i64> = bfs_cycle_nodes.iter().map(|(id, _)| *id).collect();
        bfs_cycle_ids.sort();

        // Both should reach {2,3} — the cycle back to 1 is suppressed by dedup.
        assert_eq!(cte_cycle_ids, bfs_cycle_ids,
            "Both CTE and BFS should handle cycles without infinite loops");
    }

    /// Prove that QueryEngine::subgraph terminates on a graph with an explicit cycle A→B→C→A.
    /// This is the definitive termination proof for the cycle-safety DoD.
    #[test]
    fn subgraph_terminates_on_cycle() {
        use rusqlite::Connection;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE symbols(id INTEGER PRIMARY KEY, uid TEXT, name TEXT, kind TEXT,
                file_path TEXT, start_line INT, end_line INT, is_exported INT, parent_name TEXT);
            CREATE TABLE relationships(source_id INT, target_id INT, kind TEXT, confidence REAL, reason TEXT);
            CREATE TABLE embeddings(symbol_id INT, model_id TEXT, dims INT, vector_blob BLOB, content_hash TEXT, updated_at INT);
            INSERT INTO symbols VALUES(1,'u1','A','Function','f.go',1,2,1,NULL);
            INSERT INTO symbols VALUES(2,'u2','B','Function','f.go',3,4,0,NULL);
            INSERT INTO symbols VALUES(3,'u3','C','Function','f.go',5,6,0,NULL);
            -- Explicit cycle: A→B→C→A
            INSERT INTO relationships VALUES(1,2,'CALLS',1.0,'');
            INSERT INTO relationships VALUES(2,3,'CALLS',1.0,'');
            INSERT INTO relationships VALUES(3,1,'CALLS',1.0,'');
        ").unwrap();

        let engine = QueryEngine::new(&conn);

        // Both outgoing and both-direction should terminate without infinite recursion.
        let out = engine.subgraph(Some("A"), None, None, None, "outgoing", 10, &[], 100, 500)
            .expect("subgraph should not error on a cycle");
        let out = out.expect("should find symbol A");
        // All 3 nodes reachable; BFS stops because visited set prevents re-queuing.
        assert_eq!(out.node_count, 3, "should reach all 3 nodes in the cycle");

        let both = engine.subgraph(Some("A"), None, None, None, "both", 10, &[], 100, 500)
            .expect("subgraph both-direction should not error on a cycle");
        let both = both.expect("should find symbol A");
        assert_eq!(both.node_count, 3, "direction=both should also reach all 3 nodes");
    }

    #[test]
    fn vector_blob_roundtrip() {
        let original = vec![1.5f32, -2.0, 0.333];
        let blob: Vec<u8> = original.iter().flat_map(|f| f.to_le_bytes()).collect();
        let restored: Vec<f32> = blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 1e-9);
        }
    }
}

/// Build a SymbolInfo from a rusqlite Row using named column access.
/// SQL must SELECT: uid, name, kind, file_path, start_line, end_line, is_exported, parent_name
fn symbol_info_from_row(row: &rusqlite::Row) -> rusqlite::Result<SymbolInfo> {
    Ok(SymbolInfo {
        uid: row.get("uid")?,
        name: row.get("name")?,
        kind: row.get("kind")?,
        file_path: row.get("file_path")?,
        start_line: row.get("start_line")?,
        end_line: row.get("end_line")?,
        is_exported: row.get("is_exported")?,
        parent_name: row.get("parent_name")?,
    })
}

// rusqlite optional() helper
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
