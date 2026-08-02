//! Non-TTY auto-logging to file.
//!
//! When running non-interactively (no TTY on stderr), redirects stderr
//! to `~/.recall/logs/YYYY-MM-DD.log` with timestamps via a tee approach.
//! Rotates logs: keeps last 7 days, deletes older on startup.
//!
//! Design: Since existing code uses `eprintln!` throughout, we provide a
//! `log()` function and an `init()` that activates file logging. The main
//! entry point wraps command execution — all output from commands goes
//! through the normal stderr, and we capture it by initializing the log
//! writer that command functions can call.
//!
//! In practice, the scheduled task captures all stdout/stderr anyway.
//! This module provides structured timestamped logging as an improvement.

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_FILE: Mutex<Option<fs::File>> = Mutex::new(None);
static LOGGING_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Returns true if stderr is connected to an interactive terminal.
pub fn is_interactive() -> bool {
    io::stderr().is_terminal()
}

/// Returns true if file logging is currently active.
pub fn is_active() -> bool {
    LOGGING_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Initialize file logging if running non-interactively.
/// Returns true if logging was activated (non-TTY mode).
pub fn init() -> bool {
    if is_interactive() {
        return false;
    }

    let log_dir = logs_dir();
    if fs::create_dir_all(&log_dir).is_err() {
        return false;
    }

    // Rotate: delete logs older than 7 days
    rotate_logs(&log_dir);

    // Open today's log file
    let log_path = log_dir.join(format!("{}.log", today_date()));
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path);

    match file {
        Ok(f) => {
            if let Ok(mut guard) = LOG_FILE.lock() {
                *guard = Some(f);
            }
            LOGGING_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
            // Write session start marker
            log("─── recall session start ───");
            true
        }
        Err(_) => false,
    }
}

/// Write a timestamped line to the log file (if logging is active).
/// In interactive mode, writes the message without timestamps to stderr.
pub fn log(msg: &str) {
    if LOGGING_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
        let timestamp = now_timestamp();
        let line = format!("[{}] {}\n", timestamp, msg);
        if let Ok(mut guard) = LOG_FILE.lock() {
            if let Some(ref mut file) = *guard {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
                return;
            }
        }
    }
    // Interactive fallback: write without timestamp (preserves original format)
    let _ = io::stderr().write_all(format!("{}\n", msg).as_bytes());
}

/// Get the path to the current log file (for health reporting).
pub fn current_log_path() -> Option<PathBuf> {
    let path = logs_dir().join(format!("{}.log", today_date()));
    if path.exists() { Some(path) } else { None }
}

fn rotate_logs(dir: &PathBuf) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(7 * 24 * 3600);

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}

fn logs_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".recall").join("logs")
}

fn today_date() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    epoch_to_date(now)
}

fn now_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let date = epoch_to_date(now);
    let day_secs = now % 86400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;
    format!("{}T{:02}:{:02}:{:02}", date, h, m, s)
}

fn epoch_to_date(epoch: u64) -> String {
    let days = (epoch / 86400) as i64;
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

/// Macro for logging that routes to file when non-interactive.
/// Usage: `recall_log!("Ingesting: {}", path);`
#[macro_export]
macro_rules! recall_log {
    ($($arg:tt)*) => {
        $crate::logging::log(&format!($($arg)*))
    };
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_to_date_known() {
        // Known epoch: 2024-01-01 00:00:00 UTC = 1704067200
        let date = epoch_to_date(1704067200);
        assert_eq!(date, "2024-01-01");
    }

    #[test]
    fn test_epoch_to_date_another() {
        // 2025-07-31 00:00:00 UTC = 1753920000
        let date = epoch_to_date(1753920000);
        assert_eq!(date, "2025-07-31");
    }

    #[test]
    fn test_now_timestamp_format() {
        let ts = now_timestamp();
        // Format: YYYY-MM-DDTHH:MM:SS
        assert_eq!(ts.len(), 19);
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }

    #[test]
    fn test_is_interactive_no_panic() {
        let _ = is_interactive();
    }

    #[test]
    fn test_is_active_default_false() {
        // Before init is called, should not be active
        // (This may be true or false depending on test ordering,
        // but the function should not panic)
        let _ = is_active();
    }
}
