//! Process-level single-instance guard and execution timeout.
//!
//! Prevents multiple recall.exe instances from running long operations
//! concurrently. Uses an exclusive file lock that auto-releases on crash.
//! Also enforces a hard execution timeout to prevent runaway processes.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
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

/// Which execution timeout a corpus operation installs once it holds the lock.
///
/// The timeout policy lives here — not in the command handlers — so timeout
/// selection is part of the single exclusive-operation policy (ticket 055 AC4)
/// rather than something each handler picks by calling the right free function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutPolicy {
    /// Fixed 90-minute default. For bounded bulk writes (import, migrate) whose
    /// duration isn't proportional to a known file count.
    Fixed,
    /// Workload-scaled: `floor + per_file × file_count`, clamped. For ingest/sync
    /// where runtime grows with the number of session files to process.
    Scaled(usize),
}

impl TimeoutPolicy {
    /// Install the watchdog this policy describes. `RECALL_TIMEOUT` still
    /// overrides both variants (and `RECALL_TIMEOUT=0` disables).
    fn install(self) {
        match self {
            TimeoutPolicy::Fixed => install_timeout(),
            TimeoutPolicy::Scaled(file_count) => install_timeout_scaled(file_count),
        }
    }
}

/// Outcome of attempting an exclusive corpus operation.
///
/// Distinguishes "ran the body" from "skipped because another instance held the
/// lock". The skip carries the exit code (0 — a skip is not a failure) so a
/// future telemetry outcome field (ticket 071) can record it as `skipped`
/// rather than a success-looking event, without this policy depending on the
/// telemetry layer.
#[derive(Debug)]
pub enum OpOutcome {
    /// The body ran; this is its result.
    Ran(i32),
    /// Another instance held the lock; the body did not run. Carries the exit
    /// code to return (0 — graceful skip, not an error).
    SkippedLockHeld(i32),
}

impl OpOutcome {
    /// Collapse to the process exit code, discarding the ran/skipped distinction.
    /// Callers that only care about the exit status use this; callers that need
    /// to record a skip inspect the variant directly.
    pub fn into_exit_code(self) -> i32 {
        match self {
            OpOutcome::Ran(code) | OpOutcome::SkippedLockHeld(code) => code,
        }
    }
}

/// The single policy for exclusive, corpus-wide mutating operations.
///
/// One implementation owns lock acquisition, the lock-held skip result, timeout
/// selection, and the contention log line — so command handlers call `run`
/// instead of hand-copying a `try_acquire` match + `install_timeout*` branch
/// (ticket 055). Recall has exactly one process lock, so this is a concrete
/// policy, not a pluggable lock-backend trait.
///
/// # Classification
///
/// A command is *exclusive* (uses this) when it performs a multi-step or
/// corpus-wide mutation that an overlapping run could corrupt or partially undo
/// (e.g. ingest, import, import-all, sync, migrate, migrate-wings). A command is
/// a *safe short atomic write* (does NOT use this) when it issues only brief,
/// WAL-serialized statements with no read-modify-write over the corpus — `add`
/// (one insert + two meta upserts) is the sole such mutator. `forget` is
/// exclusive despite being "just a delete": an overlapping ingest can restore
/// content the user just deleted, so the delete must hold the lock.
pub struct ExclusiveOp {
    label: &'static str,
    timeout: TimeoutPolicy,
}

/// Begin an exclusive corpus operation named `label` (used only in the
/// contention log line). Chain `.timeout(...)` then `.run(...)`.
pub fn exclusive(label: &'static str) -> ExclusiveOp {
    ExclusiveOp {
        label,
        timeout: TimeoutPolicy::Fixed,
    }
}

impl ExclusiveOp {
    /// Set the timeout policy installed once the lock is held. Defaults to
    /// `Fixed` if never called.
    pub fn timeout(mut self, policy: TimeoutPolicy) -> Self {
        self.timeout = policy;
        self
    }

    /// Acquire the exclusive lock, install the timeout, and run `body`.
    ///
    /// If another instance holds the lock, `body` does NOT run and the result is
    /// `Ok(OpOutcome::SkippedLockHeld(0))` after logging + warning once. The lock
    /// is held for the duration of `body` and released when this returns.
    pub fn run<F>(self, body: F) -> Result<OpOutcome>
    where
        F: FnOnce() -> Result<i32>,
    {
        let _guard = match ProcessGuard::try_acquire()? {
            Some(g) => g,
            None => {
                recall_log!(
                    "{}: another recall instance is running, skipping",
                    self.label
                );
                eprintln!("recall: another instance is already running, skipping");
                return Ok(OpOutcome::SkippedLockHeld(0));
            }
        };
        self.timeout.install();
        let code = body()?;
        Ok(OpOutcome::Ran(code))
    }
}

/// Whether the global process lock is currently held by another instance, for
/// diagnostics (`recall health`). Non-blocking: try-acquire then release
/// immediately. Never mutates state.
///
/// `Free` means acquisition succeeded (no other instance). `Held { pid }` means
/// another instance holds it; `pid` is read from the lock file when present.
/// On an I/O error probing the lock we report `Free` rather than failing health
/// — the probe is advisory, and a health command must not error on a lock race.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum LockStatus {
    Free,
    Held { pid: Option<u32> },
}

impl LockStatus {
    /// Compact human-readable form for the health text view.
    pub fn describe(&self) -> String {
        match self {
            LockStatus::Free => "free".to_string(),
            LockStatus::Held { pid: Some(pid) } => format!("held (pid {})", pid),
            LockStatus::Held { pid: None } => "held".to_string(),
        }
    }
}

/// Probe the process lock without holding it. See [`LockStatus`].
pub fn probe_lock() -> LockStatus {
    let Ok(lock_path) = lock_path() else {
        return LockStatus::Free;
    };
    // The lock file may not exist yet (no exclusive op has ever run) — that is
    // unambiguously free.
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
    else {
        return LockStatus::Free;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            // We grabbed it → nobody else holds it. Release immediately so this
            // probe leaves no trace.
            let _ = FileExt::unlock(&file);
            LockStatus::Free
        }
        Err(e) if is_benign_contention(&e) => LockStatus::Held {
            pid: read_lock_pid(&lock_path),
        },
        // A genuine I/O error probing the lock is not a held lock; health stays
        // non-fatal and reports free.
        Err(_) => LockStatus::Free,
    }
}

/// Read the PID written into the lock file by the holder, if parseable.
fn read_lock_pid(lock_path: &std::path::Path) -> Option<u32> {
    let content = std::fs::read_to_string(lock_path).ok()?;
    content.lines().next()?.trim().parse().ok()
}

impl ProcessGuard {
    /// Attempt to acquire the global process lock.
    ///
    /// Returns `Ok(Some(guard))` if acquired, `Ok(None)` if another instance
    /// holds the lock (caller should exit gracefully), or `Err` on I/O failure.
    pub fn try_acquire() -> Result<Option<Self>> {
        let lock_path = lock_path()?;
        std::fs::create_dir_all(lock_path.parent().unwrap())?;

        // Opening the lockfile can itself fail with a sharing/lock violation on
        // Windows (ERROR_SHARING_VIOLATION 32) when antivirus, a search indexer,
        // or a still-closing sibling process momentarily holds a handle to it.
        // That is operationally identical to "another instance is active" — skip
        // gracefully and retry next tick rather than surfacing a hard error.
        let file = match std::fs::File::create(&lock_path) {
            Ok(f) => f,
            Err(e) if is_benign_contention(&e) => return Ok(None),
            Err(e) => {
                return Err(anyhow::Error::from(e).context(format!(
                    "failed to create lock file: {}",
                    lock_path.display()
                )))
            }
        };

        match file.try_lock_exclusive() {
            Ok(()) => {}
            // Lock is held by another instance → caller should skip gracefully.
            Err(e) if is_benign_contention(&e) => return Ok(None),
            Err(e) => return Err(anyhow::Error::from(e).context("failed to acquire process lock")),
        }

        // Write PID for diagnostics
        use std::io::Write;
        let mut f = &file;
        let _ = writeln!(f, "{}", std::process::id());

        Ok(Some(Self { _file: file }))
    }
}

/// Returns true if an I/O error means the lock is effectively held by another
/// process (or a transient handle-holder) rather than a real I/O failure — the
/// caller should skip gracefully.
///
/// Covers three cases:
/// - `WouldBlock`: `flock(LOCK_NB)` contention on Unix (and std `File::try_lock`).
/// - Windows raw OS `ERROR_SHARING_VIOLATION` (32) / `ERROR_LOCK_VIOLATION` (33):
///   fs2's `try_lock_exclusive` surfaces 33 on contention; opening the lockfile
///   can surface 32 when another handle is momentarily open. These decode to an
///   uncategorized `ErrorKind`, so we must match the raw OS code.
/// - `PermissionDenied`: a transient exclusive-open denial on the lockfile,
///   treated as contention rather than a hard failure.
fn is_benign_contention(e: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    if matches!(
        e.kind(),
        ErrorKind::WouldBlock | ErrorKind::PermissionDenied
    ) {
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

    #[test]
    fn benign_contention_wouldblock() {
        let e = std::io::Error::from(std::io::ErrorKind::WouldBlock);
        assert!(is_benign_contention(&e));
    }

    #[test]
    fn benign_contention_permission_denied() {
        // Transient exclusive-open denial on the lockfile → treat as contention.
        let e = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(is_benign_contention(&e));
    }

    #[test]
    fn benign_contention_windows_sharing_and_lock_violation() {
        // ERROR_SHARING_VIOLATION (32) at lockfile open, ERROR_LOCK_VIOLATION (33)
        // from fs2 try_lock — both are contention, regardless of platform decoding.
        assert!(is_benign_contention(&std::io::Error::from_raw_os_error(32)));
        assert!(is_benign_contention(&std::io::Error::from_raw_os_error(33)));
    }

    #[test]
    fn benign_contention_rejects_real_io_errors() {
        // Genuine failures must still propagate as Err (not a graceful skip).
        assert!(!is_benign_contention(&std::io::Error::from(
            std::io::ErrorKind::NotFound
        )));
        // ERROR_DISK_FULL (112) is a real failure, not contention.
        assert!(!is_benign_contention(&std::io::Error::from_raw_os_error(
            112
        )));
    }

    #[test]
    fn try_acquire_second_instance_skips_gracefully() {
        // Holding the lock, a second try_acquire must return Ok(None) (skip),
        // not Err — the contended path. Isolated RECALL_DB under the shared env
        // lock so it can't race the other guard tests that mutate the global.
        with_isolated_db("second-instance", |_db| {
            let first = ProcessGuard::try_acquire().expect("first acquire ok");
            assert!(first.is_some(), "first acquire should hold the lock");

            let second = ProcessGuard::try_acquire().expect("second acquire must not error");
            assert!(
                second.is_none(),
                "second acquire should skip gracefully (Ok(None)), got a guard"
            );

            drop(first);
        });
    }

    #[test]
    fn op_outcome_exit_code_collapses_both_variants() {
        assert_eq!(OpOutcome::Ran(0).into_exit_code(), 0);
        assert_eq!(OpOutcome::Ran(7).into_exit_code(), 7);
        assert_eq!(OpOutcome::SkippedLockHeld(0).into_exit_code(), 0);
    }

    #[test]
    fn lock_status_describe_forms() {
        assert_eq!(LockStatus::Free.describe(), "free");
        assert_eq!(LockStatus::Held { pid: None }.describe(), "held");
        assert_eq!(
            LockStatus::Held { pid: Some(4242) }.describe(),
            "held (pid 4242)"
        );
    }

    #[test]
    fn lock_status_serializes_tagged() {
        let free = serde_json::to_value(LockStatus::Free).unwrap();
        assert_eq!(free["state"], "free");
        let held = serde_json::to_value(LockStatus::Held { pid: Some(99) }).unwrap();
        assert_eq!(held["state"], "held");
        assert_eq!(held["pid"], 99);
    }

    /// Serialize guard tests that mutate the RECALL_DB process-global and the
    /// shared on-disk lock so they can't interleave with each other.
    static GUARD_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Run `body` with an isolated RECALL_DB pointing at a fresh temp dir, held
    /// exclusively so no sibling guard test races the process-global.
    fn with_isolated_db<T>(tag: &str, body: impl FnOnce(&std::path::Path) -> T) -> T {
        let _lock = GUARD_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = std::env::temp_dir().join(format!("recall-guard-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let db = tmp.join("recall.sqlite3");
        // SAFETY: single-threaded under GUARD_ENV_LOCK; sets a process-global.
        unsafe {
            std::env::set_var("RECALL_DB", &db);
        }
        let out = body(&db);
        unsafe {
            std::env::remove_var("RECALL_DB");
        }
        let _ = std::fs::remove_dir_all(&tmp);
        out
    }

    #[test]
    fn probe_lock_free_when_nobody_holds() {
        with_isolated_db("probe-free", |_db| {
            assert_eq!(probe_lock(), LockStatus::Free, "no holder → free");
        });
    }

    #[test]
    fn probe_lock_reports_held_with_pid_when_locked() {
        with_isolated_db("probe-held", |_db| {
            let guard = ProcessGuard::try_acquire()
                .expect("acquire ok")
                .expect("should hold");
            match probe_lock() {
                LockStatus::Held { pid } => {
                    // The holder wrote its own PID into the lock file.
                    assert_eq!(pid, Some(std::process::id()), "held reports holder PID");
                }
                LockStatus::Free => panic!("probe saw free while a guard is held"),
            }
            drop(guard);
            // Released → probe sees free again.
            assert_eq!(probe_lock(), LockStatus::Free, "after drop → free");
        });
    }

    #[test]
    fn exclusive_op_runs_body_when_lock_free() {
        with_isolated_db("op-runs", |_db| {
            let ran = std::cell::Cell::new(false);
            let outcome = exclusive("test-op")
                .run(|| {
                    ran.set(true);
                    Ok(3)
                })
                .expect("run ok");
            assert!(ran.get(), "body must run when lock is free");
            assert!(matches!(outcome, OpOutcome::Ran(3)));
        });
    }

    #[test]
    fn exclusive_op_skips_body_when_lock_held() {
        with_isolated_db("op-skips", |_db| {
            let held = ProcessGuard::try_acquire()
                .expect("acquire ok")
                .expect("should hold");
            let ran = std::cell::Cell::new(false);
            let outcome = exclusive("test-op")
                .run(|| {
                    ran.set(true);
                    Ok(0)
                })
                .expect("run ok");
            assert!(
                !ran.get(),
                "body must NOT run when another instance holds lock"
            );
            assert!(
                matches!(outcome, OpOutcome::SkippedLockHeld(0)),
                "held → SkippedLockHeld(0)"
            );
            drop(held);
        });
    }
}
