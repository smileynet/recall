use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;

/// Open the recall database (creating if needed), with WAL mode.
pub fn open_db() -> Result<Connection> {
    open_db_at(&db_path())
}

/// Open a recall database at an explicit path (creating if needed), with WAL
/// mode. Used by `open_db` (via `RECALL_DB`/default) and by tests that need a
/// specific file without touching the process-global `RECALL_DB` env var.
pub fn open_db_at(path: &std::path::Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("opening database at {}", path.display()))?;

    conn.execute_batch(
        "
        PRAGMA journal_mode=WAL;
        PRAGMA busy_timeout=5000;
        PRAGMA synchronous=NORMAL;
    ",
    )?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Resolve the database path (RECALL_DB override, else ~/.recall/recall.sqlite3).
pub fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("RECALL_DB") {
        return PathBuf::from(p);
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".recall").join("recall.sqlite3")
}

/// Canonical wing name derived from an arbitrary directory or user-supplied
/// string. This is the single source of truth for wing naming — every
/// derivation site and the `--wing` CLI boundary must route through it so the
/// same project always lands in the same wing.
///
/// Rule (PEP 503-style, adapted to recall's underscore convention):
/// lowercase, map each of `-`, `.`, space to `_`, collapse runs of `_` to one,
/// trim leading/trailing `_`. An empty result maps to `"global"` (the historic
/// default for cwd-derived wings).
///
/// Idempotent: `normalize_wing(normalize_wing(x)) == normalize_wing(x)`.
pub fn normalize_wing(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_underscore = false;
    for ch in name.chars() {
        let mapped = match ch {
            '-' | '.' | ' ' | '_' => '_',
            other => other.to_ascii_lowercase(),
        };
        if mapped == '_' {
            if !prev_underscore {
                out.push('_');
                prev_underscore = true;
            }
        } else {
            out.push(mapped);
            prev_underscore = false;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "global".to_string()
    } else {
        trimmed.to_string()
    }
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
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

        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS import_sources (
            path TEXT NOT NULL,
            wing TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            file_size INTEGER NOT NULL DEFAULT 0,
            last_indexed_at INTEGER NOT NULL DEFAULT (unixepoch()),
            chunk_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (path, wing)
        );
    ",
    )?;
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

/// Insert a single chunk atomically (wraps the chunks + FTS inserts in one
/// transaction). Use from single-item callers like `recall add`; batch callers
/// that already hold a transaction must use `insert_chunk` to avoid nesting.
pub fn insert_chunk_atomic(
    conn: &Connection,
    content: &str,
    wing: &str,
    room: &str,
    dtype: &str,
    source: &str,
    embedding: &[f32],
) -> Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let rowid = insert_chunk(&tx, content, wing, room, dtype, source, embedding)?;
    tx.commit()?;
    Ok(rowid)
}

/// BM25 search via FTS5.
pub fn bm25_search(
    conn: &Connection,
    query: &str,
    wing: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let mut results = Vec::new();

    if let Some(w) = wing {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.content, c.wing, c.room, c.type, c.source, c.embedding, rank AS score
             FROM fts_chunks f JOIN chunks c ON c.id = f.rowid
             WHERE fts_chunks MATCH ?1 AND c.wing = ?2
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, w, limit * 3], map_search_row)?;
        for v in rows.flatten() {
            results.push(v);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.content, c.wing, c.room, c.type, c.source, c.embedding, rank AS score
             FROM fts_chunks f JOIN chunks c ON c.id = f.rowid
             WHERE fts_chunks MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit * 3], map_search_row)?;
        for v in rows.flatten() {
            results.push(v);
        }
    }

    Ok(results)
}

/// Get all embeddings for vector search (brute-force for now).
pub fn all_embeddings(conn: &Connection, wing: Option<&str>) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut results = Vec::new();

    if let Some(w) = wing {
        let mut stmt = conn.prepare(
            "SELECT id, embedding FROM chunks WHERE embedding IS NOT NULL AND wing = ?1",
        )?;
        let rows = stmt.query_map(params![w], |row| {
            let id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, bytes_to_embedding(&blob)))
        })?;
        for v in rows.flatten() {
            results.push(v);
        }
    } else {
        let mut stmt =
            conn.prepare("SELECT id, embedding FROM chunks WHERE embedding IS NOT NULL")?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, bytes_to_embedding(&blob)))
        })?;
        for v in rows.flatten() {
            results.push(v);
        }
    }

    Ok(results)
}

/// Get a chunk by id.
pub fn get_chunk(conn: &Connection, id: i64) -> Result<SearchResult> {
    let mut stmt = conn.prepare(
        "SELECT id, content, wing, room, type, source, embedding, 0.0 FROM chunks WHERE id = ?1",
    )?;
    let result = stmt.query_row(params![id], map_search_row)?;
    Ok(result)
}

/// Recent agent-written facts.
pub fn recent_agent_facts(
    conn: &Connection,
    wing: Option<&str>,
    limit: usize,
) -> Result<Vec<ChunkInfo>> {
    let mut results = Vec::new();

    if let Some(w) = wing {
        let mut stmt = conn.prepare(
            "SELECT content, wing, room, type, created_at FROM chunks WHERE source = 'agent' AND wing = ?1 ORDER BY created_at DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![w, limit], |row| {
            Ok(ChunkInfo {
                content: row.get(0)?,
                wing: row.get(1)?,
                room: row.get(2)?,
                dtype: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        for v in rows.flatten() {
            results.push(v);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT content, wing, room, type, created_at FROM chunks WHERE source = 'agent' ORDER BY created_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(ChunkInfo {
                content: row.get(0)?,
                wing: row.get(1)?,
                room: row.get(2)?,
                dtype: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        for v in rows.flatten() {
            results.push(v);
        }
    }

    Ok(results)
}

/// Corpus statistics.
pub fn corpus_stats(conn: &Connection) -> Result<CorpusStats> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
    let mut stmt = conn.prepare("SELECT wing, COUNT(*) FROM chunks GROUP BY wing ORDER BY wing")?;
    let wings: Vec<(String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(CorpusStats {
        total_chunks: total,
        wings,
    })
}

/// Count chunks in a wing (optionally only those older than a cutoff). Used to
/// show the impact of a `forget` before confirming.
pub fn count_wing(conn: &Connection, wing: &str, older_than: Option<i64>) -> Result<usize> {
    let count: i64 = match older_than {
        Some(cutoff) => conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE wing = ?1 AND created_at < ?2",
            params![wing, cutoff],
            |r| r.get(0),
        )?,
        None => conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE wing = ?1",
            params![wing],
            |r| r.get(0),
        )?,
    };
    Ok(count as usize)
}

/// Delete all chunks in a wing. Atomic: FTS + chunks deletes commit together.
pub fn delete_wing(conn: &Connection, wing: &str) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    // Delete FTS entries first
    tx.execute(
        "DELETE FROM fts_chunks WHERE rowid IN (SELECT id FROM chunks WHERE wing = ?1)",
        params![wing],
    )?;
    let deleted = tx.execute("DELETE FROM chunks WHERE wing = ?1", params![wing])?;
    tx.commit()?;
    Ok(deleted)
}

/// Delete chunks from a wing that are older than the given epoch timestamp.
/// Atomic: FTS + chunks deletes commit together.
pub fn delete_wing_older_than(conn: &Connection, wing: &str, cutoff_epoch: i64) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM fts_chunks WHERE rowid IN (SELECT id FROM chunks WHERE wing = ?1 AND created_at < ?2)",
        params![wing, cutoff_epoch],
    )?;
    let deleted = tx.execute(
        "DELETE FROM chunks WHERE wing = ?1 AND created_at < ?2",
        params![wing, cutoff_epoch],
    )?;
    tx.commit()?;
    Ok(deleted)
}

// --- Meta ---

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    );
    match result {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// --- Scan cache ---

pub fn get_scan_entry(conn: &Connection, path: &str) -> Result<Option<(i64, i64, String)>> {
    let mut stmt =
        conn.prepare("SELECT mtime, size, content_hash FROM scan_cache WHERE path = ?1")?;
    let result = stmt.query_row(params![path], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    });
    match result {
        Ok(r) => Ok(Some(r)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set_scan_entry(
    conn: &Connection,
    path: &str,
    mtime: i64,
    size: i64,
    hash: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scan_cache (path, mtime, size, content_hash) VALUES (?1, ?2, ?3, ?4)",
        params![path, mtime, size, hash],
    )?;
    Ok(())
}

// --- Import sources ---

/// Get the stored content hash for an import source (path + wing).
pub fn get_import_source_hash(conn: &Connection, path: &str, wing: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT content_hash FROM import_sources WHERE path = ?1 AND wing = ?2",
        params![path, wing],
        |row| row.get(0),
    );
    match result {
        Ok(h) => Ok(Some(h)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Upsert an import source entry (tracks what was imported and its hash).
pub fn upsert_import_source(
    conn: &Connection,
    path: &str,
    wing: &str,
    content_hash: &str,
    file_size: i64,
    chunk_count: i64,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO import_sources (path, wing, content_hash, file_size, last_indexed_at, chunk_count)
         VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)",
        params![path, wing, content_hash, file_size, chunk_count],
    )?;
    Ok(())
}

/// Get all import source paths for a wing.
pub fn get_import_sources_for_wing(conn: &Connection, wing: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM import_sources WHERE wing = ?1")?;
    let rows = stmt.query_map(params![wing], |row| row.get(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Delete an import source entry.
pub fn delete_import_source(conn: &Connection, path: &str, wing: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM import_sources WHERE path = ?1 AND wing = ?2",
        params![path, wing],
    )?;
    Ok(())
}

/// Delete chunks by source key.
///
/// NOT self-wrapping: the FTS + chunks deletes are two statements. Callers MUST
/// hold a transaction so the pair stays atomic (ingest wraps the main loop; the
/// force/orphan paths wrap their batch). This avoids nested-transaction errors
/// when called inside an existing `BEGIN IMMEDIATE`.
pub fn delete_chunks_by_source(conn: &Connection, source: &str) -> Result<usize> {
    // Delete FTS entries first
    conn.execute(
        "DELETE FROM fts_chunks WHERE rowid IN (SELECT id FROM chunks WHERE source = ?1)",
        params![source],
    )?;
    let deleted = conn.execute("DELETE FROM chunks WHERE source = ?1", params![source])?;
    Ok(deleted)
}

/// Delete chunks by source key prefix (LIKE 'prefix%').
/// Escapes LIKE wildcards in the prefix to prevent unintended matches.
///
/// NOT self-wrapping: callers MUST hold a transaction so the FTS + chunks
/// deletes stay atomic (see `delete_chunks_by_source`).
pub fn delete_chunks_by_source_prefix(conn: &Connection, prefix: &str) -> Result<usize> {
    let escaped = prefix
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("{}%", escaped);
    conn.execute(
        "DELETE FROM fts_chunks WHERE rowid IN (SELECT id FROM chunks WHERE source LIKE ?1 ESCAPE '\\')",
        params![pattern],
    )?;
    let deleted = conn.execute(
        "DELETE FROM chunks WHERE source LIKE ?1 ESCAPE '\\'",
        params![pattern],
    )?;
    Ok(deleted)
}

// --- Wing normalization migration (ticket 054 Option B) ---

/// A planned rewrite of one non-canonical wing into its canonical form.
#[derive(Debug, Clone, Serialize)]
pub struct WingRewrite {
    pub from: String,
    pub to: String,
    pub chunk_count: i64,
    /// import_sources manifest rows whose (path) already exists under `to`
    /// (composite-PK collision resolved by newest last_indexed_at wins).
    pub manifest_collisions: i64,
}

/// Summary of a wing-merge migration plan (no mutation performed).
#[derive(Debug, Default, Serialize)]
pub struct WingMigrationPlan {
    pub rewrites: Vec<WingRewrite>,
    pub total_chunks_affected: i64,
    pub distinct_wings_before: i64,
    pub distinct_wings_after: i64,
}

/// Compute the wing-merge plan: which stored wings differ from their canonical
/// `normalize_wing` form, and how many chunks/manifest rows each touches. Pure
/// read — performs NO mutation, so it is safe to run for a dry-run preview.
pub fn plan_wing_migration(conn: &Connection) -> Result<WingMigrationPlan> {
    let mut stmt = conn.prepare("SELECT wing, COUNT(*) FROM chunks GROUP BY wing")?;
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut plan = WingMigrationPlan::default();
    let mut canonical_set = std::collections::HashSet::new();
    for (wing, _) in &rows {
        canonical_set.insert(normalize_wing(wing));
    }
    plan.distinct_wings_before = rows.len() as i64;
    plan.distinct_wings_after = canonical_set.len() as i64;

    for (wing, chunk_count) in rows {
        let canonical = normalize_wing(&wing);
        if canonical == wing {
            continue;
        }
        // Count manifest rows that would collide on (path, canonical).
        let manifest_collisions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM import_sources src
             WHERE src.wing = ?1
               AND EXISTS (SELECT 1 FROM import_sources dst
                           WHERE dst.wing = ?2 AND dst.path = src.path)",
            params![wing, canonical],
            |r| r.get(0),
        )?;
        plan.total_chunks_affected += chunk_count;
        plan.rewrites.push(WingRewrite {
            from: wing,
            to: canonical,
            chunk_count,
            manifest_collisions,
        });
    }
    plan.rewrites.sort_by(|a, b| b.chunk_count.cmp(&a.chunk_count));
    Ok(plan)
}

/// Apply the wing-merge migration in a single transaction. Rewrites all three
/// places a wing is persisted:
///   1. `chunks.wing`
///   2. `chunks.source` — the `import:{OLD_WING}:` prefix
///   3. `import_sources` composite PK `(path, wing)` (newest-wins on collision)
///
/// Idempotent: rows already canonical are skipped; a second run is a no-op.
/// Guarded by `PRAGMA user_version` bump so a completed migration is recorded.
/// Returns the number of chunk rows whose wing was rewritten.
pub fn apply_wing_migration(conn: &Connection) -> Result<i64> {
    let plan = plan_wing_migration(conn)?;
    if plan.rewrites.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let mut rewritten = 0i64;
    for rw in &plan.rewrites {
        // 2. Rewrite the import:{old}: source prefix first (before wing column),
        //    using SQL string surgery scoped to this wing's import rows.
        let old_prefix = format!("import:{}:", rw.from);
        let new_prefix = format!("import:{}:", rw.to);
        tx.execute(
            "UPDATE chunks
             SET source = ?1 || substr(source, length(?2) + 1)
             WHERE wing = ?3 AND source LIKE ?2 || '%'",
            params![new_prefix, old_prefix, rw.from],
        )?;

        // 3. import_sources: delete rows that would collide on (path, canonical)
        //    keeping the newest by last_indexed_at, then repoint the rest.
        tx.execute(
            "DELETE FROM import_sources
             WHERE wing = ?1
               AND EXISTS (
                 SELECT 1 FROM import_sources dst
                 WHERE dst.wing = ?2 AND dst.path = import_sources.path
                   AND dst.last_indexed_at >= import_sources.last_indexed_at
               )",
            params![rw.from, rw.to],
        )?;
        // Any surviving colliding dst rows (older than the src we keep) are
        // removed so the repoint below cannot violate the PK.
        tx.execute(
            "DELETE FROM import_sources
             WHERE wing = ?2
               AND EXISTS (
                 SELECT 1 FROM import_sources src
                 WHERE src.wing = ?1 AND src.path = import_sources.path
               )",
            params![rw.from, rw.to],
        )?;
        tx.execute(
            "UPDATE import_sources SET wing = ?2 WHERE wing = ?1",
            params![rw.from, rw.to],
        )?;

        // 1. Rewrite the wing column last.
        rewritten += tx.execute(
            "UPDATE chunks SET wing = ?2 WHERE wing = ?1",
            params![rw.from, rw.to],
        )? as i64;
    }
    tx.commit()?;
    Ok(rewritten)
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
    pub created_at: i64,
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
    bytes
        .chunks_exact(4)
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

#[cfg(test)]
mod tests {
    use super::normalize_wing;

    #[test]
    fn folds_all_separators_to_underscore() {
        assert_eq!(normalize_wing("sci-phoenix"), "sci_phoenix");
        assert_eq!(normalize_wing("sci.phoenix"), "sci_phoenix");
        assert_eq!(normalize_wing("sci phoenix"), "sci_phoenix");
        assert_eq!(normalize_wing("sci_phoenix"), "sci_phoenix");
    }

    #[test]
    fn pep503_parity_all_variants_collapse_to_one() {
        // Every separator variant of the same name maps to one canonical key.
        let canonical = normalize_wing("sci_phoenix");
        for variant in ["sci-phoenix", "sci.phoenix", "sci phoenix", "SCI-Phoenix"] {
            assert_eq!(normalize_wing(variant), canonical, "variant: {variant}");
        }
    }

    #[test]
    fn lowercases() {
        assert_eq!(normalize_wing("MyApp"), "myapp");
        assert_eq!(normalize_wing("CREW-Research"), "crew_research");
    }

    #[test]
    fn collapses_runs_of_separators() {
        assert_eq!(normalize_wing("a--b"), "a_b");
        assert_eq!(normalize_wing("a-.b"), "a_b");
        assert_eq!(normalize_wing("a__b"), "a_b");
        assert_eq!(normalize_wing("a - b"), "a_b");
    }

    #[test]
    fn trims_leading_and_trailing_separators() {
        assert_eq!(normalize_wing("-web-app-"), "web_app");
        assert_eq!(normalize_wing("__x__"), "x");
        assert_eq!(normalize_wing(".hidden"), "hidden");
    }

    #[test]
    fn empty_and_separator_only_map_to_global() {
        assert_eq!(normalize_wing(""), "global");
        assert_eq!(normalize_wing("---"), "global");
        assert_eq!(normalize_wing("_._"), "global");
        assert_eq!(normalize_wing("   "), "global");
    }

    #[test]
    fn idempotent() {
        for input in ["sci-phoenix", "MyApp", "a--b", "", "---", "web_app"] {
            let once = normalize_wing(input);
            let twice = normalize_wing(&once);
            assert_eq!(once, twice, "not idempotent for: {input}");
        }
    }

    #[test]
    fn already_canonical_unchanged() {
        assert_eq!(normalize_wing("web_app"), "web_app");
        assert_eq!(normalize_wing("global"), "global");
        assert_eq!(normalize_wing("my_project"), "my_project");
    }
}
