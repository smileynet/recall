//! Wing-merge migration tests (ticket 054 Option B).
//!
//! Builds a deliberately fragmented DB (hyphen/underscore/case/dot variants of
//! the same project) and verifies that `apply_wing_migration` rewrites all three
//! places a wing is persisted — `chunks.wing`, the `import:{wing}:` source
//! prefix, and the `import_sources` composite PK — merging variants into one
//! canonical wing. Also covers idempotency and the newest-wins collision rule.

use recall::store;
use rusqlite::{params, Connection};
use tempfile::TempDir;

/// Fresh isolated DB at an explicit path (no RECALL_DB env dependency).
fn fresh_db() -> (TempDir, Connection) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("wing-migration-test.sqlite3");
    let conn = store::open_db_at(&path).unwrap();
    (dir, conn)
}

/// Insert a chunk directly (empty embedding — the migration never touches it).
fn insert(conn: &Connection, content: &str, wing: &str, source: &str) {
    conn.execute(
        "INSERT INTO chunks (content, wing, room, type, source, embedding) \
         VALUES (?1, ?2, 'general', 'session', ?3, NULL)",
        params![content, wing, source],
    )
    .unwrap();
}

fn wing_of(conn: &Connection, content: &str) -> String {
    conn.query_row(
        "SELECT wing FROM chunks WHERE content = ?1",
        params![content],
        |r| r.get(0),
    )
    .unwrap()
}

fn source_of(conn: &Connection, content: &str) -> String {
    conn.query_row(
        "SELECT source FROM chunks WHERE content = ?1",
        params![content],
        |r| r.get(0),
    )
    .unwrap()
}

fn distinct_wings(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT wing FROM chunks ORDER BY wing")
        .unwrap();
    let v: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    v
}

#[test]
fn merges_variant_wings_into_canonical() {
    let (_dir, conn) = fresh_db();
    // Three separator/case variants of the same project + one already-canonical.
    insert(&conn, "a", "sci-phoenix", "agent");
    insert(&conn, "b", "sci_phoenix", "agent");
    insert(&conn, "c", "SCI.Phoenix", "agent");
    insert(&conn, "d", "web_app", "agent"); // already canonical, must not move

    let rewritten = store::apply_wing_migration(&conn).unwrap();
    assert_eq!(rewritten, 2, "only the two non-canonical rows are rewritten");

    // All three variants now live under one canonical wing.
    assert_eq!(wing_of(&conn, "a"), "sci_phoenix");
    assert_eq!(wing_of(&conn, "b"), "sci_phoenix");
    assert_eq!(wing_of(&conn, "c"), "sci_phoenix");
    assert_eq!(wing_of(&conn, "d"), "web_app");
    assert_eq!(distinct_wings(&conn), vec!["sci_phoenix", "web_app"]);
}

#[test]
fn rewrites_import_source_prefix() {
    let (_dir, conn) = fresh_db();
    insert(&conn, "doc", "my-project", "import:my-project:README.md");

    store::apply_wing_migration(&conn).unwrap();

    assert_eq!(wing_of(&conn, "doc"), "my_project");
    // The import:{wing}: prefix must track the wing rename; the path tail stays.
    assert_eq!(source_of(&conn, "doc"), "import:my_project:README.md");
}

#[test]
fn rewrites_import_sources_manifest_no_collision() {
    let (_dir, conn) = fresh_db();
    insert(&conn, "doc", "my-project", "import:my-project:a.md");
    store::upsert_import_source(&conn, "a.md", "my-project", "hash1", 10, 1).unwrap();

    store::apply_wing_migration(&conn).unwrap();

    // Manifest row repointed to the canonical wing.
    assert_eq!(
        store::get_import_source_hash(&conn, "a.md", "my_project").unwrap(),
        Some("hash1".to_string())
    );
    assert_eq!(
        store::get_import_source_hash(&conn, "a.md", "my-project").unwrap(),
        None
    );
}

#[test]
fn manifest_collision_newest_wins() {
    let (_dir, conn) = fresh_db();
    insert(&conn, "old", "proj-x", "import:proj-x:r.md");
    insert(&conn, "new", "proj_x", "import:proj_x:r.md");
    // Same path under both variants; canonical is proj_x. Older row (proj-x)
    // must lose to the newer canonical row.
    conn.execute(
        "INSERT INTO import_sources (path, wing, content_hash, file_size, last_indexed_at, chunk_count) \
         VALUES ('r.md', 'proj-x', 'OLD', 10, 100, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO import_sources (path, wing, content_hash, file_size, last_indexed_at, chunk_count) \
         VALUES ('r.md', 'proj_x', 'NEW', 10, 200, 1)",
        [],
    )
    .unwrap();

    store::apply_wing_migration(&conn).unwrap();

    // Exactly one manifest row for (r.md, proj_x), and it's the newer NEW hash.
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM import_sources WHERE path = 'r.md'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "collision resolved to a single row");
    assert_eq!(
        store::get_import_source_hash(&conn, "r.md", "proj_x").unwrap(),
        Some("NEW".to_string()),
        "newest last_indexed_at wins"
    );
}

#[test]
fn idempotent_second_run_is_noop() {
    let (_dir, conn) = fresh_db();
    insert(&conn, "a", "sci-phoenix", "agent");
    insert(&conn, "b", "sci_phoenix", "agent");

    let first = store::apply_wing_migration(&conn).unwrap();
    assert_eq!(first, 1, "only the non-canonical sci-phoenix chunk is rewritten");

    let second = store::apply_wing_migration(&conn).unwrap();
    assert_eq!(second, 0, "second run rewrites nothing");
    assert_eq!(distinct_wings(&conn), vec!["sci_phoenix"]);
}

#[test]
fn plan_is_read_only() {
    let (_dir, conn) = fresh_db();
    insert(&conn, "a", "sci-phoenix", "agent");

    let plan = store::plan_wing_migration(&conn).unwrap();
    assert_eq!(plan.rewrites.len(), 1);
    assert_eq!(plan.rewrites[0].from, "sci-phoenix");
    assert_eq!(plan.rewrites[0].to, "sci_phoenix");
    // Planning must not mutate.
    assert_eq!(wing_of(&conn, "a"), "sci-phoenix");
}
