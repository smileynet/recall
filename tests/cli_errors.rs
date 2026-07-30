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
    let db_path = dir.into_path().join("empty.sqlite3");
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
fn add_missing_wing_arg() {
    let mut cmd = recall_cmd();
    with_empty_db(&mut cmd);
    cmd.args(["add", "some fact"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--wing"));
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
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("not a directory")));
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
