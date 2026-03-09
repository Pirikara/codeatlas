use anyhow::Result;
use rusqlite::params;
use std::path::Path;

use crate::analyzer::{Community, Process, Relationship};
use crate::parser::Symbol;
use crate::scanner::FileInfo;

use super::{chrono_now, Database, OptionalExt};

pub struct EmbeddingRecord {
    pub symbol_id: i64,
    pub model_id: String,
    pub dims: usize,
    pub vector: Vec<f32>,
    pub content_hash: i64,
}

impl Database {
    /// Get the stored content hash for a file path.
    pub fn get_file_hash(&self, path: &str) -> Result<Option<u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content_hash FROM file_index WHERE path = ?1")?;
        let result = stmt
            .query_row(params![path], |row| row.get::<_, i64>(0))
            .optional()?
            .map(|h| h as u64);
        Ok(result)
    }

    /// Store symbols and update file index within a transaction.
    pub fn store_symbols(
        &self,
        symbols: &[Symbol],
        files: &[FileInfo],
        _root: &Path,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Delete old symbols for files being re-indexed (CASCADE deletes relationships)
        for file in files {
            tx.execute(
                "DELETE FROM symbols WHERE file_path = ?1",
                params![file.relative_path],
            )?;
        }

        // Insert new symbols
        let mut insert_sym = tx.prepare(
            "INSERT OR IGNORE INTO symbols (uid, name, kind, file_path, start_line, end_line, is_exported, parent_name, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        )?;

        for sym in symbols {
            insert_sym.execute(params![
                sym.uid(),
                sym.name,
                sym.kind.to_string(),
                sym.file_path,
                sym.start_line as i64,
                sym.end_line as i64,
                sym.is_exported,
                sym.parent_name,
                0i64, // content_hash on symbol — will be set per file
            ])?;
        }

        // Insert File nodes so IMPORTS edges (File:{path} UIDs) can resolve
        // and Folder nodes for CONTAINS edges
        let mut folders_seen = std::collections::HashSet::new();
        for file in files {
            let file_uid = format!("File:{}", file.relative_path);
            insert_sym.execute(params![
                file_uid,
                file.relative_path,
                "File",
                file.relative_path,
                0i64,
                0i64,
                false,
                Option::<String>::None,
                file.content_hash as i64,
            ])?;

            // Insert Folder nodes for each directory in the path
            let path = std::path::Path::new(&file.relative_path);
            let mut current = path.parent();
            while let Some(dir) = current {
                let dir_str = dir.to_string_lossy().to_string();
                if dir_str.is_empty() || !folders_seen.insert(dir_str.clone()) {
                    break;
                }
                let folder_uid = format!("Folder:{}", dir_str);
                let dir_name = dir.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| dir_str.clone());
                insert_sym.execute(params![
                    folder_uid,
                    dir_name,
                    "Folder",
                    dir_str,
                    0i64,
                    0i64,
                    false,
                    Option::<String>::None,
                    0i64,
                ])?;
                current = dir.parent();
            }
        }
        drop(insert_sym);

        // Update file index
        let now = chrono_now();
        let mut insert_file = tx.prepare(
            "INSERT OR REPLACE INTO file_index (path, content_hash, last_indexed, language, size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for file in files {
            insert_file.execute(params![
                file.relative_path,
                file.content_hash as i64,
                now,
                file.language.to_string(),
                file.size_bytes as i64,
            ])?;
        }
        drop(insert_file);

        // Rebuild FTS index
        tx.execute("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild')", [])?;

        tx.commit()?;
        Ok(())
    }

    /// Store relationships (CALLS, IMPORTS, etc.) using symbol UIDs.
    pub fn store_relationships(&self, relationships: &[Relationship]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Clear existing relationships (they'll be rebuilt)
        tx.execute("DELETE FROM relationships", [])?;

        let mut insert = tx.prepare(
            "INSERT OR IGNORE INTO relationships (source_id, target_id, kind, confidence, reason)
             SELECT s.id, t.id, ?3, ?4, ?5
             FROM symbols s, symbols t
             WHERE s.uid = ?1 AND t.uid = ?2",
        )?;

        let mut stored = 0;
        for rel in relationships {
            let changed = insert.execute(params![
                rel.source_uid,
                rel.target_uid,
                rel.kind.to_string(),
                rel.confidence,
                rel.reason,
            ])?;
            if changed > 0 {
                stored += 1;
            }
        }
        drop(insert);

        tx.commit()?;
        eprintln!("  Stored {} relationships (out of {} candidates)", stored, relationships.len());
        Ok(())
    }

    /// Remove stale files (deleted from disk) from the index.
    /// Deletes symbols (→ cascades to relationships, embeddings, community_members, process_steps)
    /// and file_index rows, then rebuilds FTS.
    /// Returns the number of files removed.
    pub fn cleanup_deleted_files(&self, stale_paths: &[String]) -> Result<usize> {
        if stale_paths.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        for path in stale_paths {
            tx.execute("DELETE FROM symbols WHERE file_path = ?1", params![path])?;
            tx.execute("DELETE FROM file_index WHERE path = ?1", params![path])?;
        }
        tx.execute("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild')", [])?;
        tx.commit()?;
        Ok(stale_paths.len())
    }

    /// Upsert embedding records for symbols.
    pub fn upsert_embeddings(&self, records: &[EmbeddingRecord]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO embeddings (symbol_id, model_id, dims, vector_blob, content_hash, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        for rec in records {
            let blob: Vec<u8> = rec.vector.iter().flat_map(|f| f.to_le_bytes()).collect();
            stmt.execute(params![
                rec.symbol_id,
                rec.model_id,
                rec.dims as i64,
                blob,
                rec.content_hash,
                now,
            ])?;
        }
        drop(stmt);
        tx.commit()?;
        Ok(())
    }

    /// Store detected communities.
    pub fn store_communities(&self, communities: &[Community]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute("DELETE FROM community_members", [])?;
        tx.execute("DELETE FROM communities", [])?;

        let mut insert_comm = tx.prepare(
            "INSERT INTO communities (id, label, cohesion, symbol_count)
             VALUES (?1, ?2, ?3, ?4)",
        )?;

        let mut insert_member = tx.prepare(
            "INSERT OR IGNORE INTO community_members (community_id, symbol_id)
             SELECT ?1, id FROM symbols WHERE uid = ?2",
        )?;

        for comm in communities {
            insert_comm.execute(params![
                comm.id as i64,
                comm.label,
                comm.cohesion,
                comm.members.len() as i64,
            ])?;

            for member_uid in &comm.members {
                insert_member.execute(params![comm.id as i64, member_uid])?;
            }
        }

        drop(insert_comm);
        drop(insert_member);
        tx.commit()?;
        Ok(())
    }

    /// Store detected processes (execution flows).
    pub fn store_processes(&self, processes: &[Process]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute("DELETE FROM process_steps", [])?;
        tx.execute("DELETE FROM processes", [])?;

        let mut insert_proc = tx.prepare(
            "INSERT INTO processes (id, label, process_type, priority, step_count)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let mut insert_step = tx.prepare(
            "INSERT OR IGNORE INTO process_steps (process_id, symbol_id, step_index)
             SELECT ?1, id, ?3 FROM symbols WHERE uid = ?2",
        )?;

        for proc in processes {
            insert_proc.execute(params![
                proc.id as i64,
                proc.label,
                proc.process_type.to_string(),
                proc.priority,
                proc.steps.len() as i64,
            ])?;

            for (idx, step_uid) in proc.steps.iter().enumerate() {
                insert_step.execute(params![proc.id as i64, step_uid, idx as i64])?;
            }
        }

        drop(insert_proc);
        drop(insert_step);
        tx.commit()?;
        Ok(())
    }
}
