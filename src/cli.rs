use clap::{Parser, Subcommand};
use anyhow::Result;

use recall::{store, search, ingest, embed, migrate};

#[derive(Parser)]
#[command(name = "recall", about = "Cross-session semantic memory for AI coding assistants")]
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
        #[arg(long)]
        wing: String,
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
}

pub fn run() -> i32 {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Search { query, wing, results } => cmd_search(&query, wing.as_deref(), results),
        Commands::Add { content, wing, room, r#type } => cmd_add(&content, &wing, &room, &r#type),
        Commands::Ingest { path } => cmd_ingest(path.as_deref()),
        Commands::Import { path, wing } => cmd_import(&path, &wing),
        Commands::Prime { wing } => cmd_prime(wing.as_deref()),
        Commands::Status => cmd_status(),
        Commands::Health { json } => cmd_health(json),
        Commands::Forget { wing, older_than } => cmd_forget(&wing, older_than.as_deref()),
        Commands::Migrate { from, embed } => cmd_migrate(&from, embed),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("recall: {:#}", e);
            1
        }
    }
}

fn cmd_search(query: &str, wing: Option<&str>, max_results: usize) -> Result<i32> {
    let db = store::open_db()?;
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
    let embedder = embed::Embedder::new()?;
    let embedding = embedder.embed_one(content)?;
    store::insert_chunk(&db, content, wing, room, dtype, "agent", &embedding)?;
    println!("Stored in {}/{} (type: {})", wing, room, dtype);
    Ok(0)
}

fn cmd_ingest(path: Option<&str>) -> Result<i32> {
    ingest::run_ingest(path)
}

fn cmd_import(path: &str, wing: &str) -> Result<i32> {
    ingest::import_directory(path, wing)
}

fn cmd_prime(wing_arg: Option<&str>) -> Result<i32> {
    let db = store::open_db()?;

    // Auto-detect wing from cwd if not provided
    let wing = wing_arg
        .map(|w| w.to_string())
        .unwrap_or_else(|| {
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
    let stats = store::corpus_stats(&db)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("Total chunks: {}", stats.total_chunks);
        println!("Wings: {}", stats.wings.len());
    }
    Ok(0)
}

fn cmd_forget(wing: &str, _older_than: Option<&str>) -> Result<i32> {
    let db = store::open_db()?;
    let deleted = store::delete_wing(&db, wing)?;
    println!("Deleted {} chunks from wing {:?}", deleted, wing);
    Ok(0)
}

fn cmd_migrate(from: &str, batch_embed: bool) -> Result<i32> {
    migrate::run_migrate(from, batch_embed)?;
    Ok(0)
}
