use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::{embed::Embedder, store};

/// Batch size for re-embedding during migration.
const EMBED_BATCH_SIZE: usize = 64;

/// Progress report interval (rows).
const PROGRESS_INTERVAL: usize = 1000;

/// Migrate the Python recall database into the Rust schema.
///
/// The Python DB uses:
/// - Table `drawers` (id, content, embedding, wing, room, type, source, source_file, created_at, title)
/// - Embeddings: bge-base-en-v1.5 (768-dim) — incompatible with Rust's bge-small (384-dim)
/// - Table `sources` (path, wing, content_hash, file_size, last_indexed_at, chunk_count)
///
/// Strategy: migrate all text content + metadata, discard old embeddings, re-embed with bge-small.
pub fn run_migrate(source_path: &str, batch_embed: bool) -> Result<()> {
    let source = Path::new(source_path);
    if !source.exists() {
        anyhow::bail!("source database not found: {}", source_path);
    }

    let src_conn = Connection::open(source)
        .with_context(|| format!("opening source database: {}", source_path))?;

    // Validate it's a Python recall DB
    validate_source(&src_conn)?;

    let dst_conn = store::open_db()?;

    let total_rows = count_drawers(&src_conn)?;
    eprintln!("  Source: {} ({} drawers)", source_path, total_rows);

    if batch_embed {
        eprintln!("  Mode: full migration with re-embedding (bge-small-en-v1.5, 384-dim)");
        eprintln!("  Estimated time: ~{} minutes", total_rows / 210 / 60 + 1);
        let embedder = Embedder::new()?;
        migrate_with_embeddings(&src_conn, &dst_conn, &embedder, total_rows)?;
    } else {
        eprintln!("  Mode: text-only migration (embeddings deferred)");
        migrate_text_only(&src_conn, &dst_conn, total_rows)?;
    }

    // Migrate sources → scan_cache
    migrate_sources(&src_conn, &dst_conn)?;

    let stats = store::corpus_stats(&dst_conn)?;
    eprintln!("\n  Done: {} chunks in {} wings", stats.total_chunks, stats.wings.len());
    Ok(())
}

fn validate_source(conn: &Connection) -> Result<()> {
    // Check for the drawers table
    let has_drawers: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='drawers'",
        [],
        |r| r.get(0),
    )?;
    if !has_drawers {
        anyhow::bail!("not a Python recall database (missing 'drawers' table)");
    }

    // Check embedding model from meta
    let model: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'embedding_model'",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(ref m) = model {
        eprintln!("  Source model: {} (will re-embed with bge-small-en-v1.5)", m);
    }
    Ok(())
}

fn count_drawers(conn: &Connection) -> Result<usize> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM drawers", [], |r| r.get(0))?;
    Ok(count as usize)
}

/// Migrate with immediate re-embedding (slower but produces a ready-to-search DB).
fn migrate_with_embeddings(
    src: &Connection,
    dst: &Connection,
    embedder: &Embedder,
    total: usize,
) -> Result<()> {
    let mut stmt = src.prepare(
        "SELECT content, wing, room, type, source, source_file, created_at, title FROM drawers ORDER BY id"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(SourceRow {
            content: row.get(0)?,
            wing: row.get(1)?,
            room: row.get(2)?,
            dtype: row.get(3)?,
            source: row.get(4)?,
            source_file: row.get::<_, Option<String>>(5)?,
            created_at: row.get(6)?,
            title: row.get::<_, Option<String>>(7)?,
        })
    })?;

    let start = Instant::now();
    let mut batch: Vec<SourceRow> = Vec::with_capacity(EMBED_BATCH_SIZE);
    let mut migrated = 0;

    for row_result in rows {
        let row = row_result?;
        batch.push(row);

        if batch.len() >= EMBED_BATCH_SIZE {
            flush_batch_with_embeddings(dst, embedder, &batch)?;
            migrated += batch.len();
            batch.clear();

            if migrated % PROGRESS_INTERVAL == 0 {
                report_progress(migrated, total, start.elapsed().as_secs());
            }
        }
    }

    // Final partial batch
    if !batch.is_empty() {
        flush_batch_with_embeddings(dst, embedder, &batch)?;
        migrated += batch.len();
    }

    report_progress(migrated, total, start.elapsed().as_secs());
    Ok(())
}

/// Migrate text only (fast, embeddings set to NULL for later background re-embedding).
fn migrate_text_only(src: &Connection, dst: &Connection, total: usize) -> Result<()> {
    let mut stmt = src.prepare(
        "SELECT content, wing, room, type, source, source_file, created_at, title FROM drawers ORDER BY id"
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(SourceRow {
            content: row.get(0)?,
            wing: row.get(1)?,
            room: row.get(2)?,
            dtype: row.get(3)?,
            source: row.get(4)?,
            source_file: row.get::<_, Option<String>>(5)?,
            created_at: row.get(6)?,
            title: row.get::<_, Option<String>>(7)?,
        })
    })?;

    let start = Instant::now();
    let mut migrated = 0;

    dst.execute("BEGIN", [])?;
    for row_result in rows {
        let row = row_result?;
        let source = effective_source(&row);
        let created_at = parse_created_at(&row.created_at);

        dst.execute(
            "INSERT INTO chunks (content, wing, room, type, source, created_at, embedding) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![row.content, row.wing, row.room, row.dtype, source, created_at],
        )?;
        let rowid = dst.last_insert_rowid();
        dst.execute(
            "INSERT INTO fts_chunks (rowid, content) VALUES (?1, ?2)",
            params![rowid, row.content],
        )?;

        migrated += 1;
        if migrated % 10000 == 0 {
            dst.execute("COMMIT", [])?;
            dst.execute("BEGIN", [])?;
            report_progress(migrated, total, start.elapsed().as_secs());
        }
    }
    dst.execute("COMMIT", [])?;

    report_progress(migrated, total, start.elapsed().as_secs());
    Ok(())
}

fn flush_batch_with_embeddings(
    dst: &Connection,
    embedder: &Embedder,
    batch: &[SourceRow],
) -> Result<()> {
    let texts: Vec<&str> = batch.iter().map(|r| r.content.as_str()).collect();
    let embeddings = embedder.embed_batch(&texts)?;

    dst.execute("BEGIN IMMEDIATE", [])?;
    for (row, embedding) in batch.iter().zip(embeddings.iter()) {
        let source = effective_source(row);
        store::insert_chunk(dst, &row.content, &row.wing, &row.room, &row.dtype, &source, embedding)?;
    }
    dst.execute("COMMIT", [])?;
    Ok(())
}

/// Migrate the sources table to scan_cache.
fn migrate_sources(src: &Connection, dst: &Connection) -> Result<()> {
    let count: i64 = src.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))?;
    if count == 0 {
        return Ok(());
    }

    let mut stmt = src.prepare("SELECT path, content_hash, file_size, last_indexed_at FROM sources")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;

    dst.execute("BEGIN", [])?;
    let mut migrated = 0;
    for row_result in rows {
        let (path, hash, size, last_indexed) = row_result?;
        let file_size = size.unwrap_or(0);
        // Use last_indexed_at as mtime (best available approximation)
        store::set_scan_entry(dst, &path, last_indexed, file_size, &hash)?;
        migrated += 1;
    }
    dst.execute("COMMIT", [])?;
    eprintln!("  Migrated {} source entries to scan_cache", migrated);
    Ok(())
}

fn effective_source(row: &SourceRow) -> String {
    // Prefer source_file if available, fall back to source field
    row.source_file
        .as_deref()
        .unwrap_or(&row.source)
        .to_string()
}

/// Parse ISO 8601 created_at to unix epoch.
fn parse_created_at(text: &str) -> i64 {
    // Format: "2026-07-14T08:46:52-0700"
    // Try chrono-less parsing: extract components
    parse_iso8601_to_epoch(text).unwrap_or(0)
}

fn parse_iso8601_to_epoch(s: &str) -> Option<i64> {
    // "2026-07-14T08:46:52-0700" or "2026-07-14T08:46:52-07:00"
    if s.len() < 19 {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    let hour: i64 = s[11..13].parse().ok()?;
    let min: i64 = s[14..16].parse().ok()?;
    let sec: i64 = s[17..19].parse().ok()?;

    // Parse timezone offset
    let tz_offset_secs = if s.len() > 19 {
        parse_tz_offset(&s[19..])
    } else {
        0
    };

    // Days from epoch (simplified — good enough for dates 2020-2030)
    let days = days_from_epoch(year, month, day)?;
    let epoch_secs = days * 86400 + hour * 3600 + min * 60 + sec - tz_offset_secs;
    Some(epoch_secs)
}

fn parse_tz_offset(s: &str) -> i64 {
    // "+0700", "-0700", "+07:00", "-07:00", "Z"
    let s = s.trim();
    if s.is_empty() || s == "Z" {
        return 0;
    }
    let sign: i64 = if s.starts_with('-') { -1 } else { 1 };
    let digits: String = s[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 4 {
        let hours: i64 = digits[0..2].parse().unwrap_or(0);
        let mins: i64 = digits[2..4].parse().unwrap_or(0);
        sign * (hours * 3600 + mins * 60)
    } else if digits.len() >= 2 {
        let hours: i64 = digits[0..2].parse().unwrap_or(0);
        sign * hours * 3600
    } else {
        0
    }
}

fn days_from_epoch(year: i64, month: i64, day: i64) -> Option<i64> {
    // Simplified days-from-Unix-epoch calculation
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let m = month;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468; // offset from 1970-01-01
    Some(days)
}

fn report_progress(migrated: usize, total: usize, elapsed_secs: u64) {
    let pct = (migrated as f64 / total as f64 * 100.0) as u32;
    let rate = if elapsed_secs > 0 {
        migrated as f64 / elapsed_secs as f64
    } else {
        0.0
    };
    let remaining = if rate > 0.0 {
        (total - migrated) as f64 / rate
    } else {
        0.0
    };
    eprint!(
        "\r  Progress: {}/{} ({}%) — {:.0}/sec — ~{:.0}s remaining    ",
        migrated, total, pct, rate, remaining
    );
}

#[allow(dead_code)] // preserved for future title-prepend feature
struct SourceRow {
    content: String,
    wing: String,
    room: String,
    dtype: String,
    source: String,
    source_file: Option<String>,
    created_at: String,
    title: Option<String>,
}
