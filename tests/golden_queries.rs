//! Golden query regression tests — prevents search quality degradation.
//! Uses a frozen 15-chunk corpus with known-good query→result expectations.

mod common;

use recall::{search, store};
use tempfile::TempDir;

/// Seed a golden corpus of diverse content for search quality testing.
fn setup_golden_corpus() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("golden.sqlite3");
    std::env::set_var("RECALL_DB", &db_path);
    let conn = store::open_db().unwrap();
    let embedder = common::shared_embedder();

    let corpus: &[(&str, &str, &str)] = &[
        // (content, wing, room)
        ("We decided to use JWT tokens with 15-minute expiry and refresh token rotation for authentication", "web_app", "decisions"),
        ("The scan cache stores mtime and file size per file to detect changes without reading content", "recall", "architecture"),
        ("We chose Rust over Go because fastembed-rs provides native local embeddings without CGO", "recall", "decisions"),
        ("FTS5 provides BM25 ranking for keyword search while vector cosine handles semantic similarity", "recall", "technical"),
        ("The RRF fusion algorithm combines BM25 and vector ranks using score = 1/(k + rank)", "recall", "technical"),
        ("Database migrations read the Python recall schema and copy drawers to the Rust chunks table", "recall", "technical"),
        ("Shader compilation uses a two-pass approach with intermediate SPIR-V representation", "game_engine", "technical"),
        ("The deployment pipeline runs cargo build release then strips the binary with LTO enabled", "recall", "planning"),
        ("Error handling uses anyhow for application errors and thiserror for library error types", "recall", "technical"),
        ("Integration tests use tempfile for isolated databases and OnceLock for shared model instances", "recall", "technical"),
        ("The UI component uses reactive state management with unidirectional data flow", "game_engine", "architecture"),
        ("Project milestones are tracked in PLAN.md with tickets decomposed into vertical slices", "recall", "planning"),
        ("Game save files use a versioned binary format with forward-compatible field tags", "game_engine", "technical"),
        ("The parallel file scanner uses jwalk for multi-threaded directory walking at 42ms for 2600 files", "recall", "technical"),
        ("Code reviews check for the nine banned patterns including redundant defensive checks and catch-everything", "recall", "decisions"),
    ];

    for (content, wing, room) in corpus {
        let emb = embedder.embed_one(content).unwrap();
        store::insert_chunk(&conn, content, wing, room, "fact", "golden", &emb).unwrap();
    }

    (dir, conn)
}

/// Assert that at least one result in top-K contains at least one of the expected keywords.
fn assert_relevant_in_top_k(results: &[store::SearchResult], keywords: &[&str], query: &str) {
    let found = results.iter().any(|r| {
        let lower = r.content.to_lowercase();
        keywords.iter().any(|kw| lower.contains(kw))
    });
    assert!(
        found,
        "Query '{}' should find content with keywords {:?} in top-{}, got: {:?}",
        query,
        keywords,
        results.len(),
        results
            .iter()
            .map(|r| &r.content[..r.content.len().min(60)])
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// Golden queries — each verifies a specific retrieval expectation
// =============================================================================

#[test]
fn golden_authentication_decision() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results = search::hybrid_search(
        &conn,
        embedder,
        "what did we decide about authentication",
        None,
        5,
    )
    .unwrap();
    assert_relevant_in_top_k(
        &results,
        &["jwt", "token", "auth", "refresh"],
        "authentication",
    );
}

#[test]
fn golden_rust_choice() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results =
        search::hybrid_search(&conn, embedder, "why did we choose Rust", None, 5).unwrap();
    assert_relevant_in_top_k(&results, &["rust", "go", "fastembed"], "rust choice");
}

#[test]
fn golden_scan_cache() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results = search::hybrid_search(
        &conn,
        embedder,
        "how does the scan cache detect file changes",
        None,
        5,
    )
    .unwrap();
    assert_relevant_in_top_k(&results, &["scan", "cache", "mtime", "size"], "scan cache");
}

#[test]
fn golden_search_algorithm() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results = search::hybrid_search(
        &conn,
        embedder,
        "how does hybrid search combine BM25 and vector results",
        None,
        5,
    )
    .unwrap();
    assert_relevant_in_top_k(
        &results,
        &["rrf", "bm25", "vector", "fusion", "rank"],
        "search algorithm",
    );
}

#[test]
fn golden_error_handling() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results =
        search::hybrid_search(&conn, embedder, "error handling approach", None, 5).unwrap();
    assert_relevant_in_top_k(
        &results,
        &["anyhow", "error", "thiserror"],
        "error handling",
    );
}

#[test]
fn golden_file_scanning() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results = search::hybrid_search(
        &conn,
        embedder,
        "parallel file scanning performance",
        None,
        5,
    )
    .unwrap();
    assert_relevant_in_top_k(
        &results,
        &["jwalk", "parallel", "scan", "42ms"],
        "file scanning",
    );
}

#[test]
fn golden_deployment() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results =
        search::hybrid_search(&conn, embedder, "how to build release binary", None, 5).unwrap();
    assert_relevant_in_top_k(
        &results,
        &["cargo", "release", "strip", "lto", "deploy"],
        "deployment",
    );
}

#[test]
fn golden_shader() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results =
        search::hybrid_search(&conn, embedder, "shader compilation pipeline", None, 5).unwrap();
    assert_relevant_in_top_k(&results, &["shader", "spir", "compilation"], "shader");
}

#[test]
fn golden_wing_scoping() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    // Search within game_engine wing should only return game content
    let results = search::hybrid_search(
        &conn,
        embedder,
        "technical implementation",
        Some("game_engine"),
        5,
    )
    .unwrap();
    assert!(
        results.iter().all(|r| r.wing == "game_engine"),
        "wing-scoped search should only return that wing's results"
    );
}

#[test]
fn golden_database_migration() {
    let (_dir, conn) = setup_golden_corpus();
    let embedder = common::shared_embedder();
    let results =
        search::hybrid_search(&conn, embedder, "migrating from Python database", None, 5).unwrap();
    assert_relevant_in_top_k(
        &results,
        &["migration", "python", "schema", "drawers"],
        "migration",
    );
}
