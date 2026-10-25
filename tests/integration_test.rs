//! Integration tests for recall's core path:
//! - add + search round-trip (hybrid RRF fusion)
//! - ingest from fixture session files
//! - scan cache hit/miss behavior

use std::path::PathBuf;

use recall::{embed, scan, search, store};
use tempfile::TempDir;

/// Set RECALL_DB to a temp file and open the database.
fn setup_db(dir: &TempDir) -> rusqlite::Connection {
    let db_path = dir.path().join("test-recall.sqlite3");
    std::env::set_var("RECALL_DB", &db_path);
    store::open_db().expect("failed to open test database")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

// =============================================================================
// Test: add + search round-trip
// =============================================================================

#[test]
fn test_add_and_search_round_trip() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);
    let embedder = embed::Embedder::new().expect("failed to load embedding model");

    // Add several facts to different wings
    let facts = [
        (
            "We decided to use Rust for the rebuild because of fastembed-rs",
            "recall",
            "decisions",
            "decision",
        ),
        (
            "The scan cache uses mtime plus size for fast change detection",
            "recall",
            "architecture",
            "fact",
        ),
        (
            "Authentication uses JWT with 15-minute expiry tokens",
            "web-app",
            "auth",
            "decision",
        ),
        (
            "Database migrations run on startup with embedded SQL files",
            "web-app",
            "infra",
            "fact",
        ),
    ];

    for (content, wing, room, dtype) in &facts {
        let embedding = embedder.embed_one(content).expect("embedding failed");
        store::insert_chunk(&conn, content, wing, room, dtype, "agent", &embedding)
            .expect("insert failed");
    }

    // Verify corpus count
    let stats = store::corpus_stats(&conn).unwrap();
    assert_eq!(stats.total_chunks, 4, "should have 4 chunks stored");

    // Hybrid search — query about Rust rebuild should find the relevant fact
    let results = search::hybrid_search(&conn, &embedder, "why did we choose Rust", None, 5)
        .expect("search failed");
    assert!(!results.is_empty(), "search should return results");
    assert!(
        results[0].content.contains("Rust"),
        "top result should mention Rust, got: {}",
        results[0].content
    );

    // Scoped search — wing filter should restrict results
    let scoped = search::hybrid_search(
        &conn,
        &embedder,
        "authentication tokens",
        Some("web-app"),
        5,
    )
    .expect("scoped search failed");
    assert!(!scoped.is_empty(), "scoped search should return results");
    assert_eq!(
        scoped[0].wing, "web-app",
        "scoped results should be in web-app wing"
    );
    assert!(
        scoped[0].content.contains("JWT"),
        "top result should mention JWT"
    );

    // BM25 keyword search
    let bm25 = store::bm25_search(&conn, "mtime", None, 5).unwrap();
    assert!(!bm25.is_empty(), "BM25 should find 'mtime' keyword");
    assert!(bm25[0].content.contains("mtime"));
}

// =============================================================================
// Test: ingest from fixture files
// =============================================================================

#[test]
fn test_ingest_from_fixtures() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);
    let embedder = embed::Embedder::new().expect("failed to load embedding model");

    let fixtures = fixtures_dir();

    // Scan should find fixture JSONL files as "changed" (no prior cache)
    let changed = scan::scan_for_changes(&fixtures, &conn).expect("scan_for_changes failed");
    assert_eq!(
        changed.len(),
        3,
        "should detect 3 fixture JSONL files as new"
    );

    // Manually ingest: read each file, chunk, embed, store (mirrors ingest.rs logic)
    let mut total_chunks = 0;
    for file_path in &changed {
        let content = std::fs::read_to_string(file_path).unwrap();
        let mut chunks = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            // Extract content field from JSONL
            if let Some(start) = line.find("\"content\":\"") {
                let rest = &line[start + 11..];
                if let Some(end) = find_unescaped_quote(rest) {
                    let text = &rest[..end];
                    let unescaped = text.replace("\\n", "\n").replace("\\\"", "\"");
                    if unescaped.len() > 50 {
                        chunks.push(unescaped);
                    }
                }
            }
        }

        let wing = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "test".to_string());

        let texts: Vec<&str> = chunks.iter().map(|c| c.as_str()).collect();
        if texts.is_empty() {
            continue;
        }
        let embeddings = embedder.embed_batch(&texts).unwrap();

        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            store::insert_chunk(
                &conn,
                chunk,
                &wing,
                "general",
                "session",
                &file_path.to_string_lossy(),
                embedding,
            )
            .unwrap();
        }

        // Update scan cache
        scan::update_cache(&conn, file_path).unwrap();
        total_chunks += chunks.len();
    }

    assert!(
        total_chunks >= 5,
        "should have ingested at least 5 chunks from fixtures, got {}",
        total_chunks
    );

    // Verify chunks are searchable
    let results =
        search::hybrid_search(&conn, &embedder, "fastembed embeddings Rust", None, 5).unwrap();
    assert!(!results.is_empty(), "ingested content should be searchable");
    assert!(
        results[0].content.contains("fastembed") || results[0].content.contains("Rust"),
        "top result should be relevant to the query"
    );

    // Verify wing was derived from filename
    let stats = store::corpus_stats(&conn).unwrap();
    assert!(
        stats
            .wings
            .iter()
            .any(|(w, _)| w == "session-001" || w == "session-002"),
        "should have wings derived from filenames, got: {:?}",
        stats.wings
    );
}

/// Find the position of the first unescaped `"` in a string.
fn find_unescaped_quote(s: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, ch) in s.chars().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return Some(i);
        }
    }
    None
}

// =============================================================================
// Test: scan cache hit/miss
// =============================================================================

#[test]
fn test_scan_cache_hit_miss() {
    let tmp = TempDir::new().unwrap();
    let conn = setup_db(&tmp);

    let fixtures = fixtures_dir();

    // First scan: all files should be detected as changed (cache miss)
    let first_scan = scan::scan_for_changes(&fixtures, &conn).expect("first scan failed");
    assert_eq!(
        first_scan.len(),
        3,
        "first scan should detect all JSONL files as new"
    );

    // Update cache for all detected files
    for path in &first_scan {
        scan::update_cache(&conn, path).expect("update_cache failed");
    }

    // Second scan: no files should be detected (cache hit)
    let second_scan = scan::scan_for_changes(&fixtures, &conn).expect("second scan failed");
    assert_eq!(
        second_scan.len(),
        0,
        "second scan should detect no changes (cache hit)"
    );

    // Verify cache entries exist
    for path in &first_scan {
        let entry =
            store::get_scan_entry(&conn, &path.to_string_lossy()).expect("get_scan_entry failed");
        assert!(
            entry.is_some(),
            "cache entry should exist for {}",
            path.display()
        );
        let (mtime, size, hash) = entry.unwrap();
        assert!(mtime > 0, "mtime should be positive");
        assert!(size > 0, "size should be positive");
        assert!(!hash.is_empty(), "hash should not be empty");
    }

    // Simulate file modification by writing a new temp file and scanning that dir
    let scan_tmp = TempDir::new().unwrap();
    let new_file = scan_tmp.path().join("session-new.jsonl");
    std::fs::write(&new_file, r#"{"role":"user","content":"This is a brand new session file that was just created for testing change detection"}"#).unwrap();

    // Scan the temp dir — should find the new file
    let new_scan = scan::scan_for_changes(scan_tmp.path(), &conn).expect("new dir scan failed");
    assert_eq!(
        new_scan.len(),
        1,
        "should detect new file in fresh directory"
    );
    assert_eq!(new_scan[0], new_file);

    // Cache it, then modify
    scan::update_cache(&conn, &new_file).unwrap();

    // Verify cached
    let cached_scan = scan::scan_for_changes(scan_tmp.path(), &conn).unwrap();
    assert_eq!(cached_scan.len(), 0, "should be cached after update_cache");

    // Modify the file (changes mtime and size)
    std::thread::sleep(std::time::Duration::from_millis(100)); // ensure mtime changes
    std::fs::write(&new_file, r#"{"role":"user","content":"This file has been modified with additional content to trigger change detection via mtime and size difference"}
{"role":"assistant","content":"I can see the file was modified. The scan cache should detect this change because the mtime and file size have both changed."}"#).unwrap();

    // Should detect the modification
    let modified_scan =
        scan::scan_for_changes(scan_tmp.path(), &conn).expect("modified scan failed");
    assert_eq!(
        modified_scan.len(),
        1,
        "should detect modified file (cache miss after change)"
    );
}
