use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use jwalk::WalkDir;
use sha2::{Digest, Sha256};

use crate::store;

/// Scan a directory for JSONL files and return those that have changed.
pub fn scan_for_changes(dir: &Path, conn: &rusqlite::Connection) -> Result<Vec<PathBuf>> {
    let mut changed = Vec::new();

    for entry in WalkDir::new(dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
    {
        let path = entry.path();
        let meta = entry.metadata()?;
        let mtime = meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let size = meta.len() as i64;
        let path_str = path.to_string_lossy().to_string();

        // Check scan cache
        if let Some((cached_mtime, cached_size, _)) = store::get_scan_entry(conn, &path_str)? {
            if cached_mtime == mtime && cached_size == size {
                continue; // unchanged
            }
        }

        changed.push(path);
    }

    Ok(changed)
}

/// Compute SHA-256 of a file's content.
pub fn file_hash(path: &Path) -> Result<String> {
    let content = std::fs::read(path)?;
    let hash = Sha256::digest(&content);
    Ok(format!("{:x}", hash)[..16].to_string())
}

/// Update the scan cache entry for a file.
pub fn update_cache(conn: &rusqlite::Connection, path: &Path) -> Result<()> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let size = meta.len() as i64;
    let hash = file_hash(path)?;
    let path_str = path.to_string_lossy().to_string();
    store::set_scan_entry(conn, &path_str, mtime, size, &hash)?;
    Ok(())
}
