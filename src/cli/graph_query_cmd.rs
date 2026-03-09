use crate::cli::GraphQueryArgs;
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};

pub fn run(args: GraphQueryArgs, json: bool) -> Result<()> {
    // 1. Keyword validation (UX: early error)
    validate_keyword(&args.query)?;

    // 2. Resolve DB path
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        bail!("No index found. Run `codeatlas index` first.");
    }

    // 3. Read-only connection (effective guarantee: SQLITE_OPEN_READ_ONLY)
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open read-only DB: {}", db_path.display()))?;

    // 4. Prepare + stmt.readonly() check
    let mut stmt = conn
        .prepare(&args.query)
        .with_context(|| "failed to prepare query")?;
    if !stmt.readonly() {
        bail!("Only read-only queries are allowed (sqlite3_stmt_readonly check failed)");
    }

    // 5. Execute with row limit
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows: Vec<serde_json::Value> = stmt
        .query_map([], |row| Ok(build_row(row, &col_names)))?
        .take(args.limit)
        .collect::<Result<Vec<_>, _>>()?;

    // 6. Output
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for row in &rows {
            println!("{}", row);
        }
    }
    Ok(())
}

/// Reject anything that doesn't start with SELECT or WITH (UX guard).
/// The actual write-safety guarantee is SQLITE_OPEN_READ_ONLY + stmt.readonly().
fn validate_keyword(sql: &str) -> Result<()> {
    let upper = sql.trim().to_ascii_uppercase();
    if upper.starts_with("SELECT") || upper.starts_with("WITH") {
        return Ok(());
    }
    let preview: String = sql.chars().take(30).collect();
    bail!(
        "Only SELECT (or WITH...SELECT) queries are allowed. Got: {:?}",
        preview
    );
}

/// Convert a rusqlite row into a JSON object keyed by column name.
fn build_row(row: &rusqlite::Row, col_names: &[String]) -> serde_json::Value {
    use rusqlite::types::Value;
    let mut map = serde_json::Map::new();
    for (i, name) in col_names.iter().enumerate() {
        let v = match row.get::<_, Value>(i).unwrap_or(Value::Null) {
            Value::Null => serde_json::Value::Null,
            Value::Integer(n) => serde_json::Value::Number(n.into()),
            Value::Real(f) => serde_json::json!(f),
            Value::Text(s) => serde_json::Value::String(s),
            Value::Blob(b) => serde_json::Value::String(format!("<blob {} bytes>", b.len())),
        };
        map.insert(name.clone(), v);
    }
    serde_json::Value::Object(map)
}
