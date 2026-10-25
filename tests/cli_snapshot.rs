//! Spike: evaluate insta-cmd for CLI snapshot testing.
//! Tests here validate the workflow works with RECALL_DB env var and volatile filtering.

mod common;

use std::process::Command;
use tempfile::TempDir;

fn recall_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_recall"))
}

#[test]
fn spike_status_snapshot() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d+ drawers", "[N] drawers");
    let _guard = settings.bind_to_scope();

    insta_cmd::assert_cmd_snapshot!(recall_bin().env("RECALL_DB", &db_path).arg("status"));
}

#[test]
fn spike_health_json_snapshot() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\d+\.\d+h ago", "[TIME] ago");
    settings.add_filter(r#""last_ingest_ts": \d+"#, r#""last_ingest_ts": 0"#);
    settings.add_filter(
        r#""discoverable_projects": \d+"#,
        r#""discoverable_projects": "[N]""#,
    );
    settings.add_filter(
        r#"(?s)"missing_projects": \[.*?\]"#,
        r#""missing_projects": ["[FILTERED]"]"#,
    );
    let _guard = settings.bind_to_scope();

    insta_cmd::assert_cmd_snapshot!(recall_bin()
        .env("RECALL_DB", &db_path)
        .args(["health", "--json"]));
}

#[test]
fn spike_search_no_results() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    insta_cmd::assert_cmd_snapshot!(recall_bin()
        .env("RECALL_DB", &db_path)
        .args(["search", "nonexistent query"]));
}
