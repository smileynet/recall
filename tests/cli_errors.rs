//! CLI error handling tests — verify correct exit codes and helpful messages.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn recall_cmd() -> Command {
    Command::cargo_bin("recall").unwrap()
}

fn with_empty_db(cmd: &mut Command) -> &mut Command {
    let dir = TempDir::new().unwrap();
    // Leak the dir so it lives through the command execution
    let db_path = dir.keep().join("empty.sqlite3");
    cmd.env("RECALL_DB", db_path)
}

#[test]
fn no_subcommand_shows_help() {
    recall_cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn add_without_wing_uses_cwd() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    // --wing is optional now (auto-detects from cwd), so this should succeed
    cmd.args(["add", "some fact"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Stored in"));
}

#[test]
fn search_missing_query() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.arg("search")
        .assert()
        .failure()
        .stderr(predicate::str::contains("<QUERY>").or(predicate::str::contains("required")));
}

#[test]
fn ingest_nonexistent_path() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.args(["ingest", "/nonexistent/path/that/does/not/exist"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found").or(predicate::str::contains("not a directory")),
        );
}

#[test]
fn import_nonexistent_path() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.args(["import", "/nonexistent/dir", "--wing", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a directory"));
}

#[test]
fn import_file_not_directory() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("notadir.txt");
    std::fs::write(&file_path, "hello").unwrap();

    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.args(["import", file_path.to_str().unwrap(), "--wing", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a directory"));
}

#[test]
fn migrate_nonexistent_source() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.args(["migrate", "--from", "/nonexistent/recall.sqlite3"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn forget_missing_wing() {
    recall_cmd()
        .arg("forget")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--wing"));
}

#[test]
fn search_empty_db_no_results() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.args(["search", "anything at all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No results found."));
}

#[test]
fn invalid_model_env_warns() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.env("RECALL_MODEL", "bge-nonexistent-model")
        .args(["prime", "--wing", "test"])
        .assert()
        .success()
        .stderr(predicate::str::contains("unknown RECALL_MODEL"));
}

// ─── forget confirmation path (ticket 061) ──────────────────────────────────
//
// assert_cmd stdin is always a pipe, so the spawned CLI sees a non-TTY: these
// tests exercise the --yes and non-interactive branches. The interactive [y/N]
// branch is covered by the pure `decide()` unit tests in src/cli.rs (it can't be
// driven through assert_cmd — that needs a PTY).

/// Create a temp DB, seed one chunk into `wing` (model-free — insert_chunk_atomic
/// takes an arbitrary embedding), drop the connection to flush WAL, and return
/// the (leaked) db path. The caller points `RECALL_DB` at it. Uses `open_db_at`
/// so it never touches the process-global `RECALL_DB` (no parallel-test race).
fn seeded_db(wing: &str) -> std::path::PathBuf {
    let dir = TempDir::new().unwrap();
    let db_path = dir.keep().join("seeded.sqlite3");
    {
        let conn = recall::store::open_db_at(&db_path).unwrap();
        // Any &[f32] works; these tests only count/delete, never rank.
        recall::store::insert_chunk_atomic(
            &conn,
            "seed chunk",
            wing,
            "general",
            "fact",
            "test",
            &[0.1f32; 768],
        )
        .unwrap();
        // conn dropped here → WAL flushed before the child process opens the DB.
    }
    db_path
}

#[test]
fn forget_non_tty_refuses_without_yes() {
    let db = seeded_db("victim");
    recall_cmd()
        .env("RECALL_DB", &db)
        .args(["forget", "--wing", "victim"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to delete"));
}

#[test]
fn forget_yes_deletes() {
    let db = seeded_db("victim");
    recall_cmd()
        .env("RECALL_DB", &db)
        .args(["forget", "--wing", "victim", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted 1"));
}

#[test]
fn forget_empty_wing_no_prompt() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.args(["forget", "--wing", "nonexistent", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing to delete"));
}

#[test]
fn forget_negative_duration_rejected() {
    let db = seeded_db("victim");
    recall_cmd()
        .env("RECALL_DB", &db)
        .args(["forget", "--wing", "victim", "--older-than=-5d", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid duration"));
}
