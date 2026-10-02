use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

use std::path::PathBuf;
use std::sync::OnceLock;

// ─── ONNX Runtime management (load-dynamic) ─────────────────────────────────

/// The ONNX Runtime version required by ort 2.0.0-rc.13 (its default download
/// line; matches the api-24 feature floor). Re-vendor the per-platform SHA-256
/// in `ort_platform()` whenever this changes.
const ORT_VERSION: &str = "1.28.2";

/// Platform-specific ONNX Runtime release: the archive name slug, its extension,
/// and the SHA-256 of the published archive. Single `#[cfg]` cascade — the
/// download URL, extraction, and the pinned hash all key off this, so a version
/// bump touches only `ORT_VERSION` and this table.
///
/// Hashes are vendored per `ORT_VERSION` from Microsoft's GitHub release
/// (`github.com/microsoft/onnxruntime/releases`). To re-vendor on a version
/// bump: download `onnxruntime-{slug}-{ORT_VERSION}.{ext}` for each platform and
/// record `sha256sum`. Values below are for ONNX Runtime v1.28.2, obtained
/// 2026-10-02 directly from the release assets.
///
/// Note: Microsoft stopped shipping an `osx-x86_64` (Intel macOS) asset at ONNX
/// Runtime >= 1.25 — that target is unsupported on the current ORT line.
fn ort_platform() -> (&'static str, &'static str, &'static str) {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        (
            "win-x64",
            "zip",
            "c4eedd29489d5feca21866d054638416f3655bf6b18851b3b6b85c8313e95c35",
        )
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        (
            "linux-x64",
            "tgz",
            "d7209b8751b27b862b0c76332c2e20e203396edb5dab700ecf4bb485cf147415",
        )
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        // ONNX Runtime no longer ships an Intel-macOS binary (>= 1.25). Fail at
        // compile time with a clear message rather than 404 at runtime.
        compile_error!(
            "recall does not support Intel macOS (x86_64): ONNX Runtime stopped \
             publishing osx-x86_64 binaries at v1.25+. Use an arm64 (Apple \
             Silicon) macOS build."
        );
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        (
            "osx-arm64",
            "tgz",
            "c4fceacfc53765d0869dc9180c31ec91054d149017a99d1e80ffe28dc79596de",
        )
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        (
            "linux-aarch64",
            "tgz",
            "f020b3d31106cc7db03889b4a5c21e7c38ce4a09ad26119c11d1ad6d3fa0ec04",
        )
    }
}

/// Platform-specific download URL for ONNX Runtime, derived from `ORT_VERSION`.
fn ort_download_url() -> String {
    let (slug, ext, _sha) = ort_platform();
    format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{v}/onnxruntime-{slug}-{v}.{ext}",
        v = ORT_VERSION,
    )
}

/// The pinned SHA-256 of this platform's ONNX Runtime archive.
fn ort_archive_sha256() -> &'static str {
    ort_platform().2
}

/// Platform-specific library filename.
fn ort_lib_filename() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "onnxruntime.dll"
    }
    #[cfg(target_os = "linux")]
    {
        "libonnxruntime.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libonnxruntime.dylib"
    }
}

/// Resolve the recall home directory: `USERPROFILE` (Windows) or `HOME` (Unix).
/// Fails loudly rather than falling back to a volatile location — recall's corpus
/// must be durable, so an unresolvable home is an error with remediation, not a
/// silent write to CWD/temp. Escape hatches: `RECALL_DB` (database) and
/// `FASTEMBED_CACHE_DIR` (model cache) bypass home entirely.
fn recall_home() -> Result<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot determine home directory: neither USERPROFILE nor HOME is set. \
                 Set one, or set RECALL_DB (database) / FASTEMBED_CACHE_DIR (model cache) \
                 to explicit paths."
            )
        })
}

/// Directory where we cache the ONNX Runtime library.
fn ort_lib_dir() -> Result<PathBuf> {
    Ok(recall_home()?.join(".recall").join("lib"))
}

/// Full path to the cached ONNX Runtime library.
fn ort_lib_path() -> Result<PathBuf> {
    ort_lib_dir().map(|d| d.join(ort_lib_filename()))
}

/// Ensure ONNX Runtime is available and initialize ort to use it.
/// Downloads on first run if not cached. Must be called before any ort API usage.
/// Runs once per process; the result (including any error) is cached and replayed.
static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn ensure_ort_runtime() -> Result<()> {
    ORT_INIT
        .get_or_init(|| ensure_ort_runtime_inner().map_err(|e| format!("{:#}", e)))
        .clone()
        .map_err(|e| anyhow::anyhow!("ONNX Runtime initialization failed: {}", e))
}

fn ensure_ort_runtime_inner() -> Result<()> {
    let lib_path = ort_lib_path()?;

    // Detect a corrupt/truncated cache from a prior interrupted download.
    // The ORT runtime library is several MB; anything under 1MB is broken.
    if lib_path.exists() {
        if let Ok(meta) = std::fs::metadata(&lib_path) {
            if meta.len() < 1_000_000 {
                eprintln!(
                    "recall: cached ONNX Runtime looks corrupt ({} bytes), re-downloading",
                    meta.len()
                );
                let _ = std::fs::remove_file(&lib_path);
            }
        }
    }

    // Download if not cached
    if !lib_path.exists() {
        download_ort_runtime(&lib_path)?;
    }

    // Pre-flight: validate the dylib ourselves BEFORE handing it to ort. ort's
    // load-dynamic path .expect()s on OrtGetApiBase / GetVersionString and
    // panics (not Err) on a missing/incompatible DLL — a panic recall cannot
    // catch via `?`. The pre-flight turns those into a graceful domain error.
    preflight_ort_dylib(&lib_path)?;

    // Tell ort where to find the library (overrides System32 or PATH search).
    // rc.13: init_from returns Result<EnvironmentBuilder, _>; commit() returns
    // bool (false if a global env was already committed — not an error).
    let _ = ort::init_from(lib_path.to_string_lossy().as_ref())?.commit();
    Ok(())
}

/// Expected ONNX Runtime major.minor that ort 2.0.0-rc.13 requires (`ORT_VERSION`
/// minus patch). The loaded dylib's minor must match this.
fn expected_ort_minor() -> &'static str {
    // ORT_VERSION is "1.28.2" -> "1.28". Kept derived so a version bump needs
    // only ORT_VERSION changed.
    ORT_VERSION
        .rsplit_once('.')
        .map(|(mm, _)| mm)
        .unwrap_or(ORT_VERSION)
}

/// The ONNX Runtime version recall expects (compile-time constant, for display).
pub fn expected_ort_version() -> &'static str {
    ORT_VERSION
}

/// Read the version string of the cached ONNX Runtime dylib, if present and
/// readable, WITHOUT initializing `ort` (which would panic on a bad lib). Used
/// by `recall health` for diagnosability. `None` means not cached or unreadable.
pub fn ort_runtime_version() -> Option<String> {
    let path = ort_lib_path().ok()?;
    if !path.is_file() {
        return None;
    }
    read_ort_dylib_version(&path).ok()
}

/// dlopen the dylib and read `OrtGetApiBase()->GetVersionString()` via
/// `libloading`, returning the version string or a domain error. Shared by the
/// pre-flight and the health reporter so both read the version the same way,
/// never triggering `ort`'s internal panic path.
fn read_ort_dylib_version(lib_path: &std::path::Path) -> Result<String> {
    use std::ffi::{c_char, CStr};
    // SAFETY: calling the stable ONNX Runtime C ABI entry points.
    unsafe {
        let lib = libloading::Library::new(lib_path)
            .map_err(|e| anyhow::anyhow!("cannot load dylib: {e}"))?;
        type GetApiBaseFn = unsafe extern "C" fn() -> *const ort_sys::OrtApiBase;
        let get_api_base: libloading::Symbol<GetApiBaseFn> = lib
            .get(b"OrtGetApiBase")
            .map_err(|e| anyhow::anyhow!("missing OrtGetApiBase: {e}"))?;
        let base = get_api_base();
        if base.is_null() {
            anyhow::bail!("OrtGetApiBase returned null");
        }
        // ort-sys rc.13: OrtApiBase.GetVersionString is a bare `fn` (the rc.10+
        // ABI change dropped the Option<fn> wrapper). Call it directly.
        let get_version = (*base).GetVersionString;
        let vptr = get_version();
        if vptr.is_null() {
            anyhow::bail!("GetVersionString returned null");
        }
        Ok(CStr::from_ptr(vptr as *const c_char)
            .to_string_lossy()
            .into_owned())
    }
}

/// Validate an ONNX Runtime shared library before `ort` loads it.
///
/// `ort`'s load-dynamic path looks up `OrtGetApiBase` and calls
/// `GetVersionString()` with `.expect()`, so a missing symbol or wrong-version
/// DLL panics inside the crate. We reproduce those two lookups via `libloading`
/// and return a domain [`anyhow::Error`] with remediation instead, so a bad
/// cache fails gracefully. On success, `ort`'s subsequent load takes the same
/// good path.
fn preflight_ort_dylib(lib_path: &std::path::Path) -> Result<()> {
    let remediation = format!(
        "the cached ONNX Runtime at {} is unusable. Re-run the command (recall \
         re-downloads automatically), or delete {} to force a clean re-download.",
        lib_path.display(),
        lib_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.recall/lib".to_string()),
    );

    let version = read_ort_dylib_version(lib_path)
        .map_err(|e| anyhow::anyhow!("ONNX Runtime pre-flight failed: {e}. {remediation}"))?;

    // ort panics when the loaded ONNX RT API is lower than the build's api-NN;
    // require an exact minor match for a clear, early domain error regardless.
    let expected = expected_ort_minor();
    let loaded_minor = version
        .rsplit_once('.')
        .map(|(mm, _)| mm)
        .unwrap_or(&version);
    if loaded_minor != expected {
        anyhow::bail!(
            "ONNX Runtime version mismatch: ort 2.0.0-rc.13 expects {expected}.x \
             (ORT_VERSION {ORT_VERSION}), but the loaded dylib reports {version}. {remediation}"
        );
    }
    Ok(())
}

fn download_ort_runtime(target_path: &PathBuf) -> Result<()> {
    let url = ort_download_url();
    eprintln!(
        "recall: Downloading ONNX Runtime v{} (first run only)...",
        ORT_VERSION
    );

    let response = ureq::get(&url)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to download ONNX Runtime: {}", e))?;

    let len = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());

    let mut body = Vec::new();
    let mut reader = response.into_reader();
    if let Some(total) = len {
        let mut downloaded: u64 = 0;
        let mut buf = [0u8; 65536];
        loop {
            let n = std::io::Read::read(&mut reader, &mut buf)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
            downloaded += n as u64;
            eprint!(
                "\r  {:.1}MB / {:.1}MB",
                downloaded as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0
            );
        }
        eprintln!();
    } else {
        std::io::Read::read_to_end(&mut reader, &mut body)?;
    }

    // Verify the downloaded archive against the pinned per-platform SHA-256
    // BEFORE extracting. A truncated or tampered archive aborts here rather than
    // poisoning every later command. Replaces the old `>1MB` size heuristic.
    verify_archive_sha256(&body, ort_archive_sha256())?;

    // Extract the library from the archive to a temp file in the same dir, then
    // atomically rename. A crash mid-extract leaves the temp file, not a
    // truncated final file that would poison every later command.
    let lib_dir = target_path.parent().unwrap();
    std::fs::create_dir_all(lib_dir)?;
    let lib_name = ort_lib_filename();

    let tmp = tempfile::NamedTempFile::new_in(lib_dir)?;
    let tmp_path = tmp.path().to_path_buf();

    if url.ends_with(".zip") {
        extract_lib_from_zip(&body, lib_name, &tmp_path)?;
    } else {
        extract_lib_from_tgz(&body, lib_name, &tmp_path)?;
    }

    // Defense-in-depth: the SHA-256 above guarantees archive integrity, but the
    // extractor still selects an entry by name — a sanity check that we pulled a
    // real multi-MB runtime (not a tiny sidecar/symlink) catches any matcher
    // mis-selection before it poisons the cache. ORT runtimes are >1MB on every
    // platform; the smallest sidecar we must not accept is ~14KB.
    let extracted_len = std::fs::metadata(&tmp_path)?.len();
    anyhow::ensure!(
        extracted_len >= 1_000_000,
        "extracted ONNX Runtime library is implausibly small ({} bytes) — the \
         archive verified but the wrong entry was selected from it (likely a \
         sidecar or symlink). This is a bug in ort_lib_entry_matches.",
        extracted_len
    );

    tmp.persist(target_path)
        .map_err(|e| anyhow::anyhow!("failed to persist ONNX Runtime: {}", e))?;

    eprintln!("  Cached at: {}", target_path.display());
    Ok(())
}

/// Verify downloaded archive bytes against a pinned lowercase-hex SHA-256.
/// Mismatch aborts with both hashes named. The pinned value is vendored per
/// `ORT_VERSION` in `ort_platform()`.
fn verify_archive_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    use sha2::{Digest, Sha256};
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        anyhow::bail!(
            "ONNX Runtime archive checksum mismatch: expected {}, got {} \
             — refusing to install (re-run to retry the download; if this \
             persists, the pinned hash in ort_platform() may be stale for \
             ORT v{})",
            expected,
            actual,
            ORT_VERSION
        );
    }
    Ok(())
}

fn extract_lib_from_tgz(data: &[u8], lib_name: &str, target_path: &PathBuf) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_size = entry.header().size().unwrap_or(0);
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_string_lossy();
        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        if ort_lib_entry_matches(&path_str, filename, lib_name, entry_size) {
            let mut file = std::fs::File::create(target_path)?;
            std::io::copy(&mut entry, &mut file)?;
            return Ok(());
        }
    }
    anyhow::bail!("Could not find {} in the downloaded archive", lib_name);
}

/// Decide whether a tar entry is the real ONNX Runtime shared library.
///
/// The naming scheme differs per platform, so a single prefix test is wrong:
/// - Linux: `libonnxruntime.so` (symlink, size 0) and `libonnxruntime.so.1.20.0`
///   (real, has data) — version is a SUFFIX, so `starts_with(lib_name)` matches
///   the real file.
/// - macOS: `libonnxruntime.dylib` (symlink, size 0) and
///   `libonnxruntime.1.20.0.dylib` (real, has data) — version is INFIXED before
///   the extension, so `starts_with(lib_name)` does NOT match the real file.
///   The archive also ships `.../*.dSYM/.../libonnxruntime.1.20.0.dylib` (debug
///   symbols) which must be rejected.
///
/// Rule: the entry must carry data (`entry_size > 0`, which skips the zero-size
/// symlinks), must not live under a `.dSYM` bundle, and its filename must either
/// equal `lib_name` or start with `libonnxruntime` and end with the same
/// extension as `lib_name`. This covers Linux suffix-versioning and macOS
/// infix-versioning with one predicate.
fn ort_lib_entry_matches(path: &str, filename: &str, lib_name: &str, entry_size: u64) -> bool {
    if entry_size == 0 || path.contains(".dSYM") {
        return false;
    }
    // Reject sidecar libraries shipped alongside the real runtime — notably
    // `libonnxruntime_providers_shared.so` (Linux), which starts with
    // `libonnxruntime` and ends with `.so`, so the infix fallback below would
    // otherwise match it (it appears before the real lib in tar order). The
    // real library's version marker is set off by `.` (`.so.1.20.0`) or by the
    // bare name; the providers libs use `_` after `libonnxruntime`.
    if filename.starts_with("libonnxruntime_") {
        return false;
    }
    if filename == lib_name || filename.starts_with(lib_name) {
        return true;
    }
    // Infix-versioned form (macOS): libonnxruntime.<version>.dylib
    let ext = std::path::Path::new(lib_name)
        .extension()
        .and_then(|e| e.to_str());
    match ext {
        Some(ext) => {
            filename.starts_with("libonnxruntime") && filename.ends_with(&format!(".{ext}"))
        }
        None => false,
    }
}

fn extract_lib_from_zip(data: &[u8], lib_name: &str, target_path: &PathBuf) -> Result<()> {
    let bytes = crate::archive::extract_named_from_zip(data, lib_name)?;
    std::fs::write(target_path, bytes)
        .with_context(|| format!("writing {}", target_path.display()))?;
    Ok(())
}

/// Supported embedding models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// BGE-base-en-v1.5 (768-dim) — default, matches Python recall
    BgeBase,
    /// BGE-small-en-v1.5 (384-dim) — faster, half the storage
    BgeSmall,
}

impl Model {
    pub fn dimensions(self) -> usize {
        match self {
            Model::BgeBase => 768,
            Model::BgeSmall => 384,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Model::BgeBase => "bge-base-en-v1.5",
            Model::BgeSmall => "bge-small-en-v1.5",
        }
    }

    fn fastembed_model(self) -> EmbeddingModel {
        match self {
            Model::BgeBase => EmbeddingModel::BGEBaseENV15,
            Model::BgeSmall => EmbeddingModel::BGESmallENV15,
        }
    }

    /// Parse a model name string into a Model enum.
    pub fn from_name(name: &str) -> Option<Model> {
        match name.to_lowercase().as_str() {
            "bge-base-en-v1.5" | "bge-base" | "base" => Some(Model::BgeBase),
            "bge-small-en-v1.5" | "bge-small" | "small" => Some(Model::BgeSmall),
            _ => None,
        }
    }
}

/// Default model — bge-base to match the Python recall corpus.
pub const DEFAULT_MODEL: Model = Model::BgeBase;

/// Read model selection from RECALL_MODEL env var, falling back to default.
pub fn configured_model() -> Model {
    match std::env::var("RECALL_MODEL") {
        Ok(val) => match Model::from_name(&val) {
            Some(m) => m,
            None => {
                eprintln!(
                    "recall: unknown RECALL_MODEL='{}', valid options: bge-base, bge-small",
                    val
                );
                eprintln!("recall: falling back to default ({})", DEFAULT_MODEL.name());
                DEFAULT_MODEL
            }
        },
        Err(_) => DEFAULT_MODEL,
    }
}

/// Check if the configured model matches what's stored in the database.
/// Prints a warning to stderr if there's a mismatch.
pub fn check_model_mismatch(conn: &rusqlite::Connection) -> Model {
    let model = configured_model();

    if let Ok(Some(stored)) = crate::store::get_meta(conn, "embedding_model") {
        if let Some(stored_model) = Model::from_name(&stored) {
            if stored_model != model {
                eprintln!("recall: ⚠ MODEL MISMATCH");
                eprintln!(
                    "recall:   Database was built with: {} ({}-dim)",
                    stored_model.name(),
                    stored_model.dimensions()
                );
                eprintln!(
                    "recall:   Current config requests: {} ({}-dim)",
                    model.name(),
                    model.dimensions()
                );
                eprintln!(
                    "recall:   Search results will be degraded — embeddings are incompatible."
                );
                eprintln!(
                    "recall:   To fix: re-ingest all data with the new model, or switch back:"
                );
                eprintln!("recall:     RECALL_MODEL={}", stored_model.name());
                eprintln!();
            }
        }
    }

    model
}

/// Stable model cache directory: ~/.recall/models/
/// Respects FASTEMBED_CACHE_DIR env var as override.
fn model_cache_dir() -> Result<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("FASTEMBED_CACHE_DIR") {
        return Ok(std::path::PathBuf::from(dir));
    }
    Ok(recall_home()?.join(".recall").join("models"))
}

/// Embedding model wrapper — loads once, reuses for batch operations.
pub struct Embedder {
    // fastembed 7's `TextEmbedding::embed` takes `&mut self`. recall's embedder
    // is shared by `&Embedder` across ingest/search/sync (and the test harness
    // holds it in a `static OnceLock<Embedder>` across threads), so wrap the
    // model in a Mutex to keep the public `&self` API AND stay `Sync`. Embedding
    // is single-threaded per process, so contention is nil; the lock cost is
    // negligible next to inference.
    model: std::sync::Mutex<TextEmbedding>,
    which: Model,
}

impl Embedder {
    /// Load the configured model (from RECALL_MODEL env var or default).
    pub fn new() -> Result<Self> {
        ensure_ort_runtime()?;
        Self::with_model(configured_model())
    }

    /// Load a specific model.
    pub fn with_model(which: Model) -> Result<Self> {
        ensure_ort_runtime()?;
        let cache_dir = model_cache_dir()?;
        // Override HF_HOME so hf-hub downloads to our controlled cache dir,
        // not a stale/nonexistent path from the user's environment.
        std::env::set_var("HF_HOME", &cache_dir);
        let model = TextEmbedding::try_new(
            TextInitOptions::new(which.fastembed_model())
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true),
        )?;
        Ok(Embedder {
            model: std::sync::Mutex::new(model),
            which,
        })
    }

    /// Which model is loaded.
    pub fn model(&self) -> Model {
        self.which
    }

    /// Embedding dimensions for the loaded model.
    pub fn dimensions(&self) -> usize {
        self.which.dimensions()
    }

    /// Embed a single text.
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.model.lock().unwrap().embed([text], None)?;
        Ok(results.into_iter().next().unwrap())
    }

    /// Embed a batch of texts.
    ///
    /// Processes in bounded sub-batches so peak memory stays flat regardless of
    /// input size. A single large session file can produce tens of thousands of
    /// chunks; passing them all to `model.embed` at once made fastembed fan the
    /// work across every core via `par_chunks` + `from_par_iter`, allocating ONNX
    /// tensors for the whole set simultaneously and OOM-killing ingest on large
    /// files (observed: a 44MB session → ~55k chunks → >20GB RSS). Sub-batching
    /// caps the working set; results are identical (BGE-base is not dynamically
    /// quantized, so a fixed batch size is safe).
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        const SUB_BATCH: usize = 256;
        let mut out = Vec::with_capacity(texts.len());
        for window in texts.chunks(SUB_BATCH) {
            let batch = self.model.lock().unwrap().embed(window, Some(SUB_BATCH))?;
            out.extend(batch);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_non_dylib_file() {
        // A >1MB file that is not a valid shared library: must return a domain
        // Err (not panic), and the message must carry remediation.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), vec![0u8; 2_000_000]).unwrap();
        let err = preflight_ort_dylib(tmp.path()).unwrap_err().to_string();
        assert!(
            err.contains("ONNX Runtime") && err.to_lowercase().contains("re-run"),
            "error must name ORT and give remediation: {err}"
        );
    }

    #[test]
    fn preflight_passes_on_cached_real_dylib() {
        // Only runs if the real cached lib exists (dev/CI machines that have run
        // recall at least once). Skips cleanly otherwise.
        let path = match ort_lib_path() {
            Ok(p) if p.is_file() => p,
            _ => return,
        };
        // Guard: skip if the cached file is implausibly small (not the real lib).
        if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) < 1_000_000 {
            return;
        }
        assert!(
            preflight_ort_dylib(&path).is_ok(),
            "pre-flight must accept the real cached ONNX Runtime at {}",
            path.display()
        );
    }

    #[test]
    fn expected_ort_minor_derives_from_version() {
        assert_eq!(expected_ort_minor(), "1.28");
    }

    #[test]
    fn verify_archive_sha256_matches() {
        // Known vector: sha256("") and sha256("abc").
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(verify_archive_sha256(b"", empty).is_ok());
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_archive_sha256(b"abc", abc).is_ok());
        // Case-insensitive compare.
        assert!(verify_archive_sha256(b"abc", &abc.to_uppercase()).is_ok());
    }

    #[test]
    fn verify_archive_sha256_rejects_mismatch() {
        let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
        let err = verify_archive_sha256(b"abc", wrong)
            .unwrap_err()
            .to_string();
        assert!(err.contains("checksum mismatch"), "msg: {err}");
        assert!(err.contains(wrong), "error names the expected hash: {err}");
    }

    #[test]
    fn ort_pinned_hash_is_64_hex() {
        let sha = ort_archive_sha256();
        assert_eq!(sha.len(), 64, "sha256 hex must be 64 chars: {sha}");
        assert!(
            sha.chars().all(|c| c.is_ascii_hexdigit()),
            "sha must be hex: {sha}"
        );
    }

    #[test]
    fn ort_url_derives_from_version() {
        let url = ort_download_url();
        let (slug, ext, _sha) = ort_platform();
        // URL is built from ORT_VERSION + platform table — no bare version literal.
        assert!(
            url.contains(&format!("v{}/", ORT_VERSION)),
            "url must embed the release tag from ORT_VERSION: {url}"
        );
        assert!(
            url.contains(&format!("onnxruntime-{}-{}.{}", slug, ORT_VERSION, ext)),
            "url must use the platform slug/ext and version: {url}"
        );
        assert!(
            url.ends_with(&format!(".{}", ext)),
            "url ext must match: {url}"
        );
        assert!(
            url.starts_with("https://github.com/microsoft/onnxruntime/releases/download/"),
            "url host/path unchanged: {url}"
        );
    }

    #[test]
    fn ort_platform_ext_is_zip_or_tgz() {
        let (_slug, ext, _sha) = ort_platform();
        assert!(matches!(ext, "zip" | "tgz"), "unexpected ext: {ext}");
    }

    #[test]
    fn ort_lib_entry_matches_macos_versioned_dylib() {
        // Real lib: version infixed before .dylib — must match.
        assert!(ort_lib_entry_matches(
            "./onnxruntime-osx-arm64-1.20.0/lib/libonnxruntime.1.20.0.dylib",
            "libonnxruntime.1.20.0.dylib",
            "libonnxruntime.dylib",
            25_477_000,
        ));
    }

    #[test]
    fn ort_lib_entry_matches_rejects_macos_symlink_and_dsym() {
        // Zero-size symlink (libonnxruntime.dylib -> versioned) must be skipped.
        assert!(!ort_lib_entry_matches(
            "./onnxruntime-osx-arm64-1.20.0/lib/libonnxruntime.dylib",
            "libonnxruntime.dylib",
            "libonnxruntime.dylib",
            0,
        ));
        // Debug-symbol copy inside a .dSYM bundle must be rejected even though it
        // has data and a matching filename.
        assert!(!ort_lib_entry_matches(
            "./onnxruntime-osx-arm64-1.20.0/lib/libonnxruntime.1.20.0.dylib.dSYM/Contents/Resources/DWARF/libonnxruntime.1.20.0.dylib",
            "libonnxruntime.1.20.0.dylib",
            "libonnxruntime.dylib",
            9_113_758,
        ));
    }

    #[test]
    fn ort_lib_entry_matches_linux_suffix_versioned_so() {
        // Real lib: version is a suffix — starts_with(lib_name) covers it.
        assert!(ort_lib_entry_matches(
            "./onnxruntime-linux-x64-1.20.0/lib/libonnxruntime.so.1.20.0",
            "libonnxruntime.so.1.20.0",
            "libonnxruntime.so",
            15_000_000,
        ));
        // Zero-size symlink form must be skipped.
        assert!(!ort_lib_entry_matches(
            "./onnxruntime-linux-x64-1.20.0/lib/libonnxruntime.so",
            "libonnxruntime.so",
            "libonnxruntime.so",
            0,
        ));
    }

    #[test]
    fn ort_lib_entry_matches_rejects_providers_sidecar() {
        // libonnxruntime_providers_shared.so has data and ends with `.so`, so the
        // infix fallback would wrongly match it. It must be rejected — it ships
        // BEFORE the real lib in tar order, so matching it poisons the cache with
        // a ~14KB file lacking OrtGetApiBase (ticket 064 field finding).
        assert!(!ort_lib_entry_matches(
            "./onnxruntime-linux-x64-1.20.0/lib/libonnxruntime_providers_shared.so",
            "libonnxruntime_providers_shared.so",
            "libonnxruntime.so",
            14_632,
        ));
    }

    #[test]
    fn ort_lib_entry_matches_exact_name() {
        // A plain, unversioned real file (has data, name equals lib_name).
        assert!(ort_lib_entry_matches(
            "onnxruntime/lib/libonnxruntime.dylib",
            "libonnxruntime.dylib",
            "libonnxruntime.dylib",
            25_000_000,
        ));
    }

    #[test]
    fn from_name_bge_base_variants() {
        assert_eq!(Model::from_name("bge-base-en-v1.5"), Some(Model::BgeBase));
        assert_eq!(Model::from_name("bge-base"), Some(Model::BgeBase));
        assert_eq!(Model::from_name("base"), Some(Model::BgeBase));
    }

    #[test]
    fn from_name_bge_small_variants() {
        assert_eq!(Model::from_name("bge-small-en-v1.5"), Some(Model::BgeSmall));
        assert_eq!(Model::from_name("bge-small"), Some(Model::BgeSmall));
        assert_eq!(Model::from_name("small"), Some(Model::BgeSmall));
    }

    #[test]
    fn from_name_case_insensitive() {
        assert_eq!(Model::from_name("BGE-BASE"), Some(Model::BgeBase));
        assert_eq!(Model::from_name("Bge-Small"), Some(Model::BgeSmall));
    }

    #[test]
    fn from_name_invalid() {
        assert_eq!(Model::from_name("nonexistent"), None);
        assert_eq!(Model::from_name(""), None);
        assert_eq!(Model::from_name("bge-large"), None);
    }

    #[test]
    fn model_dimensions() {
        assert_eq!(Model::BgeBase.dimensions(), 768);
        assert_eq!(Model::BgeSmall.dimensions(), 384);
    }

    #[test]
    fn model_name_roundtrip() {
        assert_eq!(
            Model::from_name(Model::BgeBase.name()),
            Some(Model::BgeBase)
        );
        assert_eq!(
            Model::from_name(Model::BgeSmall.name()),
            Some(Model::BgeSmall)
        );
    }
}
