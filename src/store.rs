use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Serialize;

/// Open the recall database (creating if needed), with WAL mode.
pub fn open_db() -> Result<Connection> {
    let path = db_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let conn = Connection::open(&path)
        .with_context(|| format!("opening database at {}", path.display()))?;

    conn.execute_batch("
        PRAGMA journal_mode=WAL;
        PRAGMA busy_timeout=5000;
        PRAGMA synchronous=NORMAL;
    ")?;
    init_schema(&conn)?;
    Ok(conn)
}

fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("RECALL_DB") {
        return PathBuf::from(p);
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".recall").join("recall.sqlite3")
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS chunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            wing TEXT NOT NULL,
            room TEXT NOT NULL DEFAULT 'general',
            type TEXT NOT NULL DEFAULT 'session',
            source TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            embedding BLOB
        );

        CREATE INDEX IF NOT EXISTS idx_chunks_wing ON chunks(wing);
        CREATE INDEX IF NOT EXISTS idx_chunks_source ON chunks(source);

        CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
            content,
            content_rowid=id
        );

        CREATE TABLE IF NOT EXISTS scan_cache (
            path TEXT PRIMARY KEY,
            mtime INTEGER NOT NULL,
            size INTEGER NOT NULL,
            content_hash TEXT NOT NULL
        );
    ")?;
    Ok(())
}

/// Insert a chunk with its embedding.
pub fn insert_chunk(
    conn: &Connection,
    content: &str,
    wing: &str,
    room: &str,
    dtype: &str,
    source: &str,
    embedding: &[f32],
) -> Result<i64> {
    let embedding_bytes = embedding_to_bytes(embedding);
    conn.execute(
        "INSERT INTO chunks (content, wing, room, type, source, embedding) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![content, wing, room, dtype, source, embedding_bytes],
    )?;
    let rowid = conn.last_insert_rowid();

    // Update FTS index
    conn.execute(
        "INSERT INTO fts_chunks (rowid, content) VALUES (?1, ?2)",
        params![rowid, content],
    )?;

    Ok(rowid)
}

/// BM25 search via FTS5.
pub fn bm25_search(conn: &Connection, query: &str, wing: Option<&str>, limit: usize) -> Result<Vec<SearchResult>> {
    let mut results = Vec::new();

    if let Some(w) = wing {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.content, c.wing, c.room, c.type, c.source, c.embedding, rank AS score
             FROM fts_chunks f JOIN chunks c ON c.id = f.rowid
             WHERE fts_chunks MATCH ?1 AND c.wing = ?2
             ORDER BY rank LIMIT ?3"
        )?;
        let rows = stmt.query_map(params![query, w, limit * 3], map_search_row)?;
        for r in rows { if let Ok(v) = r { results.push(v); } }
    } else {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.content, c.wing, c.room, c.type, c.source, c.embedding, rank AS score
             FROM fts_chunks f JOIN chunks c ON c.id = f.rowid
             WHERE fts_chunks MATCH ?1
             ORDER BY rank LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![query, limit * 3], map_search_row)?;
        for r in rows { if let Ok(v) = r { results.push(v); } }
    }

    Ok(results)
}

/// Get all embeddings for vector search (brute-force for now).
pub fn all_embeddings(conn: &Connection, wing: Option<&str>) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut results = Vec::new();

    if let Some(w) = wing {
        let mut stmt = conn.prepare(
            "SELECT id, embedding FROM chunks WHERE embedding IS NOT NULL AND wing = ?1"
        )?;
        let rows = stmt.query_map(params![w], |row| {
            let id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, bytes_to_embedding(&blob)))
        })?;
        for r in rows { if let Ok(v) = r { results.push(v); } }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, embedding FROM chunks WHERE embedding IS NOT NULL"
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, bytes_to_embedding(&blob)))
        })?;
        for r in rows { if let Ok(v) = r { results.push(v); } }
    }

    Ok(results)
}

/// Get a chunk by id.
pub fn get_chunk(conn: &Connection, id: i64) -> Result<SearchResult> {
    let mut stmt = conn.prepare(
        "SELECT id, content, wing, room, type, source, embedding, 0.0 FROM chunks WHERE id = ?1"
    )?;
    let result = stmt.query_row(params![id], map_search_row)?;
    Ok(result)
}

/// Recent agent-written facts.
pub fn recent_agent_facts(conn: &Connection, limit: usize) -> Result<Vec<ChunkInfo>> {
    let mut stmt = conn.prepare(
        "SELECT content, wing, room, type FROM chunks WHERE source = 'agent' ORDER BY created_at DESC LIMIT ?1"
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(ChunkInfo {
            content: row.get(0)?,
            wing: row.get(1)?,
            room: row.get(2)?,
            dtype: row.get(3)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Corpus statistics.
pub fn corpus_stats(conn: &Connection) -> Result<CorpusStats> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
    let mut stmt = conn.prepare("SELECT wing, COUNT(*) FROM chunks GROUP BY wing ORDER BY wing")?;
    let wings: Vec<(String, i64)> = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?.filter_map(|r| r.ok()).collect();

    Ok(CorpusStats { total_chunks: total, wings })
}

/// Delete all chunks in a wing.
pub fn delete_wing(conn: &Connection, wing: &str) -> Result<usize> {
    // Delete FTS entries first
    conn.execute(
        "DELETE FROM fts_chunks WHERE rowid IN (SELECT id FROM chunks WHERE wing = ?1)",
        params![wing],
    )?;
    let deleted = conn.execute("DELETE FROM chunks WHERE wing = ?1", params![wing])?;
    Ok(deleted)
}

// --- Scan cache ---

pub fn get_scan_entry(conn: &Connection, path: &str) -> Result<Option<(i64, i64, String)>> {
    let mut stmt = conn.prepare("SELECT mtime, size, content_hash FROM scan_cache WHERE path = ?1")?;
    let result = stmt.query_row(params![path], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
    });
    match result {
        Ok(r) => Ok(Some(r)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set_scan_entry(conn: &Connection, path: &str, mtime: i64, size: i64, hash: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scan_cache (path, mtime, size, content_hash) VALUES (?1, ?2, ?3, ?4)",
        params![path, mtime, size, hash],
    )?;
    Ok(())
}

// --- Types ---

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: i64,
    pub content: String,
    pub wing: String,
    pub room: String,
    pub dtype: String,
    pub source: String,
    pub embedding: Option<Vec<f32>>,
    pub score: f64,
}

#[derive(Debug)]
pub struct ChunkInfo {
    pub content: String,
    pub wing: String,
    pub room: String,
    pub dtype: String,
}

#[derive(Debug, Serialize)]
pub struct CorpusStats {
    pub total_chunks: i64,
    pub wings: Vec<(String, i64)>,
}

// --- Helpers ---

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn map_search_row(row: &rusqlite::Row) -> rusqlite::Result<SearchResult> {
    let embedding_blob: Option<Vec<u8>> = row.get(6)?;
    Ok(SearchResult {
        id: row.get(0)?,
        content: row.get(1)?,
        wing: row.get(2)?,
        room: row.get(3)?,
        dtype: row.get(4)?,
        source: row.get(5)?,
        embedding: embedding_blob.map(|b| bytes_to_embedding(&b)),
        score: row.get(7)?,
    })
}
