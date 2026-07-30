//! Shared test helpers: embedder singleton, DB factories, seed data.

use std::sync::OnceLock;

use recall::embed::Embedder;
use recall::store;
use tempfile::TempDir;

static EMBEDDER: OnceLock<Embedder> = OnceLock::new();

/// Shared embedder — loads model once per test binary process (~500ms first call).
#[allow(dead_code)]
pub fn shared_embedder() -> &'static Embedder {
    EMBEDDER.get_or_init(|| Embedder::new().unwrap())
}

/// Create a fresh isolated test DB in a temp directory.
#[allow(dead_code)]
pub fn test_db() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test-recall.sqlite3");
    std::env::set_var("RECALL_DB", &db_path);
    let conn = store::open_db().unwrap();
    (dir, conn)
}

/// Create a seeded DB with a few embedded chunks for search/status/health tests.
#[allow(dead_code)]
pub fn seeded_db() -> (TempDir, rusqlite::Connection) {
    let (dir, conn) = test_db();
    let embedder = shared_embedder();

    let facts: &[(&str, &str, &str, &str)] = &[
        ("Authentication uses JWT with 15-minute expiry and refresh token rotation", "web_app", "decisions", "decision"),
        ("The scan cache uses mtime plus file size for fast change detection before hashing", "recall", "architecture", "fact"),
        ("We chose Rust for the rebuild because fastembed-rs gives native local embeddings", "recall", "decisions", "decision"),
        ("Database schema uses FTS5 for BM25 keyword search with WAL mode for crash safety", "recall", "technical", "fact"),
        ("Shader compilation uses a two-pass approach with an intermediate SPIR-V representation", "game_engine", "technical", "fact"),
    ];

    for (content, wing, room, dtype) in facts {
        let emb = embedder.embed_one(content).unwrap();
        store::insert_chunk(&conn, content, wing, room, dtype, "agent", &emb).unwrap();
    }

    (dir, conn)
}
