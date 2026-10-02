//! Read-only source for kiro-cli v3 sessions stored in SQLite.
//!
//! kiro-cli 2.27.0+ (v3 engine, default) stores conversations in a SQLite
//! database (`data.sqlite3`) instead of (only) the legacy JSONL tree. This
//! module locates that database per-platform, opens it **read-only** (the file
//! is actively written by a running kiro-cli — recall must never take a write
//! lock on it), and reads changed rows from `conversations_v2`.
//!
//! Parsing of each row's `value` JSON into messages lives in `ingest.rs`
//! alongside the other session parsers (`parse_kiro_v3_sqlite`), because the
//! `Message` model is private there. This module stays concerned with
//! *locating* and *reading* the foreign database only.
//!
//! Design notes (see .memory/adr/0001, 0002):
//! - Open with `OpenFlags::SQLITE_OPEN_READ_ONLY`. Do NOT reuse
//!   `store::open_db_at` — that opens read-write and runs `init_schema`.
//! - Never set `immutable=1`: the DB is live, an immutable open would pin a
//!   stale/corrupt snapshot.
//! - Incremental reads use `updated_at >= watermark` (unix ms). The `>=`
//!   (not `>`) is deliberate: unix-ms collisions on the boundary row are
//!   common, and a strict `>` would silently drop rows sharing the max ms.
//!   The ingest sink is idempotent per-source, so the 1-row re-read is free.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

/// A row from `conversations_v2` that needs (re-)ingesting.
pub struct ConversationRow {
    /// Session cwd / project dir, e.g. "/home/me/code/foo". Wing signal.
    pub key: String,
    /// UUID. Used to form the dedup `source` key `kiro-sqlite:<conversation_id>`.
    pub conversation_id: String,
    /// The conversation serialized as JSON (parsed by `ingest.rs`).
    pub value: String,
    /// Unix milliseconds. Drives the incremental watermark.
    pub updated_at: i64,
}

/// Resolve the platform directory containing `data.sqlite3`.
///
/// - Linux: `$XDG_DATA_HOME/kiro-cli` else `$HOME/.local/share/kiro-cli`
/// - macOS: `$HOME/Library/Application Support/kiro-cli`
/// - Windows: `%LOCALAPPDATA%\kiro-cli` (best-effort; see logged gap in ticket 074)
fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("kiro-cli"),
        )
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA").ok()?;
        Some(PathBuf::from(base).join("kiro-cli"))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("kiro-cli"));
            }
        }
        let home = std::env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("kiro-cli"),
        )
    }
}

/// Full path to the kiro-cli session database, if the platform dir resolves.
/// Does not check existence — callers use [`db_path_if_present`] for that.
pub fn db_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("data.sqlite3"))
}

/// Path to `data.sqlite3` only if it exists on disk. This is the "prefer SQLite
/// when present" detection signal (ticket 074 AC: SQLite preferred when present).
pub fn db_path_if_present() -> Option<PathBuf> {
    db_path().filter(|p| p.is_file())
}

/// Open the kiro-cli session database **read-only**.
///
/// Uses `SQLITE_OPEN_READ_ONLY` so no write lock is ever taken on the live DB.
/// A modest `busy_timeout` guards against the brief `SQLITE_BUSY` window during
/// the writer's WAL checkpoint. Never runs schema init.
pub fn open_readonly(path: &std::path::Path) -> Result<Connection> {
    let conn =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).with_context(|| {
            format!(
                "opening kiro-cli session DB read-only at {}",
                path.display()
            )
        })?;
    // Reader insurance against checkpoint-window contention. Setting a pragma
    // is not a write to the DB file.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// Read conversations with `updated_at >= since_ms`, oldest first.
///
/// `since_ms` is the stored watermark (0 on first run). The `>=` is intentional
/// (see module docs): it re-reads the boundary row rather than risk dropping a
/// row that shares the max millisecond, relying on the idempotent sink.
pub fn read_since(conn: &Connection, since_ms: i64) -> Result<Vec<ConversationRow>> {
    let mut stmt = conn.prepare(
        "SELECT key, conversation_id, value, updated_at \
         FROM conversations_v2 \
         WHERE updated_at >= ?1 \
         ORDER BY updated_at ASC",
    )?;
    let rows = stmt
        .query_map([since_ms], |row| {
            Ok(ConversationRow {
                key: row.get(0)?,
                conversation_id: row.get(1)?,
                value: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
