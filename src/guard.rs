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

/// Default maximum execution time for long-running commands (30 minutes).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

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
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(e) => {
                return Err(anyhow::Error::from(e).context("failed to acquire process lock"))
            }
        }

        // Write PID for diagnostics
        use std::io::Write;
        let mut f = &file;
        let _ = writeln!(f, "{}", std::process::id());

        Ok(Some(Self { _file: file }))
    }
}

/// Install a hard execution timeout. Spawns a background thread that will
/// terminate the process if the timeout is exceeded.
///
/// Uses the `RECALL_TIMEOUT` env var (seconds) if set, otherwise defaults
/// to 30 minutes. Set `RECALL_TIMEOUT=0` to disable.
pub fn install_timeout() {
    let timeout = match std::env::var("RECALL_TIMEOUT") {
        Ok(val) => match val.parse::<u64>() {
            Ok(0) => return, // explicitly disabled
            Ok(secs) => Duration::from_secs(secs),
            Err(_) => DEFAULT_TIMEOUT,
        },
        Err(_) => DEFAULT_TIMEOUT,
    };

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
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| anyhow::anyhow!("neither USERPROFILE nor HOME is set"))?;
    Ok(PathBuf::from(home).join(".recall").join("recall-process.lock"))
}
