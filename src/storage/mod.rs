pub mod write;
mod read;

#[allow(unused_imports)]
pub use read::{CommunityInfo, DataFlowInfo, ProcessInfo, ProcessStepInfo};

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub struct Database {
    conn: Connection,
}

pub struct IndexStats {
    pub symbol_count: usize,
    pub relationship_count: usize,
    pub file_count: usize,
    pub community_count: usize,
    pub process_count: usize,
    pub last_indexed: Option<String>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Get a reference to the underlying connection (for query engine).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS symbols (
                id          INTEGER PRIMARY KEY,
                uid         TEXT UNIQUE NOT NULL,
                name        TEXT NOT NULL,
                kind        TEXT NOT NULL,
                file_path   TEXT NOT NULL,
                start_line  INTEGER,
                end_line    INTEGER,
                is_exported BOOLEAN DEFAULT FALSE,
                parent_name TEXT,
                type_info   TEXT,
                metadata    TEXT,
                content_hash INTEGER
            );

            CREATE TABLE IF NOT EXISTS relationships (
                id          INTEGER PRIMARY KEY,
                source_id   INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
                target_id   INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
                kind        TEXT NOT NULL,
                confidence  REAL DEFAULT 1.0,
                reason      TEXT,
                metadata    TEXT,
                UNIQUE(source_id, target_id, kind)
            );

            CREATE TABLE IF NOT EXISTS communities (
                id           INTEGER PRIMARY KEY,
                label        TEXT,
                cohesion     REAL,
                symbol_count INTEGER
            );

            CREATE TABLE IF NOT EXISTS community_members (
                community_id INTEGER REFERENCES communities(id) ON DELETE CASCADE,
                symbol_id    INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
                PRIMARY KEY (community_id, symbol_id)
            );

            CREATE TABLE IF NOT EXISTS processes (
                id           INTEGER PRIMARY KEY,
                label        TEXT,
                process_type TEXT,
                priority     REAL,
                step_count   INTEGER
            );

            CREATE TABLE IF NOT EXISTS process_steps (
                process_id  INTEGER REFERENCES processes(id) ON DELETE CASCADE,
                symbol_id   INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
                step_index  INTEGER,
                PRIMARY KEY (process_id, step_index)
            );

            CREATE TABLE IF NOT EXISTS file_index (
                path         TEXT PRIMARY KEY,
                content_hash INTEGER NOT NULL,
                last_indexed TEXT NOT NULL,
                language     TEXT NOT NULL,
                size_bytes   INTEGER
            );

            CREATE TABLE IF NOT EXISTS embeddings (
                symbol_id    INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
                model_id     TEXT NOT NULL DEFAULT 'all-MiniLM-L6-v2',
                dims         INTEGER NOT NULL DEFAULT 384,
                vector_blob  BLOB NOT NULL,
                content_hash INTEGER NOT NULL,
                updated_at   INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path);
            CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
            CREATE INDEX IF NOT EXISTS idx_rel_source ON relationships(source_id);
            CREATE INDEX IF NOT EXISTS idx_rel_target ON relationships(target_id);
            CREATE INDEX IF NOT EXISTS idx_rel_kind ON relationships(kind);

            CREATE TABLE IF NOT EXISTS data_flows (
                id              INTEGER PRIMARY KEY,
                function_uid    TEXT,
                source_expr     TEXT NOT NULL,
                sink_expr       TEXT NOT NULL,
                flow_kind       TEXT NOT NULL,
                source_line     INTEGER,
                sink_line       INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_df_function ON data_flows(function_uid);

            CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
                name, file_path, kind, parent_name,
                content='symbols', content_rowid='id'
            );
            ",
        )?;
        Ok(())
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{}", now)
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
