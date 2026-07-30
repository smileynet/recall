//! Contract tests — validate machine-readable output format stability.
//! These protect downstream consumers (kiro-cli steering, scripts).

use assert_cmd::Command;
use tempfile::TempDir;

fn recall_cmd() -> Command {
    Command::cargo_bin("recall").unwrap()
}

// =============================================================================
// health --json contract
// =============================================================================

#[test]
fn health_json_is_valid_json() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let output = recall_cmd()
        .env("RECALL_DB", &db_path)
        .args(["health", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let _json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("health --json must produce valid JSON");
}

#[test]
fn health_json_has_required_fields() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let output = recall_cmd()
        .env("RECALL_DB", &db_path)
        .args(["health", "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Required numeric fields
    assert!(json["total_chunks"].is_number(), "total_chunks must be a number");
    assert!(json["import_chunks"].is_number(), "import_chunks must be a number");
    assert!(json["session_chunks"].is_number(), "session_chunks must be a number");
    assert!(json["agent_chunks"].is_number(), "agent_chunks must be a number");
    assert!(json["wing_count"].is_number(), "wing_count must be a number");
    assert!(json["discoverable_projects"].is_number(), "discoverable_projects must be a number");
    assert!(json["covered_projects"].is_number(), "covered_projects must be a number");

    // Required object/array fields
    assert!(json["wings"].is_object(), "wings must be an object");
    assert!(json["import_wings"].is_array(), "import_wings must be an array");
    assert!(json["duplicates"].is_array(), "duplicates must be an array");
    assert!(json["missing_projects"].is_array(), "missing_projects must be an array");
    assert!(json["stale_wings"].is_array(), "stale_wings must be an array");

    // last_ingest_ts can be null or number
    assert!(
        json["last_ingest_ts"].is_null() || json["last_ingest_ts"].is_number(),
        "last_ingest_ts must be null or number"
    );
}

#[test]
fn health_json_empty_db_has_zero_counts() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let output = recall_cmd()
        .env("RECALL_DB", &db_path)
        .args(["health", "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["total_chunks"], 0);
    assert_eq!(json["wing_count"], 0);
    assert_eq!(json["import_chunks"], 0);
    assert_eq!(json["session_chunks"], 0);
    assert_eq!(json["agent_chunks"], 0);
}

// =============================================================================
// prime contract
// =============================================================================

#[test]
fn prime_starts_with_header() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let output = recall_cmd()
        .env("RECALL_DB", &db_path)
        .args(["prime", "--wing", "test"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("## Recall - Cross-Session Memory"),
        "prime must start with the standard header, got: {}",
        &stdout[..stdout.len().min(50)]
    );
}

#[test]
fn prime_contains_usage_instructions() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    let output = recall_cmd()
        .env("RECALL_DB", &db_path)
        .args(["prime", "--wing", "test"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("recall search"), "prime must contain search usage hint");
    assert!(stdout.contains("recall add"), "prime must contain add usage hint");
}

#[test]
fn prime_exit_code_zero() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");

    recall_cmd()
        .env("RECALL_DB", &db_path)
        .args(["prime", "--wing", "nonexistent_wing"])
        .assert()
        .success();
}
