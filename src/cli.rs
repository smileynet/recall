use clap::{Parser, Subcommand};
use anyhow::Result;

use recall::{store, search, ingest, embed};

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
    Prime,
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
}

pub fn run() -> i32 {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Search { query, wing, results } => cmd_search(&query, wing.as_deref(), results),
        Commands::Add { content, wing, room, r#type } => cmd_add(&content, &wing, &room, &r#type),
        Commands::Ingest { path } => cmd_ingest(path.as_deref()),
        Commands::Import { path, wing } => cmd_import(&path, &wing),
        Commands::Prime => cmd_prime(),
        Commands::Status => cmd_status(),
        Commands::Health { json } => cmd_health(json),
        Commands::Forget { wing, older_than } => cmd_forget(&wing, older_than.as_deref()),
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

fn cmd_prime() -> Result<i32> {
    let db = store::open_db()?;
    let recent = store::recent_agent_facts(&db, 5)?;
    if !recent.is_empty() {
        println!("## Recall - Cross-Session Memory\n");
        println!("Use `recall search \"query\"` before answering questions about past decisions.");
        println!("Use `recall add \"fact\" --wing X --room Y --type decision` to persist learnings.\n");
        println!("## Recent Memories\n");
        for chunk in &recent {
            println!("- [{}] {} ({})", chunk.dtype, chunk.content, chunk.wing);
        }
    }
    Ok(0)
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
