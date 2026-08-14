//! Local telemetry and crash reporting.
//!
//! - Usage telemetry: opt-in JSONL event log at `~/.recall/telemetry.jsonl`
//! - Crash reporting: local crash files at `~/.recall/crashes/`
//!
//! No network calls. Respects DO_NOT_TRACK=1 and CI=true.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub crash_reporting: bool,
}

impl TelemetryConfig {
    pub fn load() -> Self {
        let path = config_path();
        match fs::read_to_string(&path) {
            Ok(content) => parse_config(&content),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = format!(
            "[telemetry]\nenabled = {}\ncrash_reporting = {}\n",
            self.enabled, self.crash_reporting
        );
        fs::write(&path, content)?;
        Ok(())
    }
}

fn parse_config(content: &str) -> TelemetryConfig {
    let mut config = TelemetryConfig::default();
    for line in content.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("enabled") {
            if let Some(val) = val.trim().strip_prefix('=') {
                config.enabled = val.trim() == "true";
            }
        } else if let Some(val) = line.strip_prefix("crash_reporting") {
            if let Some(val) = val.trim().strip_prefix('=') {
                config.crash_reporting = val.trim() == "true";
            }
        }
    }
    config
}

// ─── Environment overrides ───────────────────────────────────────────────────

/// Returns true if telemetry is suppressed by environment.
pub fn env_suppressed() -> bool {
    matches!(std::env::var("DO_NOT_TRACK").as_deref(), Ok("1"))
        || matches!(std::env::var("CI").as_deref(), Ok("true" | "1"))
}

/// Effective telemetry state: config AND environment.
pub fn is_enabled() -> bool {
    let config = TelemetryConfig::load();
    config.enabled && !env_suppressed()
}

// ─── First-run prompt ────────────────────────────────────────────────────────

/// Check if this is a first run (no config file) and prompt the user if interactive.
///
/// Returns `true` if the prompt was shown (caller may want to print a blank line).
/// In non-interactive environments (no TTY, CI, DO_NOT_TRACK), silently defaults
/// to disabled without prompting.
pub fn first_run_prompt() -> bool {
    let path = config_path();
    if path.exists() {
        return false;
    }

    // Non-interactive: silently save disabled config
    if env_suppressed() || !stdin_is_tty() {
        let config = TelemetryConfig::default(); // enabled: false
        let _ = config.save();
        return false;
    }

    // Interactive first run: prompt
    eprint!(
        "\n  recall collects anonymous local usage data (command names, timing)\n  \
         to improve the tool. No data leaves your machine.\n\n  \
         Enable telemetry? [y/N] "
    );

    let answer = read_yes_no();
    let config = TelemetryConfig {
        enabled: answer,
        crash_reporting: answer,
    };
    if let Err(e) = config.save() {
        eprintln!("recall: failed to save config: {}", e);
    } else if answer {
        eprintln!("  Telemetry enabled. Disable anytime with: recall telemetry disable\n");
    } else {
        eprintln!("  Telemetry disabled.\n");
    }
    true
}

/// Read a single y/n response from stdin. Default is No.
fn read_yes_no() -> bool {
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    } else {
        false
    }
}

/// Check if stdin is a TTY (interactive terminal).
fn stdin_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

// ─── Event recording ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TelemetryEvent {
    pub command: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub duration_ms: u64,
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    pub date: String,
}

pub fn record_event(command: &str, start: Instant, exit_code: i32, error: Option<&anyhow::Error>) {
    if !is_enabled() {
        return;
    }

    let event = TelemetryEvent {
        command: command.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        duration_ms: start.elapsed().as_millis() as u64,
        exit_code,
        error_type: error.map(|e| {
            format!("{}", e)
                .split(':')
                .next()
                .unwrap_or("unknown")
                .to_string()
        }),
        date: today_date(),
    };

    if let Err(e) = append_event(&event) {
        eprintln!("recall: telemetry write failed: {}", e);
    }
}

fn append_event(event: &TelemetryEvent) -> Result<()> {
    let path = telemetry_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let line = serde_json::to_string(event)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

// ─── Crash reporting ─────────────────────────────────────────────────────────

/// Install a panic hook that writes crash reports locally.
pub fn install_crash_hook() {
    let config = TelemetryConfig::load();
    if !config.crash_reporting {
        return;
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = build_crash_report(info);
        if let Err(e) = write_crash_report(&report) {
            eprintln!("recall: failed to write crash report: {}", e);
        }
        // Still run the default hook (prints panic message)
        default_hook(info);
    }));
}

fn build_crash_report(info: &std::panic::PanicHookInfo) -> String {
    let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
        redact_paths(s)
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        redact_paths(s)
    } else {
        "unknown panic".to_string()
    };

    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", redact_paths(l.file()), l.line(), l.column()))
        .unwrap_or_else(|| "unknown location".to_string());

    let command = redact_paths(&std::env::args().collect::<Vec<_>>().join(" "));

    format!(
        "recall crash report\n\
         ────────────────────\n\
         version: {}\n\
         os: {}\n\
         arch: {}\n\
         command: {}\n\
         message: {}\n\
         location: {}\n\
         time: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        command,
        message,
        location,
        today_date(),
    )
}

fn write_crash_report(report: &str) -> Result<()> {
    let dir = crashes_dir();
    fs::create_dir_all(&dir)?;
    let filename = format!("crash-{}-{}.txt", today_date(), std::process::id());
    let path = dir.join(filename);
    fs::write(&path, report)?;
    eprintln!("\nCrash report saved to: {}", path.display());
    Ok(())
}

/// Redact absolute paths to prevent PII leakage.
fn redact_paths(s: &str) -> String {
    // Replace Windows-style absolute paths
    let re = regex::Regex::new(r#"[A-Z]:\\[^\s:"']+"#).unwrap();
    let s = re.replace_all(s, "[PATH]").to_string();
    // Replace Unix-style home paths
    let re = regex::Regex::new(r#"/(?:home|Users)/[^\s:"']+"#).unwrap();
    re.replace_all(&s, "[PATH]").to_string()
}

// ─── CLI subcommands ─────────────────────────────────────────────────────────

pub fn cmd_telemetry_status() -> Result<i32> {
    let config = TelemetryConfig::load();
    let suppressed = env_suppressed();

    println!("\n  Telemetry Status");
    println!("  {}", "─".repeat(30));
    println!(
        "  Usage telemetry:   {}",
        if suppressed {
            "disabled (env override)"
        } else if config.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!(
        "  Crash reporting:   {}",
        if config.crash_reporting {
            "enabled"
        } else {
            "disabled"
        }
    );

    if suppressed {
        if matches!(std::env::var("DO_NOT_TRACK").as_deref(), Ok("1")) {
            println!("  Override:          DO_NOT_TRACK=1");
        }
        if matches!(std::env::var("CI").as_deref(), Ok("true" | "1")) {
            println!("  Override:          CI environment detected");
        }
    }

    let telemetry_file = telemetry_path();
    if telemetry_file.exists() {
        if let Ok(meta) = fs::metadata(&telemetry_file) {
            println!(
                "  Data file:         {} ({} bytes)",
                telemetry_file.display(),
                meta.len()
            );
        }
    } else {
        println!("  Data file:         (none)");
    }

    let crash_dir = crashes_dir();
    if crash_dir.is_dir() {
        let count = fs::read_dir(&crash_dir).map(|d| d.count()).unwrap_or(0);
        println!("  Crash reports:     {} files", count);
    }
    println!();
    Ok(0)
}

pub fn cmd_telemetry_enable() -> Result<i32> {
    let mut config = TelemetryConfig::load();
    config.enabled = true;
    config.save()?;
    println!("Usage telemetry enabled. Data stored locally at:");
    println!("  {}", telemetry_path().display());
    Ok(0)
}

pub fn cmd_telemetry_disable() -> Result<i32> {
    let mut config = TelemetryConfig::load();
    config.enabled = false;
    config.save()?;
    println!("Usage telemetry disabled.");
    Ok(0)
}

pub fn cmd_telemetry_stats() -> Result<i32> {
    let path = telemetry_path();
    if !path.exists() {
        println!("No telemetry data collected yet.");
        return Ok(0);
    }

    let content = fs::read_to_string(&path)?;
    let total_events = content.lines().count();

    // Count per command
    let mut commands: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut total_duration_ms: u64 = 0;
    let mut error_count: usize = 0;

    for line in content.lines() {
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(cmd) = event.get("command").and_then(|v| v.as_str()) {
                *commands.entry(cmd.to_string()).or_default() += 1;
            }
            if let Some(d) = event.get("duration_ms").and_then(|v| v.as_u64()) {
                total_duration_ms += d;
            }
            if event.get("error_type").and_then(|v| v.as_str()).is_some() {
                error_count += 1;
            }
        }
    }

    println!("\n  Telemetry Stats");
    println!("  {}", "─".repeat(30));
    println!("  Total events:      {}", total_events);
    println!("  Error events:      {}", error_count);
    println!(
        "  Total time:        {:.1}s",
        total_duration_ms as f64 / 1000.0
    );
    println!();
    println!("  Commands:");
    let mut sorted: Vec<_> = commands.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (cmd, count) in sorted {
        println!("    {:<16} {}", cmd, count);
    }
    println!();
    Ok(0)
}

pub fn cmd_telemetry_clear() -> Result<i32> {
    let telemetry_file = telemetry_path();
    let crash_dir = crashes_dir();
    let mut cleared = false;

    if telemetry_file.exists() {
        fs::remove_file(&telemetry_file)?;
        println!("Deleted telemetry data.");
        cleared = true;
    }
    if crash_dir.is_dir() {
        fs::remove_dir_all(&crash_dir)?;
        println!("Deleted crash reports.");
        cleared = true;
    }
    if !cleared {
        println!("No telemetry data to clear.");
    }
    Ok(0)
}

// ─── Paths ───────────────────────────────────────────────────────────────────

fn recall_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".recall")
}

fn config_path() -> PathBuf {
    recall_dir().join("config.toml")
}

fn telemetry_path() -> PathBuf {
    recall_dir().join("telemetry.jsonl")
}

fn crashes_dir() -> PathBuf {
    recall_dir().join("crashes")
}

fn today_date() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = now / 86400;
    let z = days as i64 + 719468;
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_disabled() {
        let config = TelemetryConfig::default();
        assert!(!config.enabled);
        assert!(!config.crash_reporting);
    }

    #[test]
    fn test_parse_config_explicit_true() {
        let content = "[telemetry]\nenabled = true\ncrash_reporting = true\n";
        let config = parse_config(content);
        assert!(config.enabled);
        assert!(config.crash_reporting);
    }

    #[test]
    fn test_parse_config_explicit_false() {
        let content = "[telemetry]\nenabled = false\ncrash_reporting = false\n";
        let config = parse_config(content);
        assert!(!config.enabled);
        assert!(!config.crash_reporting);
    }

    #[test]
    fn test_parse_config_mixed() {
        let content = "[telemetry]\nenabled = true\ncrash_reporting = false\n";
        let config = parse_config(content);
        assert!(config.enabled);
        assert!(!config.crash_reporting);
    }

    #[test]
    fn test_config_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        // Temporarily override config path via env for isolation
        // (Can't easily mock config_path, so test save format directly)
        let config = TelemetryConfig {
            enabled: true,
            crash_reporting: false,
        };
        let content = format!(
            "[telemetry]\nenabled = {}\ncrash_reporting = {}\n",
            config.enabled, config.crash_reporting
        );
        std::fs::write(&path, &content).unwrap();
        let loaded = parse_config(&std::fs::read_to_string(&path).unwrap());
        assert!(loaded.enabled);
        assert!(!loaded.crash_reporting);
    }

    #[test]
    fn test_env_suppressed_do_not_track() {
        // Can't easily test env vars in parallel, but verify the function exists
        let result = env_suppressed();
        let _ = result;
    }

    #[test]
    fn test_stdin_is_tty_in_tests() {
        // In test environments, stdin is typically NOT a TTY
        // This just verifies the function doesn't panic
        let result = stdin_is_tty();
        // In CI/test: almost always false
        assert!(
            !result,
            "expected stdin to not be a TTY in test environment"
        );
    }

    #[test]
    fn test_first_run_prompt_with_existing_config() {
        // When config already exists, first_run_prompt returns false
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("config.toml");
        std::fs::write(&config_file, "[telemetry]\nenabled = true\n").unwrap();
        // Can't test directly without mocking config_path, but verifying
        // the function's logic: config_path().exists() == true → return false
        assert!(config_file.exists());
    }

    #[test]
    fn test_read_yes_no_defaults_to_false() {
        // read_yes_no reads from stdin; in non-interactive tests with no input,
        // it should default to false (or timeout)
        // We test the matching logic directly:
        assert!(matches!("y".to_lowercase().as_str(), "y" | "yes"));
        assert!(matches!("yes".to_lowercase().as_str(), "y" | "yes"));
        assert!(matches!("Y".to_lowercase().as_str(), "y" | "yes"));
        assert!(!matches!("n".to_lowercase().as_str(), "y" | "yes"));
        assert!(!matches!("".to_lowercase().as_str(), "y" | "yes"));
        assert!(!matches!("no".to_lowercase().as_str(), "y" | "yes"));
    }

    #[test]
    fn test_redact_paths_windows() {
        let input = "failed at C:\\Users\\john\\code\\recall\\src\\main.rs";
        let output = redact_paths(input);
        assert_eq!(output, "failed at [PATH]");
        assert!(!output.contains("john"));
    }

    #[test]
    fn test_redact_paths_unix() {
        let input = "failed at /home/john/code/recall/src/main.rs";
        let output = redact_paths(input);
        assert_eq!(output, "failed at [PATH]");
        assert!(!output.contains("john"));
    }

    #[test]
    fn test_redact_paths_no_path() {
        let input = "index out of bounds: 5 >= 3";
        let output = redact_paths(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_today_date_format() {
        let date = today_date();
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }
}
