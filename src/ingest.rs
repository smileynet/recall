use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;

use crate::{embed, scan, store};

/// Default session directory.
fn default_sessions_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".kiro").join("sessions").join("cli")
}

/// Run ingestion: scan for changes, chunk new/modified files, embed, store.
pub fn run_ingest(path: Option<&str>) -> Result<i32> {
    let dir = path.map(PathBuf::from).unwrap_or_else(default_sessions_dir);
    if !dir.is_dir() {
        anyhow::bail!("session directory not found: {}", dir.display());
    }

    // Acquire exclusive lock (prevents concurrent ingestion)
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let lock_path = PathBuf::from(&home).join(".recall").join("recall.lock");
    std::fs::create_dir_all(lock_path.parent().unwrap())?;
    let lock_file = std::fs::File::create(&lock_path)?;
    if lock_file.try_lock_exclusive().is_err() {
        eprintln!("recall: another ingestion is running, skipping");
        return Ok(0);
    }

    let conn = store::open_db()?;

    // Phase 1: stat scan for changes
    let changed = scan::scan_for_changes(&dir, &conn)?;
    if changed.is_empty() {
        return Ok(0);
    }

    eprintln!("  Ingesting: {}", dir.display());
    eprintln!("  Files: {} changed of total", changed.len());

    // Phase 2: load embedder (amortize cold start over batch)
    let embedder = embed::Embedder::new()?;

    // Phase 3: process changed files
    let mut total_chunks = 0;
    for file_path in &changed {
        let chunks = chunk_session_file(file_path)?;
        if chunks.is_empty() {
            scan::update_cache(&conn, file_path)?;
            continue;
        }

        // Determine wing from file content or path
        let wing = derive_wing(file_path);

        // Batch embed
        let texts: Vec<&str> = chunks.iter().map(|c| c.as_str()).collect();
        let embeddings = embedder.embed_batch(&texts)?;

        // Store in transaction
        conn.execute("BEGIN IMMEDIATE", [])?;
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            store::insert_chunk(&conn, chunk, &wing, "general", "session",
                &file_path.to_string_lossy(), embedding)?;
        }
        scan::update_cache(&conn, file_path)?;
        conn.execute("COMMIT", [])?;

        total_chunks += chunks.len();
    }

    eprintln!("  Done: {} files, {} chunks ingested", changed.len(), total_chunks);
    Ok(0)
}

/// Import markdown files from a directory into a wing.
pub fn import_directory(path: &str, wing: &str) -> Result<i32> {
    let dir = Path::new(path);
    if !dir.is_dir() {
        anyhow::bail!("not a directory: {}", path);
    }

    let conn = store::open_db()?;
    let embedder = embed::Embedder::new()?;

    let mut total_chunks = 0;
    for entry in walkdir_md(dir) {
        let content = std::fs::read_to_string(&entry)
            .with_context(|| format!("reading {}", entry.display()))?;
        let chunks = chunk_markdown(&content);
        if chunks.is_empty() { continue; }

        let texts: Vec<&str> = chunks.iter().map(|c| c.as_str()).collect();
        let embeddings = embedder.embed_batch(&texts)?;

        conn.execute("BEGIN IMMEDIATE", [])?;
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            let room = detect_room(&entry);
            store::insert_chunk(&conn, chunk, wing, &room, "import",
                &entry.to_string_lossy(), embedding)?;
        }
        conn.execute("COMMIT", [])?;
        total_chunks += chunks.len();
    }

    println!("Imported {} chunks into wing {:?}", total_chunks, wing);
    Ok(0)
}

// --- Chunking ---

/// Chunk a JSONL session file into text chunks.
fn chunk_session_file(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
    let mut chunks = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() { continue; }
        // Extract text content from JSONL (simplified — parse role + content)
        if let Some(text) = extract_message_text(line) {
            if text.len() > 50 { // skip trivial messages
                chunks.push(text);
            }
        }
    }
    Ok(chunks)
}

/// Extract message text from a JSONL line (simplified parser).
fn extract_message_text(line: &str) -> Option<String> {
    // Look for "content":"..." pattern
    let content_key = "\"content\":\"";
    let start = line.find(content_key)? + content_key.len();
    let rest = &line[start..];
    // Find the closing quote (handling escaped quotes)
    let mut end = 0;
    let mut escaped = false;
    for (i, ch) in rest.chars().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            end = i;
            break;
        }
    }
    if end == 0 { return None; }
    let text = &rest[..end];
    // Unescape basic sequences
    let unescaped = text.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\");
    Some(unescaped)
}

/// Chunk markdown by headings (## boundaries).
fn chunk_markdown(content: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        if line.starts_with("## ") && !current.trim().is_empty() {
            chunks.push(current.trim().to_string());
            current = String::new();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

/// Derive a wing name from a file path (project name heuristic).
fn derive_wing(path: &Path) -> String {
    // Use parent directory name or "sessions" as default
    path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "sessions".to_string())
}

/// Detect room from a markdown file's subdirectory.
fn detect_room(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "general".to_string())
}

/// Walk a directory for .md files.
fn walkdir_md(dir: &Path) -> Vec<PathBuf> {
    jwalk::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .map(|e| e.path())
        .collect()
}
