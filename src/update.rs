//! Self-update check and binary update.
//!
//! - Checks GitHub Releases for newer versions (at most once per interval)
//! - Non-blocking: runs after command completes
//! - Respects DO_NOT_TRACK, CI, non-interactive, and config settings
//! - `recall update` downloads and replaces the binary

use std::fs;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::telemetry;

const GITHUB_REPO: &str = "smileynet/recall";
const DEFAULT_INTERVAL_HOURS: u64 = 24;

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct UpdateConfig {
    pub check: bool,
    pub interval_hours: u64,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            check: true,
            interval_hours: DEFAULT_INTERVAL_HOURS,
        }
    }
}

impl UpdateConfig {
    /// Load update config from ~/.recall/config.toml
    pub fn load() -> Self {
        let path = config_path();
        match fs::read_to_string(&path) {
            Ok(content) => parse_update_config(&content),
            Err(_) => Self::default(),
        }
    }
}

fn parse_update_config(content: &str) -> UpdateConfig {
    let mut config = UpdateConfig::default();
    let mut in_update_section = false;

    for line in content.lines() {
        let line = line.trim();
        if line == "[update]" {
            in_update_section = true;
            continue;
        }
        if line.starts_with('[') {
            in_update_section = false;
            continue;
        }
        if !in_update_section {
            continue;
        }

        if let Some(val) = line.strip_prefix("check") {
            if let Some(val) = val.trim().strip_prefix('=') {
                config.check = val.trim() == "true";
            }
        } else if let Some(val) = line.strip_prefix("interval_hours") {
            if let Some(val) = val.trim().strip_prefix('=') {
                if let Ok(hours) = val.trim().parse::<u64>() {
                    config.interval_hours = hours;
                }
            }
        }
    }
    config
}

// ─── Update check ────────────────────────────────────────────────────────────

/// Check for updates if conditions are met. Prints a notice to stderr if a
/// newer version is available. Returns quietly on any error (non-blocking).
pub fn check_for_update() {
    if !should_check() {
        return;
    }

    // Run the check — swallow errors silently
    if let Ok(Some(latest)) = fetch_latest_version() {
        let current = env!("CARGO_PKG_VERSION");
        if version_is_newer(current, &latest) {
            eprintln!(
                "\n  recall: update available v{} → v{} (run `recall update` to install)\n",
                current, latest
            );
        }
    }

    // Update the last-check timestamp regardless of result
    let _ = write_last_check();
}

/// Determine if we should perform an update check.
fn should_check() -> bool {
    // Disabled by environment
    if telemetry::env_suppressed() {
        return false;
    }

    // Non-interactive (scheduled task, piped output)
    if !std::io::stderr().is_terminal() {
        return false;
    }

    // Disabled by config
    let config = UpdateConfig::load();
    if !config.check {
        return false;
    }

    // Check interval
    let interval_secs = config.interval_hours * 3600;
    if let Some(last_check) = read_last_check() {
        let now = now_epoch();
        if now.saturating_sub(last_check) < interval_secs {
            return false;
        }
    }

    true
}

/// Fetch the latest version tag from GitHub Releases API.
fn fetch_latest_version() -> Result<Option<String>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = ureq::get(&url)
        .set("Accept", "application/vnd.github.v3+json")
        .set(
            "User-Agent",
            &format!("recall/{}", env!("CARGO_PKG_VERSION")),
        )
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .context("failed to fetch latest release")?;

    let body: serde_json::Value = response
        .into_json()
        .context("failed to parse release JSON")?;

    let tag = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.strip_prefix('v').unwrap_or(s).to_string());

    Ok(tag)
}

/// Compare version strings. Returns true if `latest` is newer than `current`.
fn version_is_newer(current: &str, latest: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let c = parse(current);
    let l = parse(latest);

    // Compare each component
    for i in 0..c.len().max(l.len()) {
        let cv = c.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if lv > cv {
            return true;
        }
        if lv < cv {
            return false;
        }
    }
    false
}

// ─── Self-update command ─────────────────────────────────────────────────────

/// Download and install the latest release binary.
pub fn cmd_update() -> Result<i32> {
    eprintln!("Checking for updates...");

    let latest = fetch_latest_version()?
        .ok_or_else(|| anyhow::anyhow!("could not determine latest version"))?;

    let current = env!("CARGO_PKG_VERSION");
    if !version_is_newer(current, &latest) {
        println!("Already up to date (v{}).", current);
        return Ok(0);
    }

    println!("Updating recall v{} → v{}...", current, latest);

    let asset = find_asset_url(&latest)?;
    let archive_bytes = download_asset(&asset.url)?;
    // Hard-fail if the release didn't provide a digest, or if it doesn't match —
    // never install an unverified binary.
    verify_digest(&archive_bytes, asset.digest.as_deref())?;
    let binary = extract_binary(&archive_bytes, &asset.name)?;
    replace_self(&binary)?;

    println!("Updated to v{}.", latest);
    Ok(0)
}

/// A release asset selected for this platform.
struct AssetInfo {
    url: String,
    name: String,
    /// GitHub-computed content digest, e.g. `sha256:<hex>`. Absent on very old
    /// releases (predating GitHub's per-asset digest) — treated as a hard error.
    digest: Option<String>,
}

/// Verify downloaded bytes against a GitHub `assets[].digest` (`sha256:<hex>`).
/// Hard-fails when the digest is absent or does not match — recall refuses to
/// install an unverified self-update.
fn verify_digest(bytes: &[u8], digest: Option<&str>) -> Result<()> {
    use sha2::{Digest, Sha256};
    let digest = digest.ok_or_else(|| {
        anyhow::anyhow!(
            "release asset has no checksum (digest) — refusing to install unverified update"
        )
    })?;
    let expected = digest.strip_prefix("sha256:").ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported digest format '{}' (expected sha256:<hex>)",
            digest
        )
    })?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        anyhow::bail!(
            "checksum mismatch: expected {}, got {} — refusing to install",
            expected,
            actual
        );
    }
    Ok(())
}

/// Find the correct asset for this platform (URL + name + digest).
fn find_asset_url(version: &str) -> Result<AssetInfo> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/v{}",
        GITHUB_REPO, version
    );

    let response = http_agent()
        .get(&url)
        .set("Accept", "application/vnd.github.v3+json")
        .set(
            "User-Agent",
            &format!("recall/{}", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .context("failed to fetch release info")?;

    let body: serde_json::Value = response
        .into_json()
        .context("failed to parse release JSON")?;

    let target = platform_target();
    let assets = body
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| anyhow::anyhow!("no assets in release"))?;

    let names: Vec<&str> = assets
        .iter()
        .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
        .collect();

    // Prefer the most specific match: the full target triple (with ABI, e.g.
    // x86_64-pc-windows-msvc) beats the OS-only substring. This disambiguates
    // gnu vs musl vs msvc when a release ships multiple variants. Exclude the
    // `.sha256` sidecar assets so we never pick a checksum file as the binary.
    let full_triple = full_target_triple();
    let pick = names
        .iter()
        .filter(|n| !n.ends_with(".sha256"))
        .find(|n| n.contains(&full_triple))
        .or_else(|| {
            names
                .iter()
                .filter(|n| !n.ends_with(".sha256"))
                .find(|n| n.contains(&target))
        })
        .copied();

    if let Some(name) = pick {
        for asset in assets {
            if asset.get("name").and_then(|n| n.as_str()) == Some(name) {
                if let Some(url) = asset.get("browser_download_url").and_then(|u| u.as_str()) {
                    let digest = asset
                        .get("digest")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string());
                    return Ok(AssetInfo {
                        url: url.to_string(),
                        name: name.to_string(),
                        digest,
                    });
                }
            }
        }
    }

    anyhow::bail!("no asset found for platform '{}'", target)
}

/// Full Rust target triple including ABI (e.g. `x86_64-pc-windows-msvc`).
/// Used for exact asset disambiguation before falling back to the OS-only match.
fn full_target_triple() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "windows" => format!("{}-pc-windows-msvc", arch),
        "macos" => format!("{}-apple-darwin", arch),
        // Default Linux ABI is gnu; musl builds carry an explicit -musl in the name.
        "linux" => format!("{}-unknown-linux-gnu", arch),
        other => format!("{}-{}", arch, other),
    }
}

/// Determine the platform target string used in release asset names.
fn platform_target() -> String {
    let os = match std::env::consts::OS {
        "linux" => "unknown-linux",
        "macos" | "darwin" => "apple-darwin",
        "windows" => "pc-windows",
        other => other,
    };
    let arch = std::env::consts::ARCH;
    format!("{}-{}", arch, os)
}

/// Shared HTTP agent with bounded timeouts. ureq 2.x defaults read/write
/// timeouts to INFINITE, so a stalled connection would hang forever without
/// these. The overall `.timeout` is the watchdog covering the whole request.
fn http_agent() -> ureq::Agent {
    use std::time::Duration;
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .timeout(Duration::from_secs(120))
        .build()
}

/// Download an asset from the given URL (with bounded timeouts).
fn download_asset(url: &str) -> Result<Vec<u8>> {
    let response = http_agent()
        .get(url)
        .set(
            "User-Agent",
            &format!("recall/{}", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .context("failed to download asset")?;

    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .context("failed to read asset bytes")?;

    Ok(bytes)
}

/// Extract the recall binary from a release archive, dispatching on the asset's
/// file type. `.zip` (Windows) and `.tar.gz` are supported; `.tar.xz`
/// (Linux/macOS cargo-dist default) is not yet — see ticket 063.
fn extract_binary(archive_bytes: &[u8], asset_name: &str) -> Result<Vec<u8>> {
    let binary_name = if cfg!(windows) {
        "recall.exe"
    } else {
        "recall"
    };

    let lower = asset_name.to_lowercase();
    if lower.ends_with(".zip") {
        crate::archive::extract_named_from_zip(archive_bytes, binary_name)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        extract_binary_tar_gz(archive_bytes, binary_name)
    } else if lower.ends_with(".tar.xz") {
        extract_binary_tar_xz(archive_bytes, binary_name)
    } else {
        anyhow::bail!("unrecognized release archive format: '{}'", asset_name)
    }
}

/// Upper bound on decompressed archive bytes we will read while hunting for the
/// binary. The release binary is ~25 MB; this cap is a safe margin above that
/// so a malicious/corrupt archive can't drive unbounded decompression (xz can
/// expand ~1000:1). We only ever extract a digest-verified official asset, so
/// this is defense-in-depth, not the primary gate.
const MAX_DECOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;

/// Extract `binary_name` from a decompressed tar stream.
///
/// Safety: accepts only `EntryType::Regular` entries (rejects symlink/hardlink/
/// dir entries — the class behind tar-rs CVE-2026-33056) and reads bytes into a
/// buffer we own (never `unpack()` to an archive-controlled path, so path
/// traversal does not apply). The caller wraps the decoder in `Read::take` to
/// bound total decompressed bytes.
fn extract_binary_from_tar<R: Read>(reader: R, binary_name: &str) -> Result<Vec<u8>> {
    let mut archive = tar::Archive::new(reader);

    for entry in archive
        .entries()
        .context("failed to read archive entries")?
    {
        let mut entry = entry.context("failed to read archive entry")?;
        if entry.header().entry_type() != tar::EntryType::Regular {
            continue;
        }
        let path = entry.path().context("failed to read entry path")?;
        if path.file_name().and_then(|n| n.to_str()) == Some(binary_name) {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .context("failed to read binary from archive")?;
            return Ok(buf);
        }
    }

    anyhow::bail!("binary '{}' not found in archive", binary_name)
}

/// Extract `binary_name` from a gzip-compressed tar archive.
fn extract_binary_tar_gz(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    use flate2::read::GzDecoder;

    let decoder = GzDecoder::new(archive_bytes).take(MAX_DECOMPRESSED_BYTES);
    extract_binary_from_tar(decoder, binary_name)
}

/// Extract `binary_name` from an xz-compressed tar archive (Linux/macOS
/// cargo-dist default). Pure-Rust decode via `lzma-rust2` (no C toolchain).
fn extract_binary_tar_xz(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    use lzma_rust2::XzReader;

    // allow_multiple_streams = true: cargo-dist .xz output may concatenate
    // independent streams.
    let decoder = XzReader::new(archive_bytes, true).take(MAX_DECOMPRESSED_BYTES);
    extract_binary_from_tar(decoder, binary_name)
}

/// Replace the current binary with the new one.
fn replace_self(new_binary: &[u8]) -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to determine current exe path")?;

    if cfg!(windows) {
        // Windows can't overwrite a running exe, but CAN rename it aside.
        // Rename current → .old, write new in its place. If the write fails,
        // restore .old so the user is never left without a working binary.
        // The .old file (the still-running process) is cleaned up on next launch.
        let backup = current_exe.with_extension("old");
        let _ = fs::remove_file(&backup); // clear any stale .old first
        fs::rename(&current_exe, &backup).context("failed to rename current binary")?;
        if let Err(e) = fs::write(&current_exe, new_binary) {
            // Rollback: put the original binary back.
            let _ = fs::rename(&backup, &current_exe);
            return Err(e).context("failed to write new binary — rolled back to previous version");
        }
        // Do NOT delete .old here — the running process still holds it locked.
        // cleanup_old_binary() removes it on the next launch.
    } else {
        // Unix: write to temp, set executable, rename (atomic on same filesystem)
        let tmp_path = current_exe.with_extension("tmp");
        fs::write(&tmp_path, new_binary).context("failed to write new binary")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
                .context("failed to set executable permission")?;
        }

        fs::rename(&tmp_path, &current_exe).context("failed to replace binary")?;
    }

    Ok(())
}

/// Remove a leftover `.old` binary from a previous Windows self-update.
/// Called at startup, when the previous process no longer holds the file lock.
/// Best-effort: silently ignores failures (file may still be locked, or absent).
pub fn cleanup_old_binary() {
    if let Ok(current_exe) = std::env::current_exe() {
        let old = current_exe.with_extension("old");
        let _ = fs::remove_file(old);
    }
}

// ─── Persistence ─────────────────────────────────────────────────────────────

fn read_last_check() -> Option<u64> {
    let path = last_check_path();
    let content = fs::read_to_string(path).ok()?;
    content.trim().parse().ok()
}

fn write_last_check() -> Result<()> {
    let path = last_check_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, now_epoch().to_string())?;
    Ok(())
}

fn last_check_path() -> PathBuf {
    recall_dir().join("last_update_check")
}

fn recall_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".recall")
}

fn config_path() -> PathBuf {
    recall_dir().join("config.toml")
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_digest_matches() {
        // sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let d = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(verify_digest(b"hello", Some(d)).is_ok());
    }

    #[test]
    fn verify_digest_case_insensitive() {
        let d = "sha256:2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824";
        assert!(verify_digest(b"hello", Some(d)).is_ok());
    }

    #[test]
    fn verify_digest_mismatch_fails() {
        let d = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        assert!(verify_digest(b"hello", Some(d)).is_err());
    }

    #[test]
    fn verify_digest_absent_hard_fails() {
        // Decision (a): no digest → refuse to install.
        assert!(verify_digest(b"hello", None).is_err());
    }

    #[test]
    fn verify_digest_bad_format_fails() {
        assert!(verify_digest(b"hello", Some("md5:abc")).is_err());
    }

    /// Build an in-memory `.tar.xz` containing `entries` (path, contents) as
    /// regular files. Mirrors archive.rs's `make_zip` fixture helper.
    fn make_tar_xz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use lzma_rust2::{XzOptions, XzWriter};
        use std::io::Write;

        // 1. Build the tar into a buffer.
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            for (path, data) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o755);
                header.set_entry_type(tar::EntryType::Regular);
                header.set_cksum();
                builder.append_data(&mut header, path, *data).unwrap();
            }
            builder.finish().unwrap();
        }

        // 2. XZ-compress the tar.
        let mut xz = XzWriter::new(Vec::new(), XzOptions::default()).unwrap();
        xz.write_all(&tar_buf).unwrap();
        xz.finish().unwrap()
    }

    #[test]
    fn extract_binary_tar_xz_extracts_binary() {
        let archive = make_tar_xz(&[
            ("recall-x86_64-unknown-linux-gnu/recall", b"XZBINARY"),
            ("recall-x86_64-unknown-linux-gnu/README.md", b"ignore me"),
        ]);
        let got = extract_binary(&archive, "recall-x86_64-unknown-linux-gnu.tar.xz").unwrap();
        assert_eq!(got, b"XZBINARY");
    }

    #[test]
    fn extract_binary_tar_xz_rejects_symlink_entry() {
        // A .tar.xz whose only "recall" entry is a symlink must not be extracted
        // as the binary (EntryType::Regular gate). Build a tar with a symlink
        // entry named `recall`, xz-compress it, and confirm extraction fails.
        use lzma_rust2::{XzOptions, XzWriter};
        use std::io::Write;

        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_cksum();
            builder
                .append_link(&mut header, "recall", "/etc/passwd")
                .unwrap();
            builder.finish().unwrap();
        }
        let mut xz = XzWriter::new(Vec::new(), XzOptions::default()).unwrap();
        xz.write_all(&tar_buf).unwrap();
        let archive = xz.finish().unwrap();

        let err = extract_binary(&archive, "recall-x.tar.xz")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("not found"),
            "symlink entry must be rejected, not extracted: {err}"
        );
    }

    #[test]
    fn extract_binary_unknown_format_fails() {
        assert!(extract_binary(b"x", "recall-x.rar").is_err());
    }

    #[test]
    fn test_version_is_newer() {
        assert!(version_is_newer("0.1.0", "0.2.0"));
        assert!(version_is_newer("0.1.0", "0.1.1"));
        assert!(version_is_newer("0.1.0", "1.0.0"));
        assert!(version_is_newer("1.2.3", "1.2.4"));
        assert!(version_is_newer("1.2.3", "1.3.0"));
    }

    #[test]
    fn test_version_is_not_newer() {
        assert!(!version_is_newer("0.2.0", "0.1.0"));
        assert!(!version_is_newer("0.1.0", "0.1.0"));
        assert!(!version_is_newer("1.0.0", "0.9.9"));
        assert!(!version_is_newer("2.0.0", "1.99.99"));
    }

    #[test]
    fn test_version_different_lengths() {
        assert!(version_is_newer("0.1", "0.1.1"));
        assert!(!version_is_newer("0.1.1", "0.1"));
    }

    #[test]
    fn test_parse_update_config_defaults() {
        let config = parse_update_config("");
        assert!(config.check);
        assert_eq!(config.interval_hours, 24);
    }

    #[test]
    fn test_parse_update_config_disabled() {
        let content = "[update]\ncheck = false\ninterval_hours = 12\n";
        let config = parse_update_config(content);
        assert!(!config.check);
        assert_eq!(config.interval_hours, 12);
    }

    #[test]
    fn test_parse_update_config_ignores_other_sections() {
        let content = "[telemetry]\nenabled = true\n\n[update]\ncheck = false\n";
        let config = parse_update_config(content);
        assert!(!config.check);
    }

    #[test]
    fn test_platform_target_format() {
        let target = platform_target();
        assert!(
            target.contains('-'),
            "target should contain arch-os: {}",
            target
        );
    }
}
