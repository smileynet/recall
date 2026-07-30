use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde_json::Value;

use crate::{embed, scan, store};

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

    eprintln!("  Done: {} files, {} chunks ingested", changed.len(), total_chunks);
    Ok(0)
}

/// Import markdown files from a directory into a wing.
/// Uses SHA-256 content hashing to skip unchanged files on re-import.
pub fn import_directory(path: &str, wing: &str) -> Result<i32> {
    let dir = Path::new(path);
    if !dir.is_dir() {
        anyhow::bail!("not a directory: {}", path);
    }

    let conn = store::open_db()?;
    let embedder = embed::Embedder::new()?;

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

    eprintln!("  Importing: {}", dir.display());
    eprintln!("  Wing: {}", wing);
    eprintln!("  Files: {} markdown", md_files.len());

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

#[derive(PartialEq, Clone, Copy)]
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
    let text_lower = &text[..text.len().min(3000)].to_lowercase();
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
