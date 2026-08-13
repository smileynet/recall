//! Self-update: `recall update` downloads and replaces the binary.
//!
//! No passive/automatic update check. Network calls only happen when
//! the user explicitly runs `recall update`. This avoids unexpected
//! outbound traffic from a local tool.

use std::fs;
use std::io::Read;

use anyhow::{Context, Result};

const GITHUB_REPO: &str = "smileynet/recall";

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

    let asset_url = find_asset_url(&latest)?;
    let archive_bytes = download_asset(&asset_url)?;
    let binary = extract_binary(&archive_bytes)?;
    replace_self(&binary)?;

    println!("Updated to v{}.", latest);
    Ok(0)
}

// ─── Version check ──────────────────────────────────────────────────────────

/// Fetch the latest version tag from GitHub Releases API.
fn fetch_latest_version() -> Result<Option<String>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = ureq::get(&url)
        .set("Accept", "application/vnd.github.v3+json")
        .set("User-Agent", &format!("recall/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
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

// ─── Download and install ────────────────────────────────────────────────────

/// Find the correct asset URL for this platform.
fn find_asset_url(version: &str) -> Result<String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/v{}",
        GITHUB_REPO, version
    );

    let response = ureq::get(&url)
        .set("Accept", "application/vnd.github.v3+json")
        .set("User-Agent", &format!("recall/{}", env!("CARGO_PKG_VERSION")))
        .call()
        .context("failed to fetch release info")?;

    let body: serde_json::Value = response
        .into_json()
        .context("failed to parse release JSON")?;

    let target = platform_target();
    let assets = body.get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| anyhow::anyhow!("no assets in release"))?;

    for asset in assets {
        if let Some(name) = asset.get("name").and_then(|n| n.as_str()) {
            if name.contains(&target) {
                if let Some(url) = asset.get("browser_download_url").and_then(|u| u.as_str()) {
                    return Ok(url.to_string());
                }
            }
        }
    }

    anyhow::bail!("no asset found for platform '{}'", target)
}

/// Determine the platform target string used in release asset names.
fn platform_target() -> String {
    let os = match std::env::consts::OS {
        "linux" => "unknown-linux",
        "macos" | "darwin" => "apple-darwin",
        "windows" => "pc-windows",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => other,
    };
    format!("{}-{}", arch, os)
}

/// Download an asset from the given URL.
fn download_asset(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .set("User-Agent", &format!("recall/{}", env!("CARGO_PKG_VERSION")))
        .call()
        .context("failed to download asset")?;

    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .context("failed to read asset bytes")?;

    Ok(bytes)
}

/// Extract the recall binary from a tar.gz archive.
fn extract_binary(archive_bytes: &[u8]) -> Result<Vec<u8>> {
    use flate2::read::GzDecoder;

    let decoder = GzDecoder::new(archive_bytes);
    let mut archive = tar::Archive::new(decoder);

    let binary_name = if cfg!(windows) { "recall.exe" } else { "recall" };

    for entry in archive.entries().context("failed to read archive entries")? {
        let mut entry = entry.context("failed to read archive entry")?;
        let path = entry.path().context("failed to read entry path")?;
        if path.file_name().and_then(|n| n.to_str()) == Some(binary_name) {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).context("failed to read binary from archive")?;
            return Ok(buf);
        }
    }

    anyhow::bail!("binary '{}' not found in archive", binary_name)
}

/// Replace the current binary with the new one.
fn replace_self(new_binary: &[u8]) -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to determine current exe path")?;

    if cfg!(windows) {
        // Windows: can't overwrite a running exe. Rename old, write new, delete old on next run.
        let backup = current_exe.with_extension("old");
        fs::rename(&current_exe, &backup)
            .context("failed to rename current binary")?;
        fs::write(&current_exe, new_binary)
            .context("failed to write new binary")?;
        let _ = fs::remove_file(&backup);
    } else {
        // Unix: write to temp, set executable, rename (atomic on same filesystem)
        let tmp_path = current_exe.with_extension("tmp");
        fs::write(&tmp_path, new_binary)
            .context("failed to write new binary")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
                .context("failed to set executable permission")?;
        }

        fs::rename(&tmp_path, &current_exe)
            .context("failed to replace binary")?;
    }

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_platform_target_format() {
        let target = platform_target();
        assert!(target.contains('-'), "target should contain arch-os: {}", target);
    }
}
