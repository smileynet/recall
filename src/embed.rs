use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

#[cfg(target_os = "windows")]
use std::io::Write;
use std::path::PathBuf;
use std::sync::Once;

// ─── ONNX Runtime management (load-dynamic) ─────────────────────────────────

/// The ONNX Runtime version required by ort 2.0.0-rc.9.
const ORT_VERSION: &str = "1.20.0";

/// Platform-specific download URL for ONNX Runtime from Microsoft's GitHub releases.
fn ort_download_url() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "https://github.com/microsoft/onnxruntime/releases/download/v1.20.0/onnxruntime-win-x64-1.20.0.zip"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "https://github.com/microsoft/onnxruntime/releases/download/v1.20.0/onnxruntime-linux-x64-1.20.0.tgz"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "https://github.com/microsoft/onnxruntime/releases/download/v1.20.0/onnxruntime-osx-x86_64-1.20.0.tgz"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "https://github.com/microsoft/onnxruntime/releases/download/v1.20.0/onnxruntime-osx-arm64-1.20.0.tgz"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "https://github.com/microsoft/onnxruntime/releases/download/v1.20.0/onnxruntime-linux-aarch64-1.20.0.tgz"
    }
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

/// Directory where we cache the ONNX Runtime library.
fn ort_lib_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".recall").join("lib")
}

/// Full path to the cached ONNX Runtime library.
fn ort_lib_path() -> PathBuf {
    ort_lib_dir().join(ort_lib_filename())
}

/// Ensure ONNX Runtime is available and initialize ort to use it.
/// Downloads on first run if not cached. Must be called before any ort API usage.
static ORT_INIT: Once = Once::new();
static mut ORT_INIT_ERROR: Option<String> = None;

pub fn ensure_ort_runtime() -> Result<()> {
    ORT_INIT.call_once(|| {
        if let Err(e) = ensure_ort_runtime_inner() {
            unsafe {
                ORT_INIT_ERROR = Some(format!("{:#}", e));
            }
        }
    });
    unsafe {
        if let Some(ref err) = ORT_INIT_ERROR {
            anyhow::bail!("ONNX Runtime initialization failed: {}", err);
        }
    }
    Ok(())
}

fn ensure_ort_runtime_inner() -> Result<()> {
    let lib_path = ort_lib_path();

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

    let response = ureq::get(url)
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

#[cfg(target_os = "windows")]
fn extract_lib_from_zip(data: &[u8], lib_name: &str, target_path: &PathBuf) -> Result<()> {
    // Minimal zip extraction — find the DLL entry and extract it
    // ZIP format: search for the file by scanning central directory
    use std::io::{Cursor, Read, Seek, SeekFrom};

    let mut cursor = Cursor::new(data);
    let len = data.len();

    // Find End of Central Directory record (last 22+ bytes)
    let eocd_search_start = len.saturating_sub(65557);
    let mut eocd_pos = None;
    for i in (eocd_search_start..len.saturating_sub(3)).rev() {
        if data[i] == 0x50 && data[i + 1] == 0x4b && data[i + 2] == 0x05 && data[i + 3] == 0x06 {
            eocd_pos = Some(i);
            break;
        }
    }
    let eocd_pos = eocd_pos.ok_or_else(|| anyhow::anyhow!("Invalid ZIP: no EOCD found"))?;

    // Parse EOCD to find central directory offset
    let cd_offset = u32::from_le_bytes([
        data[eocd_pos + 16],
        data[eocd_pos + 17],
        data[eocd_pos + 18],
        data[eocd_pos + 19],
    ]) as u64;
    let cd_entries = u16::from_le_bytes([data[eocd_pos + 10], data[eocd_pos + 11]]) as usize;

    cursor.seek(SeekFrom::Start(cd_offset))?;

    for _ in 0..cd_entries {
        let mut sig = [0u8; 4];
        cursor.read_exact(&mut sig)?;
        if sig != [0x50, 0x4b, 0x01, 0x02] {
            break;
        }

        let mut header = [0u8; 42];
        cursor.read_exact(&mut header)?;

        let compressed_size =
            u32::from_le_bytes([header[16], header[17], header[18], header[19]]) as u64;
        let uncompressed_size =
            u32::from_le_bytes([header[20], header[21], header[22], header[23]]) as u64;
        let name_len = u16::from_le_bytes([header[24], header[25]]) as usize;
        let extra_len = u16::from_le_bytes([header[26], header[27]]) as usize;
        let comment_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let local_header_offset =
            u32::from_le_bytes([header[38], header[39], header[40], header[41]]) as u64;
        let compression = u16::from_le_bytes([header[6], header[7]]);

        let mut name_buf = vec![0u8; name_len];
        cursor.read_exact(&mut name_buf)?;
        let name = String::from_utf8_lossy(&name_buf);

        // Skip extra and comment
        cursor.seek(SeekFrom::Current((extra_len + comment_len) as i64))?;

        if name.ends_with(lib_name) {
            // Found it — read from local file header
            let mut local_cursor = Cursor::new(data);
            local_cursor.seek(SeekFrom::Start(local_header_offset))?;

            let mut local_sig = [0u8; 4];
            local_cursor.read_exact(&mut local_sig)?;
            let mut local_header = [0u8; 26];
            local_cursor.read_exact(&mut local_header)?;
            let local_name_len = u16::from_le_bytes([local_header[22], local_header[23]]) as u64;
            let local_extra_len = u16::from_le_bytes([local_header[24], local_header[25]]) as u64;
            local_cursor.seek(SeekFrom::Current((local_name_len + local_extra_len) as i64))?;

            let pos = local_cursor.position() as usize;
            let file_data = if compression == 0 {
                // Stored (no compression)
                data[pos..pos + uncompressed_size as usize].to_vec()
            } else if compression == 8 {
                // Deflate
                let mut decoder =
                    flate2::read::DeflateDecoder::new(&data[pos..pos + compressed_size as usize]);
                let mut out = Vec::with_capacity(uncompressed_size as usize);
                decoder.read_to_end(&mut out)?;
                out
            } else {
                anyhow::bail!("Unsupported ZIP compression method: {}", compression);
            };

            let mut file = std::fs::File::create(target_path)?;
            file.write_all(&file_data)?;
            return Ok(());
        }
    }
    anyhow::bail!("Could not find {} in the ZIP archive", lib_name);
}

#[cfg(not(target_os = "windows"))]
fn extract_lib_from_zip(_data: &[u8], _lib_name: &str, _target_path: &PathBuf) -> Result<()> {
    anyhow::bail!("ZIP extraction not expected on this platform (ONNX Runtime uses .tgz)")
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
fn model_cache_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("FASTEMBED_CACHE_DIR") {
        return std::path::PathBuf::from(dir);
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".recall")
        .join("models")
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
        let cache_dir = model_cache_dir();
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
