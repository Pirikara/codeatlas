use anyhow::Result;
use rusqlite::params;

use super::{Database, IndexStats, OptionalExt};

#[derive(Debug, serde::Serialize)]
pub struct CommunityInfo {
    pub id: usize,
    pub label: Option<String>,
    pub cohesion: f64,
    pub symbol_count: usize,
    pub top_symbols: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ProcessInfo {
    pub id: usize,
    pub label: String,
    pub process_type: String,
    pub priority: f64,
    pub step_count: usize,
    pub steps: Vec<ProcessStepInfo>,
}

#[derive(Debug, serde::Serialize)]
pub struct ProcessStepInfo {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub step_index: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct DataFlowInfo {
    pub id: i64,
    pub function_uid: Option<String>,
    pub source_expr: String,
    pub sink_expr: String,
    pub flow_kind: String,
    pub source_line: i64,
    pub sink_line: i64,
}

impl Database {
    /// Get symbols that need embeddings (new or stale due to hash/model/dims change).
    /// Returns (symbol_id, name, kind, file_path, parent_name, file_content_hash).
    pub fn get_symbols_needing_embed(
        &self,
        model_id: &str,
        dims: i64,
    ) -> Result<Vec<(i64, String, String, String, Option<String>, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.kind, s.file_path, s.parent_name, fi.content_hash
             FROM symbols s
             JOIN file_index fi ON fi.path = s.file_path
             LEFT JOIN embeddings e ON e.symbol_id = s.id
             WHERE s.kind NOT IN ('File', 'Folder')
               AND (
                 e.symbol_id IS NULL
                 OR e.content_hash != fi.content_hash
                 OR e.model_id != ?1
                 OR e.dims != ?2
               )",
        )?;

        let rows = stmt
            .query_map(params![model_id, dims], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Get all symbols (including already-embedded) for force re-embed.
    pub fn get_all_symbols_for_embed(
        &self,
    ) -> Result<Vec<(i64, String, String, String, Option<String>, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.kind, s.file_path, s.parent_name, fi.content_hash
             FROM symbols s
             JOIN file_index fi ON fi.path = s.file_path
             WHERE s.kind NOT IN ('File', 'Folder')",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Return all file paths currently recorded in file_index.
    pub fn get_all_indexed_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM file_index")?;
        let paths = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(paths)
    }

    /// Get index statistics.
    pub fn stats(&self) -> Result<IndexStats> {
        let symbol_count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        let relationship_count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;
        let file_count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM file_index", [], |row| row.get(0))?;
        let community_count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM communities", [], |row| row.get(0))?;
        let process_count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM processes", [], |row| row.get(0))?;
        let last_indexed: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(last_indexed) FROM file_index",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        Ok(IndexStats {
            symbol_count,
            relationship_count,
            file_count,
            community_count,
            process_count,
            last_indexed,
        })
    }

    /// List all communities with their top members.
    pub fn list_communities(&self) -> Result<Vec<CommunityInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.label, c.cohesion, c.symbol_count FROM communities c ORDER BY c.symbol_count DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CommunityInfo {
                id: row.get(0)?,
                label: row.get(1)?,
                cohesion: row.get(2)?,
                symbol_count: row.get(3)?,
                top_symbols: Vec::new(),
            })
        })?;
        let mut communities: Vec<CommunityInfo> = rows.collect::<std::result::Result<_, _>>()?;

        let mut member_stmt = self.conn.prepare(
            "SELECT s.name, s.kind FROM community_members cm
             JOIN symbols s ON s.id = cm.symbol_id
             WHERE cm.community_id = ?1
             ORDER BY s.name LIMIT 10",
        )?;
        for comm in &mut communities {
            let members: Vec<String> = member_stmt
                .query_map(params![comm.id as i64], |row| {
                    let name: String = row.get(0)?;
                    let kind: String = row.get(1)?;
                    Ok(format!("{} {}", kind, name))
                })?
                .collect::<std::result::Result<_, _>>()?;
            comm.top_symbols = members;
        }
        Ok(communities)
    }

    /// Get data flows for a given function UID.
    #[allow(dead_code)]
    pub fn get_data_flows_by_function(&self, function_uid: &str) -> Result<Vec<DataFlowInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, function_uid, source_expr, sink_expr, flow_kind, source_line, sink_line
             FROM data_flows WHERE function_uid = ?1 ORDER BY source_line",
        )?;
        let rows = stmt.query_map(params![function_uid], |row| {
            Ok(DataFlowInfo {
                id: row.get(0)?,
                function_uid: row.get(1)?,
                source_expr: row.get(2)?,
                sink_expr: row.get(3)?,
                flow_kind: row.get(4)?,
                source_line: row.get(5)?,
                sink_line: row.get(6)?,
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List all processes with their steps.
    pub fn list_processes(&self) -> Result<Vec<ProcessInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.label, p.process_type, p.priority, p.step_count FROM processes p ORDER BY p.priority DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProcessInfo {
                id: row.get(0)?,
                label: row.get(1)?,
                process_type: row.get(2)?,
                priority: row.get(3)?,
                step_count: row.get(4)?,
                steps: Vec::new(),
            })
        })?;
        let mut processes: Vec<ProcessInfo> = rows.collect::<std::result::Result<_, _>>()?;

        let mut step_stmt = self.conn.prepare(
            "SELECT s.name, s.kind, s.file_path, ps.step_index FROM process_steps ps
             JOIN symbols s ON s.id = ps.symbol_id
             WHERE ps.process_id = ?1
             ORDER BY ps.step_index",
        )?;
        for proc in &mut processes {
            let steps: Vec<ProcessStepInfo> = step_stmt
                .query_map(params![proc.id as i64], |row| {
                    Ok(ProcessStepInfo {
                        name: row.get(0)?,
                        kind: row.get(1)?,
                        file_path: row.get(2)?,
                        step_index: row.get(3)?,
                    })
                })?
                .collect::<std::result::Result<_, _>>()?;
            proc.steps = steps;
        }
        Ok(processes)
    }
}
