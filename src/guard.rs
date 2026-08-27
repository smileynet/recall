//! Process-level single-instance guard and execution timeout.
//!
//! Prevents multiple recall.exe instances from running long operations
//! concurrently. Uses an exclusive file lock that auto-releases on crash.
//! Also enforces a hard execution timeout to prevent runaway processes.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use fs2::FileExt;

use crate::recall_log;

/// Default maximum execution time for long-running commands (90 minutes).
/// Measured full ingest is ~68 min; 90 min gives ~30% headroom so the
/// scheduled task's own guard doesn't kill its documented workload.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(90 * 60);

/// Floor for workload-scaled timeouts — never fire before this (covers model
/// cold start + small batches).
const SCALED_FLOOR: Duration = Duration::from_secs(60);

/// Ceiling for workload-scaled timeouts — hard upper bound regardless of size.
const SCALED_CEILING: Duration = Duration::from_secs(2 * 60 * 60);

/// Per-file budget for workload scaling. Measured embed cost is ~24ms/file;
/// 100ms gives a 4x safety margin for I/O and slower machines.
const PER_FILE_BUDGET: Duration = Duration::from_millis(100);

/// RAII guard that holds an exclusive file lock for the process lifetime.
/// Dropping this releases the lock.
pub struct ProcessGuard {
    _file: std::fs::File,
}

impl ProcessGuard {
    /// Attempt to acquire the global process lock.
    ///
    /// Returns `Ok(Some(guard))` if acquired, `Ok(None)` if another instance
    /// holds the lock (caller should exit gracefully), or `Err` on I/O failure.
    pub fn try_acquire() -> Result<Option<Self>> {
        let lock_path = lock_path()?;
        std::fs::create_dir_all(lock_path.parent().unwrap())?;
        let file = std::fs::File::create(&lock_path)
            .with_context(|| format!("failed to create lock file: {}", lock_path.display()))?;

        match file.try_lock_exclusive() {
            Ok(()) => {}
            // Lock is held by another instance → caller should skip gracefully.
            // fs2 reports this as WouldBlock on Unix; on Windows the raw OS error
            // is ERROR_LOCK_VIOLATION (33), which maps to an uncategorized kind.
            Err(e) if is_lock_contended(&e) => return Ok(None),
            Err(e) => return Err(anyhow::Error::from(e).context("failed to acquire process lock")),
        }

        // Write PID for diagnostics
        use std::io::Write;
        let mut f = &file;
        let _ = writeln!(f, "{}", std::process::id());

        Ok(Some(Self { _file: file }))
    }
}

/// Returns true if the error indicates the lock is held by another process
/// (as opposed to a real I/O failure). Unix reports `WouldBlock`; Windows
/// reports `ERROR_SHARING_VIOLATION` (32) or `ERROR_LOCK_VIOLATION` (33).
fn is_lock_contended(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    matches!(e.raw_os_error(), Some(32) | Some(33))
}

/// Pure timeout resolution from an optional env value. Kept separate from env
/// access so it can be unit-tested without mutating process-global state.
/// `None` env → default; `Some("0")` → disabled (`None`); valid → that value;
/// invalid → default (caller warns).
fn resolve_timeout_from(env_val: Option<&str>, default: Duration) -> Option<Duration> {
    match env_val {
        None => Some(default),
        Some(val) => match val.parse::<u64>() {
            Ok(0) => None,
            Ok(secs) => Some(Duration::from_secs(secs)),
            Err(_) => Some(default),
        },
    }
}

/// Compute the workload-scaled timeout: `floor + per_file × count`, clamped.
fn scaled_timeout(file_count: usize) -> Duration {
    SCALED_FLOOR
        .saturating_add(PER_FILE_BUDGET.saturating_mul(file_count as u32))
        .min(SCALED_CEILING)
}

/// Resolve the timeout duration. `RECALL_TIMEOUT` (seconds) overrides everything
/// if set: `0` disables (returns `None`), a valid value wins, and an invalid
/// value warns and falls back to `default`.
fn resolve_timeout(default: Duration) -> Option<Duration> {
    let raw = std::env::var("RECALL_TIMEOUT").ok();
    if let Some(ref val) = raw {
        if val.parse::<u64>().is_err() {
            eprintln!(
                "recall: warning: invalid RECALL_TIMEOUT='{}' (want seconds), using {}s",
                val,
                default.as_secs()
            );
        }
    }
    resolve_timeout_from(raw.as_deref(), default)
}

/// Install a hard execution timeout using the fixed default (90 min).
/// `RECALL_TIMEOUT` env var (seconds) overrides; `RECALL_TIMEOUT=0` disables.
pub fn install_timeout() {
    if let Some(timeout) = resolve_timeout(DEFAULT_TIMEOUT) {
        spawn_timeout(timeout);
    }
}

/// Install a workload-scaled execution timeout: `floor + per_file × file_count`,
/// clamped to `[SCALED_FLOOR, SCALED_CEILING]`. `RECALL_TIMEOUT` still overrides.
/// Use for ingest/import where duration is proportional to file count.
pub fn install_timeout_scaled(file_count: usize) {
    if let Some(timeout) = resolve_timeout(scaled_timeout(file_count)) {
        spawn_timeout(timeout);
    }
}

/// Spawn the watchdog thread that terminates the process after `timeout`.
fn spawn_timeout(timeout: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        recall_log!(
            "TIMEOUT: process exceeded {}s limit, terminating",
            timeout.as_secs()
        );
        eprintln!(
            "recall: execution timeout ({}s) exceeded, terminating",
            timeout.as_secs()
        );
        std::process::exit(2);
    });
}

fn lock_path() -> Result<PathBuf> {
    let db = crate::store::db_path();
    let has_parent = db
        .parent()
        .map(|p| !p.as_os_str().is_empty())
        .unwrap_or(false);
    if has_parent {
        Ok(db.with_extension("lock"))
    } else {
        // db_path is a bare filename (e.g. RECALL_DB=foo.sqlite3) — keep the lock
        // beside it in the CWD.
        let name = db.file_name().and_then(|f| f.to_str()).unwrap_or("recall");
        Ok(PathBuf::from(format!("{}.lock", name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_timeout_none_uses_default() {
        assert_eq!(
            resolve_timeout_from(None, Duration::from_secs(42)),
            Some(Duration::from_secs(42))
        );
    }

    #[test]
    fn resolve_timeout_zero_disables() {
        assert_eq!(
            resolve_timeout_from(Some("0"), Duration::from_secs(42)),
            None
        );
    }

    #[test]
    fn resolve_timeout_valid_wins() {
        assert_eq!(
            resolve_timeout_from(Some("300"), Duration::from_secs(42)),
            Some(Duration::from_secs(300))
        );
    }

    #[test]
    fn resolve_timeout_invalid_falls_back_to_default() {
        assert_eq!(
            resolve_timeout_from(Some("banana"), Duration::from_secs(42)),
            Some(Duration::from_secs(42))
        );
        assert_eq!(
            resolve_timeout_from(Some("-5"), Duration::from_secs(42)),
            Some(Duration::from_secs(42))
        );
    }

    #[test]
    fn scaled_timeout_respects_floor() {
        // Zero files → floor.
        assert_eq!(scaled_timeout(0), SCALED_FLOOR);
    }

    #[test]
    fn scaled_timeout_scales_with_count() {
        // 100 files × 100ms = 10s, + 60s floor = 70s.
        assert_eq!(scaled_timeout(100), Duration::from_secs(70));
    }

    #[test]
    fn scaled_timeout_respects_ceiling() {
        // A huge count must clamp to the ceiling, not overflow.
        assert_eq!(scaled_timeout(usize::MAX), SCALED_CEILING);
        assert_eq!(scaled_timeout(10_000_000), SCALED_CEILING);
    }
}
