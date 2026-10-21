//! Configuration owner for recall (ticket 37).
//!
//! Single owner of the `~/.recall/config.toml` document contract:
//!
//! - **Lossless read/modify/write** (`edit`): a targeted section update
//!   preserves every other known section, every unknown future section/key,
//!   and comments/ordering (via `toml_edit`). This replaces the hand-rolled
//!   parsers in `telemetry`/`update` whose whole-file rewrite silently erased
//!   other sections.
//! - **Typed accessors** for the sections recall understands today
//!   (`telemetry`, `update`, `import`), with env-var > config > default
//!   precedence where applicable.
//! - **Project discovery** (`discover_memory_projects`) driven by
//!   `import.roots` / `import.exclude`, replacing three hardcoded root scans.
//!
//! #087 extends this module with the remaining local-state path policy (DB,
//! logs, model cache, markers) rather than introducing a second parser.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml_edit::{value, Array, DocumentMut, Item, Table};

// ─── Paths ───────────────────────────────────────────────────────────────────

/// `~/.recall` (or `.recall` under CWD if neither HOME nor USERPROFILE is set).
///
/// This mirrors the resolution duplicated in `telemetry`/`update` today. #087
/// will make this the single owner; for now it is centralized here and those
/// modules delegate to it.
pub fn recall_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".recall")
}

/// Path to `config.toml`, honoring `RECALL_CONFIG` for tests/overrides.
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("RECALL_CONFIG") {
        return PathBuf::from(p);
    }
    recall_dir().join("config.toml")
}

// ─── Lossless document editing ─────────────────────────────────────────────────

/// Load the config document, returning an empty document if the file is absent
/// or unreadable. Parse errors surface so a corrupt file is not silently reset.
pub fn load_document() -> Result<DocumentMut> {
    load_document_at(&config_path())
}

fn load_document_at(path: &Path) -> Result<DocumentMut> {
    match std::fs::read_to_string(path) {
        Ok(content) => content
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(_) => Ok(DocumentMut::new()),
    }
}

/// Apply a targeted edit to the config document and write it back losslessly.
///
/// The closure mutates only the keys it cares about; everything else in the
/// document — other known sections, unknown future sections, comments, and key
/// ordering — is preserved. The file (and `~/.recall`) is created if absent.
pub fn edit<F>(f: F) -> Result<()>
where
    F: FnOnce(&mut DocumentMut),
{
    let path = config_path();
    edit_at(&path, f)
}

fn edit_at<F>(path: &Path, f: F) -> Result<()>
where
    F: FnOnce(&mut DocumentMut),
{
    let mut doc = load_document_at(path)?;
    f(&mut doc);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, doc.to_string())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Ensure `doc[section]` exists as a table and return it mutably.
fn section_mut<'a>(doc: &'a mut DocumentMut, section: &str) -> &'a mut Table {
    let item = doc
        .entry(section)
        .or_insert_with(|| Item::Table(Table::new()));
    if !item.is_table() {
        *item = Item::Table(Table::new());
    }
    item.as_table_mut().expect("just ensured table")
}

// ─── Typed read helpers ────────────────────────────────────────────────────────

fn read_bool(doc: &DocumentMut, section: &str, key: &str) -> Option<bool> {
    doc.get(section)?.get(key)?.as_bool()
}

fn read_u64(doc: &DocumentMut, section: &str, key: &str) -> Option<u64> {
    doc.get(section)?.get(key)?.as_integer().and_then(
        |i| {
            if i >= 0 {
                Some(i as u64)
            } else {
                None
            }
        },
    )
}

fn read_string_array(doc: &DocumentMut, section: &str, key: &str) -> Option<Vec<String>> {
    let arr = doc.get(section)?.get(key)?.as_array()?;
    Some(
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
    )
}

// ─── Telemetry section ─────────────────────────────────────────────────────────

/// `[telemetry]` values. Defaults: disabled.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TelemetrySection {
    pub enabled: bool,
    pub crash_reporting: bool,
}

pub fn telemetry() -> TelemetrySection {
    let doc = load_document().unwrap_or_default();
    TelemetrySection {
        enabled: read_bool(&doc, "telemetry", "enabled").unwrap_or(false),
        crash_reporting: read_bool(&doc, "telemetry", "crash_reporting").unwrap_or(false),
    }
}

/// Write `[telemetry]` losslessly, preserving all other sections.
pub fn set_telemetry(cfg: TelemetrySection) -> Result<()> {
    edit(|doc| {
        let t = section_mut(doc, "telemetry");
        t["enabled"] = value(cfg.enabled);
        t["crash_reporting"] = value(cfg.crash_reporting);
    })
}

// ─── Update section ────────────────────────────────────────────────────────────

/// `[update]` values. Defaults: check enabled, 24h interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateSection {
    pub check: bool,
    pub interval_hours: u64,
}

impl Default for UpdateSection {
    fn default() -> Self {
        Self {
            check: true,
            interval_hours: 24,
        }
    }
}

pub fn update() -> UpdateSection {
    let doc = load_document().unwrap_or_default();
    let d = UpdateSection::default();
    UpdateSection {
        check: read_bool(&doc, "update", "check").unwrap_or(d.check),
        interval_hours: read_u64(&doc, "update", "interval_hours").unwrap_or(d.interval_hours),
    }
}

// ─── Import section ────────────────────────────────────────────────────────────

/// `[import]` values driving project discovery.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportSection {
    /// Roots to scan for project directories containing `.memory/`. When empty
    /// in config, discovery falls back to the built-in defaults
    /// (`$HOME/code`, `D:/code`).
    pub roots: Vec<String>,
    /// Directory-name patterns to skip. A trailing `*` is a prefix glob
    /// (`archived_*`); otherwise an exact directory-name match
    /// (e.g. `node_modules`).
    pub exclude: Vec<String>,
}

pub fn import() -> ImportSection {
    let doc = load_document().unwrap_or_default();
    ImportSection {
        roots: read_string_array(&doc, "import", "roots").unwrap_or_default(),
        exclude: read_string_array(&doc, "import", "exclude").unwrap_or_default(),
    }
}

/// Write `[import]` losslessly, preserving all other sections.
pub fn set_import(cfg: &ImportSection) -> Result<()> {
    edit(|doc| {
        let t = section_mut(doc, "import");
        let mut roots = Array::new();
        for r in &cfg.roots {
            roots.push(r.as_str());
        }
        let mut exclude = Array::new();
        for e in &cfg.exclude {
            exclude.push(e.as_str());
        }
        t["roots"] = value(roots);
        t["exclude"] = value(exclude);
    })
}

// ─── Project discovery ─────────────────────────────────────────────────────────

/// Built-in default roots used when `import.roots` is unset. Preserves the
/// pre-config behavior exactly: `$HOME/code` (or `%USERPROFILE%\code`) and
/// `D:/code` (Windows multi-drive).
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        roots.push(PathBuf::from(home).join("code"));
    }
    roots.push(PathBuf::from("D:/code"));
    roots
}

/// Resolve the effective list of existing root directories to scan.
///
/// Precedence: configured `import.roots` (if non-empty) > built-in defaults.
/// Only directories that actually exist are returned.
pub fn resolve_roots(cfg: &ImportSection) -> Vec<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg.roots.is_empty() {
        default_roots()
    } else {
        cfg.roots.iter().map(PathBuf::from).collect()
    };
    candidates.into_iter().filter(|p| p.is_dir()).collect()
}

/// True if `dir_name` matches any exclude pattern. A pattern ending in `*` is a
/// prefix match; otherwise an exact match.
fn is_excluded(dir_name: &str, exclude: &[String]) -> bool {
    exclude.iter().any(|pat| match pat.strip_suffix('*') {
        Some(prefix) => dir_name.starts_with(prefix),
        None => dir_name == pat,
    })
}

/// A project discovered under a scanned root: its `.memory/` directory and the
/// wing name derived from the project directory name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredProject {
    /// Path to the project's `.memory/` directory.
    pub memory_path: PathBuf,
    /// Normalized wing name (project directory name, normalized).
    pub wing: String,
}

/// Discover all projects with a `.memory/` directory under the given roots,
/// applying the exclude patterns. Wing names are normalized via the supplied
/// `normalize` function (so this module does not depend on `store`).
///
/// This is the single implementation that replaces the three previously
/// duplicated, hardcoded root scans (`import-all`, `sync` phase 2, `health`).
pub fn discover_memory_projects(
    roots: &[PathBuf],
    exclude: &[String],
    normalize: impl Fn(&str) -> String,
) -> Vec<DiscoveredProject> {
    let mut found = Vec::new();
    for root in roots {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || !path.join(".memory").is_dir() {
                continue;
            }
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if is_excluded(dir_name, exclude) {
                continue;
            }
            let wing = normalize(dir_name);
            if wing.is_empty() {
                continue;
            }
            found.push(DiscoveredProject {
                memory_path: path.join(".memory"),
                wing,
            });
        }
    }
    found
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp_config(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    // ── lossless editing ──────────────────────────────────────────────────

    #[test]
    fn edit_telemetry_preserves_update_and_unknown_sections() {
        // A document with [update], [import], and an unknown future section.
        let (dir, path) = write_tmp_config(
            "[update]\ncheck = false\ninterval_hours = 12\n\n\
             [import]\nroots = [\"/a\", \"/b\"]\n\n\
             [future]\nsome_new_key = \"keep me\"\n",
        );
        let _ = dir;

        // Edit only [telemetry].
        edit_at(&path, |doc| {
            let t = section_mut(doc, "telemetry");
            t["enabled"] = value(true);
            t["crash_reporting"] = value(true);
        })
        .unwrap();

        let doc = load_document_at(&path).unwrap();

        // Telemetry was written.
        assert_eq!(read_bool(&doc, "telemetry", "enabled"), Some(true));
        assert_eq!(read_bool(&doc, "telemetry", "crash_reporting"), Some(true));

        // [update] survived with its values.
        assert_eq!(read_bool(&doc, "update", "check"), Some(false));
        assert_eq!(read_u64(&doc, "update", "interval_hours"), Some(12));

        // [import] survived.
        assert_eq!(
            read_string_array(&doc, "import", "roots"),
            Some(vec!["/a".to_string(), "/b".to_string()])
        );

        // Unknown future section AND key survived.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("[future]"), "unknown section erased: {raw}");
        assert!(raw.contains("keep me"), "unknown key erased: {raw}");
    }

    #[test]
    fn edit_update_preserves_telemetry() {
        let (dir, path) = write_tmp_config("[telemetry]\nenabled = true\ncrash_reporting = true\n");
        let _ = dir;
        edit_at(&path, |doc| {
            let t = section_mut(doc, "update");
            t["check"] = value(false);
        })
        .unwrap();
        let doc = load_document_at(&path).unwrap();
        assert_eq!(read_bool(&doc, "telemetry", "enabled"), Some(true));
        assert_eq!(read_bool(&doc, "update", "check"), Some(false));
    }

    #[test]
    fn edit_preserves_comments() {
        let (dir, path) =
            write_tmp_config("# recall config\n[telemetry]\nenabled = false # user set this\n");
        let _ = dir;
        edit_at(&path, |doc| {
            section_mut(doc, "update")["check"] = value(false);
        })
        .unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("# recall config"),
            "header comment lost: {raw}"
        );
        assert!(
            raw.contains("# user set this"),
            "inline comment lost: {raw}"
        );
    }

    #[test]
    fn edit_creates_file_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        edit_at(&path, |doc| {
            section_mut(doc, "telemetry")["enabled"] = value(true);
        })
        .unwrap();
        assert!(path.exists());
        let doc = load_document_at(&path).unwrap();
        assert_eq!(read_bool(&doc, "telemetry", "enabled"), Some(true));
    }

    // ── typed reads ───────────────────────────────────────────────────────

    #[test]
    fn import_reads_roots_and_exclude() {
        let (dir, path) = write_tmp_config(
            "[import]\nroots = [\"/x\", \"/y\"]\nexclude = [\"node_modules\", \"archived_*\"]\n",
        );
        let _ = dir;
        let doc = load_document_at(&path).unwrap();
        assert_eq!(
            read_string_array(&doc, "import", "roots"),
            Some(vec!["/x".to_string(), "/y".to_string()])
        );
        assert_eq!(
            read_string_array(&doc, "import", "exclude"),
            Some(vec!["node_modules".to_string(), "archived_*".to_string()])
        );
    }

    #[test]
    fn update_defaults_when_absent() {
        let doc = DocumentMut::new();
        let d = UpdateSection::default();
        assert!(d.check);
        assert_eq!(d.interval_hours, 24);
        // Reading missing keys falls back to defaults.
        assert_eq!(read_bool(&doc, "update", "check"), None);
    }

    #[test]
    fn read_u64_rejects_negative() {
        let doc = "[update]\ninterval_hours = -5\n"
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(read_u64(&doc, "update", "interval_hours"), None);
    }

    // ── exclude matching ──────────────────────────────────────────────────

    #[test]
    fn exclude_exact_and_prefix() {
        let ex = vec!["node_modules".to_string(), "archived_*".to_string()];
        assert!(is_excluded("node_modules", &ex));
        assert!(is_excluded("archived_2024", &ex));
        assert!(is_excluded("archived_", &ex));
        assert!(!is_excluded("node_modules_x", &ex)); // exact only
        assert!(!is_excluded("src", &ex));
    }

    #[test]
    fn exclude_empty_matches_nothing() {
        assert!(!is_excluded("anything", &[]));
    }

    // ── discovery ─────────────────────────────────────────────────────────

    fn make_project(root: &Path, name: &str, with_memory: bool) {
        let p = root.join(name);
        std::fs::create_dir_all(&p).unwrap();
        if with_memory {
            std::fs::create_dir_all(p.join(".memory")).unwrap();
        }
    }

    #[test]
    fn discover_finds_memory_projects() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        make_project(root, "alpha", true);
        make_project(root, "beta", true);
        make_project(root, "gamma", false); // no .memory → skipped

        let mut got = discover_memory_projects(&[root.to_path_buf()], &[], |n| n.to_string());
        got.sort_by(|a, b| a.wing.cmp(&b.wing));
        let wings: Vec<_> = got.iter().map(|p| p.wing.clone()).collect();
        assert_eq!(wings, vec!["alpha".to_string(), "beta".to_string()]);
        assert!(got[0].memory_path.ends_with("alpha/.memory"));
    }

    #[test]
    fn discover_applies_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        make_project(root, "keep", true);
        make_project(root, "node_modules", true);
        make_project(root, "archived_old", true);

        let got = discover_memory_projects(
            &[root.to_path_buf()],
            &["node_modules".to_string(), "archived_*".to_string()],
            |n| n.to_string(),
        );
        let wings: Vec<_> = got.iter().map(|p| p.wing.clone()).collect();
        assert_eq!(wings, vec!["keep".to_string()]);
    }

    #[test]
    fn discover_skips_missing_root() {
        let got = discover_memory_projects(&[PathBuf::from("/nonexistent/root/xyz")], &[], |n| {
            n.to_string()
        });
        assert!(got.is_empty());
    }

    #[test]
    fn discover_uses_normalize() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        make_project(root, "My-Project", true);
        let got = discover_memory_projects(&[root.to_path_buf()], &[], |n| {
            n.to_lowercase().replace('-', "_")
        });
        assert_eq!(got[0].wing, "my_project");
    }

    #[test]
    fn resolve_roots_prefers_configured() {
        let dir = tempfile::tempdir().unwrap();
        let configured = dir.path().join("custom");
        std::fs::create_dir_all(&configured).unwrap();
        let cfg = ImportSection {
            roots: vec![configured.to_string_lossy().to_string()],
            exclude: vec![],
        };
        let resolved = resolve_roots(&cfg);
        assert_eq!(resolved, vec![configured]);
    }

    #[test]
    fn resolve_roots_filters_nonexistent() {
        let cfg = ImportSection {
            roots: vec!["/definitely/not/here".to_string()],
            exclude: vec![],
        };
        assert!(resolve_roots(&cfg).is_empty());
    }

    #[test]
    fn set_import_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = ImportSection {
            roots: vec!["/one".to_string(), "/two".to_string()],
            exclude: vec!["node_modules".to_string()],
        };
        edit_at(&path, |doc| {
            let t = section_mut(doc, "import");
            let mut roots = Array::new();
            for r in &cfg.roots {
                roots.push(r.as_str());
            }
            let mut exclude = Array::new();
            for e in &cfg.exclude {
                exclude.push(e.as_str());
            }
            t["roots"] = value(roots);
            t["exclude"] = value(exclude);
        })
        .unwrap();
        let doc = load_document_at(&path).unwrap();
        assert_eq!(
            read_string_array(&doc, "import", "roots"),
            Some(vec!["/one".to_string(), "/two".to_string()])
        );
    }
}
