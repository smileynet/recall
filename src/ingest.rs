use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde_json::Value;

use crate::{embed, recall_log, scan, store};

// --- Constants (matching Python recall) ---

const CHUNK_SIZE: usize = 800;
const MIN_CHUNK_SIZE: usize = 30;

const TOPIC_KEYWORDS: &[(&str, &[&str])] = &[
    ("technical", &["code", "function", "bug", "error", "api", "server", "deploy", "git", "test", "debug", "refactor"]),
    ("architecture", &["architecture", "design", "pattern", "structure", "interface", "module", "component", "layer"]),
    ("planning", &["plan", "roadmap", "milestone", "scope", "requirement", "spec", "backlog", "sprint"]),
    ("decisions", &["decided", "chose", "recommendation", "trade-off", "approach", "option", "prefer", "agree"]),
    ("problems", &["problem", "issue", "broken", "failed", "crash", "stuck", "workaround", "fix", "solved"]),
];

/// Write unix timestamp to ~/.recall/last_ingest after successful ingest.
fn write_last_ingest_marker() {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let marker = PathBuf::from(home).join(".recall").join("last_ingest");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = std::fs::write(&marker, now.to_string());
}

/// Check if a file was modified less than 5 minutes ago (likely still being written).
fn is_active_file(path: &Path) -> bool {
    const ACTIVE_THRESHOLD_SECS: u64 = 300; // 5 minutes
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
                return age.as_secs() < ACTIVE_THRESHOLD_SECS;
            }
        }
    }
    false
}

/// Default session directory.
fn default_sessions_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".kiro").join("sessions").join("cli")
}

/// Run ingestion: scan for changes, chunk new/modified files, embed, store.
pub fn run_ingest(path: Option<&str>) -> Result<i32> {
    let embedder = embed::Embedder::new()?;
    run_ingest_with_embedder(path, &embedder)
}

/// Run ingestion with a pre-loaded embedder (for shared-embedder use in sync).
pub fn run_ingest_with_embedder(path: Option<&str>, embedder: &embed::Embedder) -> Result<i32> {
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
        recall_log!("recall: another ingestion is running, skipping");
        return Ok(0);
    }

    let conn = store::open_db()?;

    // Phase 1: stat scan for changes
    let changed = scan::scan_for_changes(&dir, &conn)?;
    if changed.is_empty() {
        return Ok(0);
    }

    recall_log!("  Ingesting: {}", dir.display());
    recall_log!("  Files: {} changed of total", changed.len());

    // Phase 3: process changed files
    let mut total_chunks = 0;
    let mut files_deferred = 0;
    for file_path in &changed {
        // Skip files modified less than 5 minutes ago (likely still being written)
        if is_active_file(file_path) {
            files_deferred += 1;
            continue;
        }

        let messages = parse_session_file(file_path)?;
        if messages.is_empty() {
            scan::update_cache(&conn, file_path)?;
            continue;
        }

        let chunks = chunk_messages(&messages);
        if chunks.is_empty() {
            scan::update_cache(&conn, file_path)?;
            continue;
        }

        // Determine wing from session metadata (cwd) or fallback to path
        let wing = derive_wing_from_session(&dir, file_path);

        // Batch embed
        let texts: Vec<&str> = chunks.iter().map(|c| c.as_str()).collect();
        let embeddings = embedder.embed_batch(&texts)?;

        // Store in transaction
        conn.execute("BEGIN IMMEDIATE", [])?;
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            let room = classify_room(chunk);
            store::insert_chunk(&conn, chunk, &wing, &room, "session",
                &file_path.to_string_lossy(), embedding)?;
        }
        scan::update_cache(&conn, file_path)?;
        conn.execute("COMMIT", [])?;

        total_chunks += chunks.len();
    }

    // Record model metadata
    if total_chunks > 0 {
        store::set_meta(&conn, "embedding_model", embedder.model().name())?;
        store::set_meta(&conn, "embedding_dim", &embedder.dimensions().to_string())?;
        // Checkpoint WAL after large batch
        if total_chunks > 100 {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        }
    }

    recall_log!("  Done: {} files, {} chunks ingested{}", changed.len(), total_chunks,
        if files_deferred > 0 { format!(" ({} deferred — active)", files_deferred) } else { String::new() });

    // Write staleness marker
    if total_chunks > 0 {
        write_last_ingest_marker();
    }

    Ok(0)
}

/// Import markdown files from a directory into a wing.
/// Uses SHA-256 content hashing to skip unchanged files on re-import.
pub fn import_directory(path: &str, wing: &str, force: bool) -> Result<i32> {
    let embedder = embed::Embedder::new()?;
    import_directory_with_embedder(path, wing, force, &embedder)
}

/// Import markdown files with a pre-loaded embedder (for shared-embedder use in sync).
pub fn import_directory_with_embedder(path: &str, wing: &str, force: bool, embedder: &embed::Embedder) -> Result<i32> {
    let dir = Path::new(path);
    if !dir.is_dir() {
        anyhow::bail!("not a directory: {}", path);
    }

    let conn = store::open_db()?;

    // --force: delete all existing import chunks and manifest entries for this wing
    if force {
        let deleted = store::delete_chunks_by_source_prefix(&conn, &format!("import:{}:", wing))?;
        // Clear manifest
        for src in store::get_import_sources_for_wing(&conn, wing)? {
            store::delete_import_source(&conn, &src, wing)?;
        }
        if deleted > 0 {
            recall_log!("  Force: deleted {} existing import chunks for wing '{}'", deleted, wing);
        }
    }

    let md_files = walkdir_md(dir);
    if md_files.is_empty() {
        println!("  No .md files found in {}", path);
        return Ok(0);
    }

    // Build set of current file relative paths for orphan detection
    let current_rel_paths: std::collections::HashSet<String> = md_files.iter()
        .filter_map(|f| f.strip_prefix(dir).ok())
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Phase 1: Detect deleted files (in manifest but no longer on disk)
    let existing_sources = store::get_import_sources_for_wing(&conn, wing)?;
    let mut files_deleted = 0;
    for src_path in &existing_sources {
        if !current_rel_paths.contains(src_path.as_str()) {
            let source_key = format!("import:{}:{}", wing, src_path);
            store::delete_chunks_by_source(&conn, &source_key)?;
            store::delete_import_source(&conn, src_path, wing)?;
            files_deleted += 1;
        }
    }

    // Phase 2: Hash-gate import for new and changed files
    let mut total_chunks = 0;
    let mut files_imported = 0;
    let mut files_updated = 0;
    let mut files_skipped = 0;

    recall_log!("  Importing: {}", dir.display());
    recall_log!("  Wing: {}", wing);
    recall_log!("  Files: {} markdown", md_files.len());

    for entry in &md_files {
        let rel_path = match entry.strip_prefix(dir) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let source_key = format!("import:{}:{}", wing, rel_path);

        let content = std::fs::read_to_string(entry)
            .with_context(|| format!("reading {}", entry.display()))?;
        if content.trim().is_empty() {
            continue;
        }

        // Compute content hash
        use sha2::{Sha256, Digest};
        let content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        let file_size = content.len() as i64;

        // Hash-gate: compare against stored hash
        let stored_hash = store::get_import_source_hash(&conn, &rel_path, wing)?;
        if stored_hash.as_deref() == Some(&content_hash) {
            files_skipped += 1;
            continue;
        }

        // File is new or changed — delete old chunks if updating
        if stored_hash.is_some() {
            store::delete_chunks_by_source(&conn, &source_key)?;
            files_updated += 1;
        } else {
            files_imported += 1;
        }

        // Chunk
        let chunks = chunk_markdown(&content);
        if chunks.is_empty() {
            store::upsert_import_source(&conn, &rel_path, wing, &content_hash, file_size, 0)?;
            continue;
        }

        // Derive room from relative path
        let room = Path::new(&rel_path)
            .parent()
            .and_then(|p| p.components().next())
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| "general".to_string());

        // Detect type from frontmatter
        let dtype = detect_type_from_frontmatter(&content);

        // Embed and store
        let texts: Vec<&str> = chunks.iter().map(|c| c.as_str()).collect();
        let embeddings = embedder.embed_batch(&texts)?;

        conn.execute("BEGIN IMMEDIATE", [])?;
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            store::insert_chunk(&conn, chunk, wing, &room, &dtype, &source_key, embedding)?;
        }
        conn.execute("COMMIT", [])?;

        // Update manifest
        store::upsert_import_source(&conn, &rel_path, wing, &content_hash, file_size, chunks.len() as i64)?;
        total_chunks += chunks.len();
    }

    // Summary
    let mut parts = Vec::new();
    if files_imported > 0 { parts.push(format!("{} new", files_imported)); }
    if files_updated > 0 { parts.push(format!("{} updated", files_updated)); }
    if files_skipped > 0 { parts.push(format!("{} unchanged", files_skipped)); }
    if files_deleted > 0 { parts.push(format!("{} deleted", files_deleted)); }
    let summary = if parts.is_empty() { "no changes".to_string() } else { parts.join(", ") };

    // Record model metadata
    if total_chunks > 0 {
        store::set_meta(&conn, "embedding_model", embedder.model().name())?;
        store::set_meta(&conn, "embedding_dim", &embedder.dimensions().to_string())?;
        // Checkpoint WAL after large batch
        if total_chunks > 100 {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        }
    }

    println!("  Done: {} ({} chunks indexed)", summary, total_chunks);
    Ok(0)
}

/// Extract document type from YAML frontmatter (simple key scan).
fn detect_type_from_frontmatter(content: &str) -> String {
    let content = content.trim_start_matches('\u{feff}'); // strip BOM
    if !content.starts_with("---") {
        return "document".to_string();
    }
    // Find end of frontmatter
    if let Some(end) = content[3..].find("\n---") {
        let fm = &content[3..3 + end];
        for line in fm.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("type:") {
                return trimmed[5..].trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            }
        }
    }
    "document".to_string()
}

// =============================================================================
// Session JSONL Parsing (multi-format: v3, v2, codex)
// =============================================================================

/// A parsed message with role and text.
struct Message {
    role: Role,
    text: String,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Role {
    User,
    Assistant,
}

/// Parse a session JSONL file, auto-detecting format.
fn parse_session_file(path: &Path) -> Result<Vec<Message>> {
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Try v3 first, then v2, then codex
    if let Some(msgs) = parse_kiro_v3(&content) {
        return Ok(msgs);
    }
    if let Some(msgs) = parse_kiro_v2(&content) {
        return Ok(msgs);
    }
    if let Some(msgs) = parse_codex(&content) {
        return Ok(msgs);
    }

    Ok(Vec::new())
}

/// Parse kiro-cli v3 JSONL: {id, timestamp, payload: {type: "user"|"assistant", content}}
fn parse_kiro_v3(content: &str) -> Option<Vec<Message>> {
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }

    // Detect: v3 has payload.type
    let first: Value = serde_json::from_str(lines[0]).ok()?;
    if first.get("payload")?.get("type").is_none() {
        return None;
    }

    let mut messages = Vec::new();
    for line in &lines {
        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let payload = entry.get("payload")?;
        let ptype = payload.get("type")?.as_str()?;
        let text = payload.get("content")?.as_str()?.trim();
        if text.is_empty() {
            continue;
        }

        match ptype {
            "user" => messages.push(Message { role: Role::User, text: text.to_string() }),
            "assistant" => {
                // Merge consecutive assistant messages
                if let Some(last) = messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.text.push('\n');
                        last.text.push_str(text);
                        continue;
                    }
                }
                messages.push(Message { role: Role::Assistant, text: text.to_string() });
            }
            _ => {}
        }
    }

    if messages.len() >= 2 { Some(messages) } else { None }
}

/// Parse kiro-cli v1/v2 JSONL: {version: "v1", kind: "Prompt"|"AssistantMessage", data: {...}}
fn parse_kiro_v2(content: &str) -> Option<Vec<Message>> {
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }

    // Detect: v2 has version=v1 and kind field
    let mut is_v2 = false;
    for line in lines.iter().take(5) {
        if let Ok(entry) = serde_json::from_str::<Value>(line) {
            if entry.get("version").and_then(|v| v.as_str()) == Some("v1")
                && entry.get("kind").is_some()
            {
                is_v2 = true;
                break;
            }
        }
    }
    if !is_v2 {
        return None;
    }

    let mut messages = Vec::new();
    for line in &lines {
        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if entry.get("version").and_then(|v| v.as_str()) != Some("v1") {
            continue;
        }

        let kind = match entry.get("kind").and_then(|k| k.as_str()) {
            Some(k) => k,
            None => continue,
        };
        let data = match entry.get("data") {
            Some(d) => d,
            None => continue,
        };

        match kind {
            "Prompt" => {
                let mut parts = Vec::new();
                if let Some(blocks) = data.get("content").and_then(|c| c.as_array()) {
                    for block in blocks {
                        if block.get("kind").and_then(|k| k.as_str()) == Some("text") {
                            if let Some(t) = block.get("data").and_then(|d| d.as_str()) {
                                let trimmed = t.trim();
                                if !trimmed.is_empty() {
                                    parts.push(trimmed.to_string());
                                }
                            }
                        }
                    }
                }
                if !parts.is_empty() {
                    messages.push(Message { role: Role::User, text: parts.join("\n") });
                }
            }
            "AssistantMessage" => {
                let mut text_parts = Vec::new();
                if let Some(blocks) = data.get("content").and_then(|c| c.as_array()) {
                    for block in blocks {
                        let bk = block.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                        match bk {
                            "text" => {
                                if let Some(t) = block.get("data").and_then(|d| d.as_str()) {
                                    let trimmed = t.trim();
                                    if !trimmed.is_empty() {
                                        text_parts.push(trimmed.to_string());
                                    }
                                }
                            }
                            "toolUse" => {
                                if let Some(td) = block.get("data") {
                                    let name = td.get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("unknown");
                                    let purpose = td.get("input")
                                        .and_then(|i| i.get("__tool_use_purpose"))
                                        .and_then(|p| p.as_str())
                                        .unwrap_or("");
                                    let summary = if purpose.is_empty() {
                                        format!("[tool: {}]", name)
                                    } else {
                                        format!("[tool: {}] {}", name, purpose)
                                    };
                                    text_parts.push(summary);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                let combined = text_parts.join("\n");
                if !combined.is_empty() {
                    // Merge consecutive assistant messages
                    if let Some(last) = messages.last_mut() {
                        if last.role == Role::Assistant {
                            last.text.push('\n');
                            last.text.push_str(&combined);
                            continue;
                        }
                    }
                    messages.push(Message { role: Role::Assistant, text: combined });
                }
            }
            _ => {}
        }
    }

    if messages.len() >= 2 { Some(messages) } else { None }
}

/// Parse OpenAI Codex CLI JSONL: {type: "event_msg", payload: {type, message}}
fn parse_codex(content: &str) -> Option<Vec<Message>> {
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut messages = Vec::new();
    let mut has_session_meta = false;

    for line in &lines {
        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = entry.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if entry_type == "session_meta" {
            has_session_meta = true;
            continue;
        }
        if entry_type != "event_msg" {
            continue;
        }

        let payload = match entry.get("payload") {
            Some(p) => p,
            None => continue,
        };
        let msg = match payload.get("message").and_then(|m| m.as_str()) {
            Some(m) if !m.trim().is_empty() => m.trim(),
            _ => continue,
        };
        let ptype = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match ptype {
            "user_message" => messages.push(Message { role: Role::User, text: msg.to_string() }),
            "agent_message" => messages.push(Message { role: Role::Assistant, text: msg.to_string() }),
            _ => {}
        }
    }

    if messages.len() >= 2 && has_session_meta { Some(messages) } else { None }
}

// =============================================================================
// Message-pair chunking (matches Python chunker.chunk_messages)
// =============================================================================

/// Chunk conversation messages into ~CHUNK_SIZE char groups.
/// User messages are prefixed with "> ", messages joined with newlines.
/// Chunks smaller than MIN_CHUNK_SIZE are discarded.
fn chunk_messages(messages: &[Message]) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_len = 0;

    for msg in messages {
        let line = match msg.role {
            Role::User => format!("> {}", msg.text),
            Role::Assistant => msg.text.clone(),
        };

        if current_len + line.len() > CHUNK_SIZE && !current.is_empty() {
            chunks.push(current.join("\n"));
            current.clear();
            current_len = 0;
        }
        current_len += line.len();
        current.push(line);
    }

    if !current.is_empty() {
        chunks.push(current.join("\n"));
    }

    chunks.into_iter().filter(|c| c.len() >= MIN_CHUNK_SIZE).collect()
}

// =============================================================================
// Room classification (keyword scoring, matches Python chunker.classify_room)
// =============================================================================

/// Classify a chunk into a room by keyword scoring.
fn classify_room(text: &str) -> String {
    // Truncate at a char boundary for the keyword scan
    let end = text.char_indices()
        .take_while(|(i, _)| *i < 3000)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(text.len());
    let text_lower = text[..end].to_lowercase();
    let mut best_room = "general";
    let mut best_score = 0;

    for &(room, keywords) in TOPIC_KEYWORDS {
        let score: usize = keywords.iter().filter(|kw| text_lower.contains(*kw)).count();
        if score > best_score {
            best_score = score;
            best_room = room;
        }
    }

    best_room.to_string()
}

// =============================================================================
// Wing derivation from session metadata
// =============================================================================

/// Derive wing name from session metadata (cwd field).
/// Looks for <session_id>.json (v2) or parent/session.json (v3) to find the cwd,
/// then uses the last path component with hyphens replaced by underscores.
fn derive_wing_from_session(sessions_dir: &Path, jsonl_path: &Path) -> String {
    // Try cwd-based derivation
    if let Some(cwd) = extract_cwd_from_session(sessions_dir, jsonl_path) {
        let project_name = Path::new(&cwd)
            .file_name()
            .map(|n| n.to_string_lossy().replace('-', "_"))
            .unwrap_or_default();
        if !project_name.is_empty() {
            return project_name;
        }
    }

    // Fallback: parent directory name
    jsonl_path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().replace('-', "_"))
        .unwrap_or_else(|| "sessions".to_string())
}

/// Extract cwd from session JSON metadata.
/// V2: sessions_dir/<session_id>.json → data.cwd
/// V3: jsonl is in sess_<id>/messages.jsonl → sess_<id>/session.json → workspacePaths[0]
fn extract_cwd_from_session(sessions_dir: &Path, jsonl_path: &Path) -> Option<String> {
    let filename = jsonl_path.file_name()?.to_str()?;

    if filename == "messages.jsonl" {
        // V3: session dir contains session.json
        let session_dir = jsonl_path.parent()?;
        let meta_path = session_dir.join("session.json");
        if meta_path.exists() {
            let data: Value = serde_json::from_str(
                &std::fs::read_to_string(&meta_path).ok()?
            ).ok()?;
            let paths = data.get("workspacePaths")?.as_array()?;
            return paths.first()?.as_str().map(|s| s.to_string());
        }
    } else if filename.ends_with(".jsonl") {
        // V2: look for <session_id>.json alongside the JSONL
        let session_id = jsonl_path.file_stem()?.to_str()?;
        let json_path = sessions_dir.join(format!("{}.json", session_id));
        if json_path.exists() {
            let data: Value = serde_json::from_str(
                &std::fs::read_to_string(&json_path).ok()?
            ).ok()?;
            return data.get("cwd")?.as_str().map(|s| s.to_string());
        }
    }

    None
}

// =============================================================================
// Markdown chunking (for import)
// =============================================================================

/// Chunk markdown by heading boundaries, splitting oversized sections at paragraphs.
fn chunk_markdown(content: &str) -> Vec<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in &lines {
        if line.starts_with("## ") && !current.is_empty() {
            sections.push(current);
            current = vec![line];
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        sections.push(current);
    }

    let mut chunks = Vec::new();
    for section in sections {
        let section_text = section.join("\n");
        let trimmed = section_text.trim();
        if trimmed.is_empty() || trimmed.len() < MIN_CHUNK_SIZE {
            continue;
        }

        if trimmed.len() <= CHUNK_SIZE {
            chunks.push(trimmed.to_string());
        } else {
            // Split at paragraph boundaries
            let paragraphs: Vec<&str> = trimmed.split("\n\n").collect();
            let mut buf: Vec<&str> = Vec::new();
            let mut buf_len = 0;

            for para in &paragraphs {
                if buf_len + para.len() > CHUNK_SIZE && !buf.is_empty() {
                    chunks.push(buf.join("\n\n"));
                    buf = vec![para];
                    buf_len = para.len();
                } else {
                    buf.push(para);
                    buf_len += para.len() + 2;
                }
            }
            if !buf.is_empty() {
                let remainder = buf.join("\n\n");
                if remainder.len() >= MIN_CHUNK_SIZE {
                    chunks.push(remainder);
                }
            }
        }
    }

    chunks
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


// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- chunk_messages tests ---

    #[test]
    fn chunk_messages_single_short_message() {
        let messages = vec![
            Message { role: Role::User, text: "Hello, how does the scan cache work?".to_string() },
            Message { role: Role::Assistant, text: "It stores mtime and size per file.".to_string() },
        ];
        let chunks = chunk_messages(&messages);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].starts_with("> Hello"));
        assert!(chunks[0].contains("It stores mtime"));
    }

    #[test]
    fn chunk_messages_splits_at_size_limit() {
        let messages = vec![
            Message { role: Role::User, text: "a".repeat(500) },
            Message { role: Role::Assistant, text: "b".repeat(500) },
        ];
        let chunks = chunk_messages(&messages);
        assert!(chunks.len() >= 2, "should split since total > CHUNK_SIZE (800)");
    }

    #[test]
    fn chunk_messages_user_prefixed() {
        let messages = vec![
            Message { role: Role::User, text: "user question".to_string() },
            Message { role: Role::Assistant, text: "assistant answer".to_string() },
        ];
        let chunks = chunk_messages(&messages);
        assert!(chunks[0].contains("> user question"));
        assert!(chunks[0].contains("assistant answer"));
        assert!(!chunks[0].contains("> assistant answer"));
    }

    #[test]
    fn chunk_messages_discards_tiny_chunks() {
        let messages = vec![
            Message { role: Role::User, text: "hi".to_string() },
        ];
        let chunks = chunk_messages(&messages);
        assert!(chunks.is_empty(), "chunk '> hi' is < MIN_CHUNK_SIZE (30)");
    }

    #[test]
    fn chunk_messages_empty_input() {
        let chunks = chunk_messages(&[]);
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_messages_many_small_messages_accumulate() {
        let messages: Vec<Message> = (0..20).map(|i| Message {
            role: if i % 2 == 0 { Role::User } else { Role::Assistant },
            text: format!("Message number {} with some content here", i),
        }).collect();
        let chunks = chunk_messages(&messages);
        // 20 messages × ~40 chars = ~800 chars, should produce 1-2 chunks
        assert!(!chunks.is_empty());
        assert!(chunks.len() <= 3);
    }

    // --- parse format detection tests ---

    #[test]
    fn parse_kiro_v3_valid() {
        let content = r#"{"id":"1","timestamp":"2026-07-20T10:00:00Z","payload":{"type":"user","content":"hello world"}}
{"id":"2","timestamp":"2026-07-20T10:00:05Z","payload":{"type":"assistant","content":"hi there, this is a response with enough content to matter"}}"#;
        let messages = parse_kiro_v3(content);
        assert!(messages.is_some());
        let msgs = messages.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
    }

    #[test]
    fn parse_kiro_v3_rejects_non_v3() {
        let content = r#"{"role":"user","content":"not v3 format"}"#;
        assert!(parse_kiro_v3(content).is_none());
    }

    #[test]
    fn parse_kiro_v3_merges_consecutive_assistant() {
        let content = r#"{"id":"1","timestamp":"t","payload":{"type":"user","content":"question here with enough text to be meaningful"}}
{"id":"2","timestamp":"t","payload":{"type":"assistant","content":"first part of the answer"}}
{"id":"3","timestamp":"t","payload":{"type":"assistant","content":"second part of the answer"}}"#;
        let msgs = parse_kiro_v3(content).unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(msgs[1].text.contains("first part"));
        assert!(msgs[1].text.contains("second part"));
    }

    #[test]
    fn parse_kiro_v2_valid() {
        let content = r#"{"version":"v1","kind":"Prompt","data":{"content":[{"kind":"text","data":"What is the architecture?"}]}}
{"version":"v1","kind":"AssistantMessage","data":{"content":[{"kind":"text","data":"The architecture uses a layered approach with SQLite for persistence."}]}}"#;
        let messages = parse_kiro_v2(content);
        assert!(messages.is_some());
        let msgs = messages.unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].text.contains("architecture"));
    }

    #[test]
    fn parse_kiro_v2_summarizes_tool_use() {
        let content = r#"{"version":"v1","kind":"Prompt","data":{"content":[{"kind":"text","data":"Read the file and tell me what is in it please"}]}}
{"version":"v1","kind":"AssistantMessage","data":{"content":[{"kind":"toolUse","data":{"name":"read","input":{"__tool_use_purpose":"Read config file"}}},{"kind":"text","data":"The file contains configuration settings for the project build system."}]}}"#;
        let msgs = parse_kiro_v2(content).unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(msgs[1].text.contains("[tool: read]"));
        assert!(msgs[1].text.contains("Read config file"));
    }

    #[test]
    fn parse_codex_valid() {
        let content = r#"{"type":"session_meta","payload":{"session_id":"test"}}
{"type":"event_msg","payload":{"type":"user_message","message":"Explain the RRF algorithm used for combining search results together"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"RRF combines ranked lists by computing reciprocal rank scores and summing them per document"}}"#;
        let messages = parse_codex(content);
        assert!(messages.is_some());
        let msgs = messages.unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn parse_codex_requires_session_meta() {
        let content = r#"{"type":"event_msg","payload":{"type":"user_message","message":"hello there friend"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"hi back to you friend"}}"#;
        assert!(parse_codex(content).is_none(), "codex format requires session_meta");
    }

    // --- classify_room tests ---

    #[test]
    fn classify_room_technical() {
        assert_eq!(classify_room("The bug in the API server caused a deployment error"), "technical");
    }

    #[test]
    fn classify_room_architecture() {
        assert_eq!(classify_room("The architecture uses a layered design pattern with module interfaces"), "architecture");
    }

    #[test]
    fn classify_room_decisions() {
        assert_eq!(classify_room("We decided to chose this approach as our recommendation"), "decisions");
    }

    #[test]
    fn classify_room_general_fallback() {
        assert_eq!(classify_room("The weather is nice today and I like cats"), "general");
    }

    #[test]
    fn classify_room_highest_score_wins() {
        // "architecture" keywords: architecture, design, pattern, structure
        // "technical" keywords: code, function, bug, error
        let text = "The architecture design pattern and structure of this module is important";
        assert_eq!(classify_room(text), "architecture");
    }

    #[test]
    fn classify_room_utf8_safe() {
        let text = "│".repeat(1000) + " some code with a bug in the api";
        // Should not panic on multi-byte content
        let room = classify_room(&text);
        assert!(!room.is_empty());
    }

    // --- chunk_markdown tests ---

    #[test]
    fn chunk_markdown_splits_at_headings() {
        let content = "# Title\nSome intro text that is long enough to be a chunk.\n\n## Section One\nContent for section one is here.\n\n## Section Two\nContent for section two is here.";
        let chunks = chunk_markdown(content);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn chunk_markdown_drops_tiny_sections() {
        let content = "## A\nhi\n\n## B\nThis section has enough content to survive the minimum size filter.";
        let chunks = chunk_markdown(content);
        // "hi" is < MIN_CHUNK_SIZE, should be dropped
        assert!(chunks.iter().all(|c| c.len() >= MIN_CHUNK_SIZE));
    }

    #[test]
    fn chunk_markdown_splits_oversized_at_paragraphs() {
        let long_para = "word ".repeat(200); // ~1000 chars
        let content = format!("## Big Section\n{}\n\n{}", long_para, long_para);
        let chunks = chunk_markdown(&content);
        assert!(chunks.len() >= 2, "oversized section should split at paragraph boundary");
    }

    #[test]
    fn chunk_markdown_empty_input() {
        let chunks = chunk_markdown("");
        assert!(chunks.is_empty());
    }

    // --- detect_type_from_frontmatter tests ---

    #[test]
    fn frontmatter_extracts_type() {
        let content = "---\ntitle: Test\ntype: decision\n---\n# Content";
        assert_eq!(detect_type_from_frontmatter(content), "decision");
    }

    #[test]
    fn frontmatter_defaults_to_document() {
        let content = "# No frontmatter here\nJust content.";
        assert_eq!(detect_type_from_frontmatter(content), "document");
    }

    #[test]
    fn frontmatter_handles_bom() {
        let content = "\u{feff}---\ntype: spec\n---\n# Spec";
        assert_eq!(detect_type_from_frontmatter(content), "spec");
    }
}
