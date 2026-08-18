use anyhow::Result;
use clap::{Parser, Subcommand};

use recall::{embed, ingest, logging, migrate, recall_log, search, store, telemetry, update};

#[derive(Parser)]
#[command(
    name = "recall",
    version,
    about = "Cross-session semantic memory for AI coding assistants"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Hybrid semantic search over stored memories
    Search {
        query: String,
        #[arg(long)]
        wing: Option<String>,
        #[arg(long, default_value = "5")]
        results: usize,
    },
    /// Store a fact (agent write-back)
    Add {
        content: String,
        /// Wing name (default: auto-detect from cwd)
        #[arg(long)]
        wing: Option<String>,
        #[arg(long, default_value = "general")]
        room: String,
        #[arg(long, default_value = "fact")]
        r#type: String,
    },
    /// Ingest session files (background task)
    Ingest {
        /// Path to session directory (default: ~/.kiro/sessions/cli)
        path: Option<String>,
    },
    /// Import markdown files into a wing
    Import {
        path: String,
        #[arg(long)]
        wing: String,
        /// Delete existing imports and reimport from scratch
        #[arg(long)]
        force: bool,
    },
    /// Import all .memory/ directories from project roots
    ImportAll {
        /// Force reimport all (delete + re-embed)
        #[arg(long)]
        force: bool,
    },
    /// Session start payload (recent facts + top results)
    Prime {
        /// Wing to scope results (default: auto-detect from cwd)
        #[arg(long)]
        wing: Option<String>,
    },
    /// Corpus overview
    Status,
    /// Machine-readable health diagnostics
    Health {
        #[arg(long)]
        json: bool,
    },
    /// Delete memories from a wing
    Forget {
        #[arg(long)]
        wing: String,
        #[arg(long)]
        older_than: Option<String>,
    },
    /// Migrate from a Python recall database
    Migrate {
        /// Path to the Python recall SQLite database
        #[arg(long)]
        from: String,
        /// Re-embed all content immediately (slower but ready to search)
        #[arg(long)]
        embed: bool,
    },
    /// Manage local telemetry and crash reporting
    Telemetry {
        #[command(subcommand)]
        action: TelemetryAction,
    },
    /// Run all periodic maintenance (ingest + import-all) in one process
    Sync {
        /// Force reimport all wings (bypass hash-gate)
        #[arg(long)]
        force: bool,
        /// Skip import step (only ingest sessions)
        #[arg(long)]
        skip_import: bool,
        /// Skip ingest step (only import .memory/ directories)
        #[arg(long)]
        skip_ingest: bool,
    },
    /// Check for and install updates
    Update,
}

#[derive(Subcommand)]
enum TelemetryAction {
    /// Show current telemetry status
    Status,
    /// Enable usage telemetry
    Enable,
    /// Disable usage telemetry
    Disable,
    /// Show telemetry statistics
    Stats,
    /// Delete all telemetry data
    Clear,
}

pub fn run() -> i32 {
    let start = std::time::Instant::now();

    // First-run: prompt for telemetry opt-in if no config exists
    telemetry::first_run_prompt();

    let cli = Cli::parse();
    let command_name = command_name(&cli.command);

    let result = match cli.command {
        Commands::Search {
            query,
            wing,
            results,
        } => cmd_search(&query, wing.as_deref(), results),
        Commands::Add {
            content,
            wing,
            room,
            r#type,
        } => {
            let resolved_wing = wing.unwrap_or_else(wing_from_cwd);
            cmd_add(&content, &resolved_wing, &room, &r#type)
        }
        Commands::Ingest { path } => cmd_ingest(path.as_deref()),
        Commands::Import { path, wing, force } => cmd_import(&path, &wing, force),
        Commands::ImportAll { force } => cmd_import_all(force),
        Commands::Prime { wing } => cmd_prime(wing.as_deref()),
        Commands::Status => cmd_status(),
        Commands::Health { json } => cmd_health(json),
        Commands::Forget { wing, older_than } => cmd_forget(&wing, older_than.as_deref()),
        Commands::Migrate { from, embed } => cmd_migrate(&from, embed),
        Commands::Telemetry { action } => match action {
            TelemetryAction::Status => telemetry::cmd_telemetry_status(),
            TelemetryAction::Enable => telemetry::cmd_telemetry_enable(),
            TelemetryAction::Disable => telemetry::cmd_telemetry_disable(),
            TelemetryAction::Stats => telemetry::cmd_telemetry_stats(),
            TelemetryAction::Clear => telemetry::cmd_telemetry_clear(),
        },
        Commands::Sync {
            force,
            skip_import,
            skip_ingest,
        } => cmd_sync(force, skip_import, skip_ingest),
        Commands::Update => update::cmd_update(),
    };

    match result {
        Ok(code) => {
            telemetry::record_event(&command_name, start, code, None);
            // Once-per-day update check (after command output, no-op if <24h since last)
            update::check_for_update();
            code
        }
        Err(e) => {
            let msg = format!("recall: {:#}", e);
            eprintln!("{}", msg);
            logging::log(&msg);
            telemetry::record_event(&command_name, start, 1, Some(&e));
            1
        }
    }
}

fn command_name(cmd: &Commands) -> String {
    match cmd {
        Commands::Search { .. } => "search",
        Commands::Add { .. } => "add",
        Commands::Ingest { .. } => "ingest",
        Commands::Import { .. } => "import",
        Commands::ImportAll { .. } => "import-all",
        Commands::Prime { .. } => "prime",
        Commands::Status => "status",
        Commands::Health { .. } => "health",
        Commands::Forget { .. } => "forget",
        Commands::Migrate { .. } => "migrate",
        Commands::Telemetry { .. } => "telemetry",
        Commands::Sync { .. } => "sync",
        Commands::Update => "update",
    }
    .to_string()
}

/// Derive wing name from current working directory.
fn wing_from_cwd() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().replace('-', "_")))
        .unwrap_or_else(|| "global".to_string())
}

fn cmd_search(query: &str, wing: Option<&str>, max_results: usize) -> Result<i32> {
    let db = store::open_db()?;
    embed::check_model_mismatch(&db);
    let embedder = embed::Embedder::new()?;
    let results = search::hybrid_search(&db, &embedder, query, wing, max_results)?;

    if results.is_empty() {
        println!("No results found.");
        return Ok(0);
    }

    println!("\n  Results for: {:?}\n", query);
    for (i, r) in results.iter().enumerate() {
        println!("  [{}] {} / {}", i + 1, r.wing, r.room);
        println!("      Source: {}", r.source);
        println!("      Score: {:.3}", r.score);
        println!();
        // Truncate content for display
        let preview: String = r.content.chars().take(200).collect();
        println!("      > {}", preview.replace('\n', "\n      > "));
        println!();
    }
    Ok(0)
}

fn cmd_add(content: &str, wing: &str, room: &str, dtype: &str) -> Result<i32> {
    let db = store::open_db()?;
    embed::check_model_mismatch(&db);
    let embedder = embed::Embedder::new()?;
    let embedding = embedder.embed_one(content)?;
    store::insert_chunk(&db, content, wing, room, dtype, "agent", &embedding)?;
    // Record model on first write
    store::set_meta(&db, "embedding_model", embedder.model().name())?;
    store::set_meta(&db, "embedding_dim", &embedder.dimensions().to_string())?;
    println!("Stored in {}/{} (type: {})", wing, room, dtype);
    Ok(0)
}

fn cmd_ingest(path: Option<&str>) -> Result<i32> {
    ingest::run_ingest(path)
}

fn cmd_import(path: &str, wing: &str, force: bool) -> Result<i32> {
    ingest::import_directory(path, wing, force)
}

fn cmd_import_all(force: bool) -> Result<i32> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home_code = std::path::PathBuf::from(&home).join("code");
        if home_code.is_dir() {
            roots.push(home_code);
        }
    }
    let d_code = std::path::PathBuf::from("D:/code");
    if d_code.is_dir() {
        roots.push(d_code);
    }

    let mut imported = 0;
    for root in &roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".memory").is_dir() {
                    let wing = path
                        .file_name()
                        .map(|n| n.to_string_lossy().replace('-', "_").replace('.', ""))
                        .unwrap_or_default();
                    let mem_path = path.join(".memory");
                    recall_log!(
                        "  {} → wing: {}",
                        path.file_name().unwrap().to_string_lossy(),
                        wing
                    );
                    ingest::import_directory(&mem_path.to_string_lossy(), &wing, force)?;
                    imported += 1;
                }
            }
        }
    }

    if imported == 0 {
        println!("No projects with .memory/ found in {:?}", roots);
    } else {
        println!("\nImported {} projects", imported);
    }
    Ok(0)
}

fn cmd_sync(force: bool, skip_import: bool, skip_ingest: bool) -> Result<i32> {
    // Note: ingest acquires its own exclusive lock (recall.lock) but import relies on
    // SQLite WAL for write safety. Concurrent sync is prevented by Windows Task Scheduler
    // single-instance policy. If manual overlap occurs, WAL ensures correctness.
    recall_log!("sync: starting");

    // Load embedder once for both operations
    let embedder = embed::Embedder::new()?;

    // Phase 1: Ingest sessions
    if !skip_ingest {
        recall_log!("sync: ingesting sessions");
        ingest::run_ingest_with_embedder(None, &embedder)?;
    }

    // Phase 2: Import all .memory/ directories
    if !skip_import {
        recall_log!("sync: importing .memory/ directories");

        let mut roots = Vec::new();
        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            let home_code = std::path::PathBuf::from(&home).join("code");
            if home_code.is_dir() {
                roots.push(home_code);
            }
        }
        let d_code = std::path::PathBuf::from("D:/code");
        if d_code.is_dir() {
            roots.push(d_code);
        }

        let mut imported = 0;
        for root in &roots {
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && path.join(".memory").is_dir() {
                        let wing = path
                            .file_name()
                            .map(|n| n.to_string_lossy().replace('-', "_").replace('.', ""))
                            .unwrap_or_default();
                        let mem_path = path.join(".memory");
                        recall_log!(
                            "  {} → wing: {}",
                            path.file_name().unwrap().to_string_lossy(),
                            wing
                        );
                        ingest::import_directory_with_embedder(
                            &mem_path.to_string_lossy(),
                            &wing,
                            force,
                            &embedder,
                        )?;
                        imported += 1;
                    }
                }
            }
        }

        recall_log!("sync: imported {} projects", imported);
    }

    recall_log!("sync: complete");
    println!("Sync complete.");
    Ok(0)
}

fn cmd_prime(wing_arg: Option<&str>) -> Result<i32> {
    let db = store::open_db()?;
    embed::check_model_mismatch(&db);

    // Auto-detect wing from cwd if not provided
    let wing = wing_arg.map(|w| w.to_string()).unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().replace('-', "_")))
            .unwrap_or_else(|| "global".to_string())
    });

    // Instructions header (always shown)
    println!("## Recall - Cross-Session Memory");
    println!();
    println!("Use `recall search \"query\"` before answering questions about past decisions.");
    println!("Use `recall add \"fact\" --wing X --room Y --type decision` to persist learnings.");
    println!();

    // Recent agent-written memories (scoped to wing)
    let recent = store::recent_agent_facts(&db, Some(&wing), 7)?;
    if !recent.is_empty() {
        println!("## Recent Memories ({})", wing);
        println!();
        for chunk in &recent {
            let date = format_epoch_date(chunk.created_at);
            let preview: String = chunk.content.chars().take(120).collect();
            println!("- [{}] {} ({})", chunk.dtype, preview, date);
        }
        println!();
    }

    // Relevant context via search (top retrieval for the wing)
    let embedder = embed::Embedder::new()?;
    let query = format!("important decisions and architecture for {}", wing);
    let results = search::hybrid_search(&db, &embedder, &query, Some(&wing), 3)?;
    if !results.is_empty() {
        println!("## Relevant Context");
        println!();
        for r in &results {
            let preview: String = r.content.chars().take(200).collect();
            let formatted = preview.replace('\n', "\n  ");
            println!("  {}", formatted);
            if !r.source.is_empty() {
                println!("  (source: {})", r.source);
            }
            println!();
        }
    }

    Ok(0)
}

/// Format a unix epoch timestamp as YYYY-MM-DD.
fn format_epoch_date(epoch: i64) -> String {
    if epoch <= 0 {
        return "unknown".to_string();
    }
    // Convert epoch seconds to date components
    let days = epoch / 86400;
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html (civil_from_days)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn cmd_status() -> Result<i32> {
    let db = store::open_db()?;
    let stats = store::corpus_stats(&db)?;
    println!("\n  Recall — {} drawers\n", stats.total_chunks);
    for (wing, count) in &stats.wings {
        println!("  WING: {} ({})", wing, count);
    }
    Ok(0)
}

fn cmd_health(json: bool) -> Result<i32> {
    let db = store::open_db()?;
    let health = build_health_report(&db)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&health)?);
    } else {
        println!("\n  Recall Health");
        println!("  {}", "─".repeat(40));
        println!(
            "  Total chunks:  {:>6} ({} import, {} session, {} agent)",
            health.total_chunks, health.import_chunks, health.session_chunks, health.agent_chunks
        );
        println!("  Wings:         {:>6}", health.wing_count);
        println!(
            "  Coverage:      {}/{} projects imported",
            health.covered_projects, health.discoverable_projects
        );
        if !health.missing_projects.is_empty() {
            let display: Vec<&str> = health
                .missing_projects
                .iter()
                .take(5)
                .map(|s| s.as_str())
                .collect();
            let suffix = if health.missing_projects.len() > 5 {
                format!(" (+{} more)", health.missing_projects.len() - 5)
            } else {
                String::new()
            };
            println!("  Missing:       {}{}", display.join(", "), suffix);
        }
        if !health.duplicates.is_empty() {
            println!("  ⚠ Duplicates:  {:?}", health.duplicates);
        }
        if let Some(ts) = health.last_ingest_ts {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let hours = (now - ts) as f64 / 3600.0;
            println!("  Last ingest:   {:.1}h ago", hours);
        } else {
            println!("  Last ingest:   never");
        }
        if let Some(log_path) = recall::logging::current_log_path() {
            if let Ok(meta) = std::fs::metadata(&log_path) {
                println!(
                    "  Last log:      {} ({:.1} KB)",
                    log_path.display(),
                    meta.len() as f64 / 1024.0
                );
            }
        }
        println!();
    }
    Ok(0)
}

#[derive(serde::Serialize)]
struct HealthReport {
    total_chunks: i64,
    import_chunks: i64,
    session_chunks: i64,
    agent_chunks: i64,
    wing_count: usize,
    wings: std::collections::HashMap<String, i64>,
    import_wings: Vec<String>,
    duplicates: Vec<Vec<String>>,
    last_ingest_ts: Option<i64>,
    discoverable_projects: usize,
    covered_projects: usize,
    missing_projects: Vec<String>,
    stale_wings: Vec<String>,
}

fn build_health_report(db: &rusqlite::Connection) -> Result<HealthReport> {
    // Total chunks
    let total: i64 = db.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;

    // Per-source-type counts
    let import_chunks: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE source LIKE 'import:%'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let session_chunks: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE type = 'session'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let agent_chunks: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE source = 'agent'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Per-wing breakdown
    let mut stmt = db.prepare("SELECT wing, COUNT(*) FROM chunks GROUP BY wing ORDER BY wing")?;
    let wings: std::collections::HashMap<String, i64> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Import wings (wings with import: source entries)
    let mut stmt = db.prepare("SELECT DISTINCT wing FROM chunks WHERE source LIKE 'import:%'")?;
    let import_wings: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Duplicate wing detection (names differing only by hyphen/underscore)
    let duplicates = detect_wing_duplicates(&wings.keys().cloned().collect::<Vec<_>>());

    // Last ingest timestamp (from marker file)
    let last_ingest_ts = read_last_ingest_marker();

    // Discoverable projects (scan ~/code and D:/code for dirs with .memory/)
    let (discoverable, covered, missing) = discover_project_coverage(&import_wings);

    Ok(HealthReport {
        total_chunks: total,
        import_chunks,
        session_chunks,
        agent_chunks,
        wing_count: wings.len(),
        wings,
        import_wings,
        duplicates,
        last_ingest_ts,
        discoverable_projects: discoverable,
        covered_projects: covered,
        missing_projects: missing,
        stale_wings: Vec::new(),
    })
}

fn detect_wing_duplicates(wing_names: &[String]) -> Vec<Vec<String>> {
    let mut normalized: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for name in wing_names {
        let key = name.replace(['-', '.'], "_").to_lowercase();
        normalized.entry(key).or_default().push(name.clone());
    }
    normalized
        .into_values()
        .filter(|names| names.len() > 1)
        .collect()
}

fn read_last_ingest_marker() -> Option<i64> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    let marker = std::path::PathBuf::from(home)
        .join(".recall")
        .join("last_ingest");
    let content = std::fs::read_to_string(marker).ok()?;
    content.trim().parse().ok()
}

fn discover_project_coverage(import_wings: &[String]) -> (usize, usize, Vec<String>) {
    let mut roots = Vec::new();

    // User's home/code directory
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home_code = std::path::PathBuf::from(&home).join("code");
        if home_code.is_dir() {
            roots.push(home_code);
        }
    }
    // D:/code (Windows multi-drive)
    let d_code = std::path::PathBuf::from("D:/code");
    if d_code.is_dir() {
        roots.push(d_code);
    }

    let mut discoverable = Vec::new();
    for root in &roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".memory").is_dir() {
                    let wing_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().replace('-', "_").replace('.', ""))
                        .unwrap_or_default();
                    if !wing_name.is_empty() {
                        discoverable.push(wing_name);
                    }
                }
            }
        }
    }

    let covered: Vec<&String> = discoverable
        .iter()
        .filter(|p| import_wings.contains(p))
        .collect();
    let missing: Vec<String> = discoverable
        .iter()
        .filter(|p| !import_wings.contains(p))
        .cloned()
        .collect();

    (discoverable.len(), covered.len(), missing)
}

fn cmd_forget(wing: &str, older_than: Option<&str>) -> Result<i32> {
    let db = store::open_db()?;

    let deleted = if let Some(age_str) = older_than {
        let seconds = parse_duration(age_str).ok_or_else(|| {
            anyhow::anyhow!("invalid duration '{}' (use e.g. 90d, 24h, 4w)", age_str)
        })?;
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - seconds;
        store::delete_wing_older_than(&db, wing, cutoff)?
    } else {
        store::delete_wing(&db, wing)?
    };

    println!("Deleted {} chunks from wing {:?}", deleted, wing);
    Ok(0)
}

/// Parse a duration string like "90d", "24h", "4w" into seconds.
fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num_str, suffix) = s.split_at(s.len() - 1);
    let num: i64 = num_str.parse().ok()?;
    match suffix {
        "s" => Some(num),
        "m" => Some(num * 60),
        "h" => Some(num * 3600),
        "d" => Some(num * 86400),
        "w" => Some(num * 7 * 86400),
        _ => None,
    }
}

fn cmd_migrate(from: &str, batch_embed: bool) -> Result<i32> {
    migrate::run_migrate(from, batch_embed)?;
    Ok(0)
}
