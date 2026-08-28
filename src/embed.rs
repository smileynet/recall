use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use std::path::PathBuf;
use std::sync::OnceLock;

// ─── ONNX Runtime management (load-dynamic) ─────────────────────────────────

/// The ONNX Runtime version required by ort 2.0.0-rc.9.
const ORT_VERSION: &str = "1.20.0";

/// Platform-specific ONNX Runtime release: the archive name slug and its
/// extension. Single `#[cfg]` cascade — the download URL and (in ticket 051) the
/// pinned SHA-256 both key off this, so a version bump touches only `ORT_VERSION`.
fn ort_platform() -> (&'static str, &'static str) {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        ("win-x64", "zip")
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        ("linux-x64", "tgz")
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        ("osx-x86_64", "tgz")
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        ("osx-arm64", "tgz")
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        ("linux-aarch64", "tgz")
    }
}

/// Platform-specific download URL for ONNX Runtime, derived from `ORT_VERSION`.
fn ort_download_url() -> String {
    let (slug, ext) = ort_platform();
    format!(
        "https://github.com/microsoft/onnxruntime/releases/download/v{v}/onnxruntime-{slug}-{v}.{ext}",
        v = ORT_VERSION,
    )
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

    // Tell ort where to find the library (overrides System32 or PATH search)
    ort::init_from(lib_path.to_string_lossy().as_ref()).commit()?;
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

    // Validate the extracted library is a plausible size before committing.
    let extracted_len = std::fs::metadata(&tmp_path)?.len();
    anyhow::ensure!(
        extracted_len >= 1_000_000,
        "extracted ONNX Runtime too small ({} bytes) — likely corrupt",
        extracted_len
    );

    tmp.persist(target_path)
        .map_err(|e| anyhow::anyhow!("failed to persist ONNX Runtime: {}", e))?;

    eprintln!("  Cached at: {}", target_path.display());
    Ok(())
}

fn extract_lib_from_tgz(data: &[u8], lib_name: &str, target_path: &PathBuf) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_size = entry.header().size().unwrap_or(0);
        let path = entry.path()?.to_path_buf();
        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        // Match the library file (may be in a subdirectory like onnxruntime-linux-x64-1.20.0/lib/).
        // Skip zero-size entries: on Linux the archive contains symlinks
        // (libonnxruntime.so → libonnxruntime.so.1.20.0) that share the prefix
        // but carry no data — extracting one would produce an empty file.
        if (filename == lib_name || filename.starts_with(lib_name)) && entry_size > 0 {
            let mut file = std::fs::File::create(target_path)?;
            std::io::copy(&mut entry, &mut file)?;
            return Ok(());
        }
    }
    anyhow::bail!("Could not find {} in the downloaded archive", lib_name);
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
    model: TextEmbedding,
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
            InitOptions::new(which.fastembed_model())
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true),
        )?;
        Ok(Embedder { model, which })
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
        let results = self.model.embed(vec![text], None)?;
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
            let batch = self.model.embed(window.to_vec(), Some(SUB_BATCH))?;
            out.extend(batch);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ort_url_derives_from_version() {
        let url = ort_download_url();
        let (slug, ext) = ort_platform();
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
        let (_slug, ext) = ort_platform();
        assert!(matches!(ext, "zip" | "tgz"), "unexpected ext: {ext}");
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
