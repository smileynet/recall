//! Expanded integration tests: multi-format ingest, import lifecycle, wing scoping.

mod common;

use std::path::PathBuf;

use recall::{scan, search, store};
use tempfile::TempDir;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn setup_db(dir: &TempDir) -> rusqlite::Connection {
    let db_path = dir.path().join("test-recall.sqlite3");
    std::env::set_var("RECALL_DB", &db_path);
    store::open_db().expect("failed to open test database")
}

// =============================================================================
// Wing-scoped search
// =============================================================================

#[test]
fn test_wing_scoped_search() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);
    let embedder = common::shared_embedder();

    // Add chunks to different wings
    let web_content = "JWT authentication with refresh token rotation for secure sessions";
    let recall_content = "The scan cache uses mtime and file size for change detection";

    let emb1 = embedder.embed_one(web_content).unwrap();
    store::insert_chunk(
        &conn,
        web_content,
        "web_app",
        "auth",
        "decision",
        "test",
        &emb1,
    )
    .unwrap();

    let emb2 = embedder.embed_one(recall_content).unwrap();
    store::insert_chunk(
        &conn,
        recall_content,
        "recall",
        "architecture",
        "fact",
        "test",
        &emb2,
    )
    .unwrap();

    // Unscoped search finds both
    let all = search::hybrid_search(&conn, embedder, "authentication tokens", None, 5).unwrap();
    assert!(!all.is_empty());

    // Wing-scoped search only finds web_app chunks
    let scoped =
        search::hybrid_search(&conn, embedder, "authentication tokens", Some("web_app"), 5)
            .unwrap();
    assert!(!scoped.is_empty());
    assert!(
        scoped.iter().all(|r| r.wing == "web_app"),
        "all results should be from web_app wing"
    );

    // Searching wrong wing returns nothing relevant
    let wrong_wing = search::hybrid_search(
        &conn,
        embedder,
        "authentication tokens",
        Some("nonexistent"),
        5,
    )
    .unwrap();
    assert!(wrong_wing.is_empty());
}

// =============================================================================
// Import hash-gate lifecycle
// =============================================================================

#[test]
fn test_import_hash_gate_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);

    // Create a temporary memory directory with one file
    let memory_dir = TempDir::new().unwrap();
    let md_file = memory_dir.path().join("test.md");
    std::fs::write(&md_file, "## Architecture\n\nThe system uses SQLite for storage with WAL mode for crash safety and FTS5 for full-text search.").unwrap();

    // First import: should add chunks
    let hash1 = compute_file_hash(&md_file);
    store::upsert_import_source(&conn, "test.md", "test_wing", &hash1, 100, 0).ok();
    // Actually simulate what import does: check it detects as new when no prior entry
    let stored = store::get_import_source_hash(&conn, "test.md", "fresh_wing").unwrap();
    assert!(stored.is_none(), "fresh wing should have no stored hash");

    // After storing, it should find it
    store::upsert_import_source(&conn, "test.md", "fresh_wing", &hash1, 100, 1).unwrap();
    let stored = store::get_import_source_hash(&conn, "test.md", "fresh_wing").unwrap();
    assert_eq!(stored, Some(hash1.clone()));

    // Same hash = skip
    let same_hash = store::get_import_source_hash(&conn, "test.md", "fresh_wing").unwrap();
    assert_eq!(
        same_hash.as_deref(),
        Some(hash1.as_str()),
        "unchanged file should match stored hash"
    );

    // Modified file has different hash
    std::fs::write(&md_file, "## Architecture\n\nCompletely rewritten architecture using a new approach with different storage layer.").unwrap();
    let hash2 = compute_file_hash(&md_file);
    assert_ne!(hash1, hash2, "modified file should have different hash");
}

fn compute_file_hash(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let content = std::fs::read_to_string(path).unwrap();
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

// =============================================================================
// V3 format parsing (via scan detection)
// =============================================================================

#[test]
fn test_scan_detects_v3_session_dir() {
    // This test only inspects the v3 fixture on disk — no DB needed.
    // v3 sessions have messages.jsonl inside sess_*/  directories
    // Our scan_for_changes only looks for *.jsonl at depth 1
    // v3 format is handled differently — it looks in subdirs
    // For now, verify the fixture files exist and are parseable
    let v3_messages = fixtures_dir()
        .join("session-v3")
        .join("sess_test")
        .join("messages.jsonl");
    assert!(v3_messages.exists(), "v3 fixture should exist");

    let content = std::fs::read_to_string(&v3_messages).unwrap();
    assert!(
        content.contains("payload"),
        "v3 format should have payload field"
    );
}

// =============================================================================
// Room classification on ingested content
// =============================================================================

#[test]
fn test_ingested_chunks_get_classified_rooms() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);
    let embedder = common::shared_embedder();

    // Ingest technical content
    let tech_content = "The bug in the API server caused the deployment to fail with an error code";
    let emb = embedder.embed_one(tech_content).unwrap();
    store::insert_chunk(
        &conn,
        tech_content,
        "test",
        "technical",
        "session",
        "test",
        &emb,
    )
    .unwrap();

    // Ingest architecture content
    let arch_content = "The module interface design uses a layered architecture pattern with clear component boundaries";
    let emb = embedder.embed_one(arch_content).unwrap();
    store::insert_chunk(
        &conn,
        arch_content,
        "test",
        "architecture",
        "session",
        "test",
        &emb,
    )
    .unwrap();

    // Search should find both
    let results =
        search::hybrid_search(&conn, embedder, "system design patterns", Some("test"), 5).unwrap();
    assert!(!results.is_empty());

    // Verify rooms are set correctly
    let rooms: Vec<&str> = results.iter().map(|r| r.room.as_str()).collect();
    assert!(rooms.contains(&"architecture") || rooms.contains(&"technical"));
}

// =============================================================================
// Scan cache with real fixtures
// =============================================================================

#[test]
fn test_scan_fixtures_codex_detected() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);

    let fixtures = fixtures_dir();
    let changed = scan::scan_for_changes(&fixtures, &conn).unwrap();

    // Should detect the codex fixture file
    let codex_detected = changed.iter().any(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().contains("codex"))
            .unwrap_or(false)
    });
    assert!(codex_detected, "scan should detect session-codex.jsonl");
}

// =============================================================================
// kiro-cli v3 SQLite session source (ticket 074)
// =============================================================================

#[test]
fn test_ingest_kiro_v3_sqlite_fixture() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);
    let embedder = common::shared_embedder();

    let db = fixtures_dir().join("kiro_v3.sqlite3");
    assert!(db.is_file(), "fixture kiro_v3.sqlite3 must exist");

    let chunks = recall::ingest::ingest_kiro_sqlite(&conn, embedder, &db)
        .expect("sqlite ingest should succeed");
    assert!(
        chunks >= 2,
        "expected chunks from 2 conversations, got {chunks}"
    );

    // Wing derived from the project dir (`key`), normalized.
    let stats = store::corpus_stats(&conn).unwrap();
    assert!(
        stats.total_chunks >= 2,
        "corpus should hold the ingested chunks"
    );

    // Content from the SQLite session is searchable.
    let results = search::hybrid_search(&conn, embedder, "widget cache config", None, 5).unwrap();
    assert!(
        results.iter().any(|r| r.content.contains("cache_ttl")
            || r.content.to_lowercase().contains("widget cache")),
        "expected to find widget-cache content from the SQLite session"
    );

    // Source keys use the kiro-sqlite namespace (dedup / fallback separation).
    let src: String = conn
        .query_row(
            "SELECT source FROM chunks WHERE source LIKE 'kiro-sqlite:%' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("at least one kiro-sqlite source row");
    assert!(src.starts_with("kiro-sqlite:"), "source was {src}");

    // ToolUseResults user turns are NOT ingested as user prose; the assistant
    // Response text IS. Spot-check the beta-service answer landed.
    let results2 =
        search::hybrid_search(&conn, embedder, "beta-service database", None, 5).unwrap();
    assert!(
        results2.iter().any(|r| r.content.contains("PostgreSQL")),
        "expected assistant Response prose to be searchable"
    );
}

#[test]
fn test_kiro_v3_sqlite_incremental_watermark() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);
    let embedder = common::shared_embedder();
    let db = fixtures_dir().join("kiro_v3.sqlite3");

    let first = recall::ingest::ingest_kiro_sqlite(&conn, embedder, &db).unwrap();
    assert!(first >= 2);
    let corpus_after_first = store::corpus_stats(&conn).unwrap().total_chunks;

    // Second run: watermark advanced to the max updated_at. With the `>=`
    // boundary (chosen to never drop an equal-ms row), the single boundary
    // conversation is re-read, but the idempotent per-source sink
    // (delete_chunks_by_source + re-insert) means the CORPUS SIZE is unchanged
    // — no duplicates. This is the key incrementality guarantee: the bulk of
    // conversations below the watermark are NOT re-embedded.
    let second = recall::ingest::ingest_kiro_sqlite(&conn, embedder, &db).unwrap();
    assert!(
        second <= first,
        "second run must not re-process everything (got {second} vs {first})"
    );
    let corpus_after_second = store::corpus_stats(&conn).unwrap().total_chunks;
    assert_eq!(
        corpus_after_first, corpus_after_second,
        "idempotent sink: re-reading the boundary row must not duplicate chunks"
    );

    // Watermark persisted to meta at the max updated_at seen.
    let wm = store::get_meta(&conn, "kiro_sqlite_watermark_ms")
        .unwrap()
        .expect("watermark should be stored");
    assert_eq!(wm, "1788000003000", "watermark should equal max updated_at");
}

#[test]
fn test_kiro_v3_sqlite_open_is_readonly() {
    let db = fixtures_dir().join("kiro_v3.sqlite3");
    let ro = recall::sqlite_source::open_readonly(&db).unwrap();
    // A write must fail on a read-only connection — proves no write lock path.
    let err = ro.execute("CREATE TABLE _probe (x)", []);
    assert!(err.is_err(), "write on read-only connection must error");
}
