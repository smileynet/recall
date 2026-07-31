use anyhow::Result;
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

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
        Ok(val) => {
            match Model::from_name(&val) {
                Some(m) => m,
                None => {
                    eprintln!("recall: unknown RECALL_MODEL='{}', valid options: bge-base, bge-small", val);
                    eprintln!("recall: falling back to default ({})", DEFAULT_MODEL.name());
                    DEFAULT_MODEL
                }
            }
        }
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
                eprintln!("recall:   Database was built with: {} ({}-dim)", stored_model.name(), stored_model.dimensions());
                eprintln!("recall:   Current config requests: {} ({}-dim)", model.name(), model.dimensions());
                eprintln!("recall:   Search results will be degraded — embeddings are incompatible.");
                eprintln!("recall:   To fix: re-ingest all data with the new model, or switch back:");
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
    std::path::PathBuf::from(home).join(".recall").join("models")
}

/// Embedding model wrapper — loads once, reuses for batch operations.
pub struct Embedder {
    model: TextEmbedding,
    which: Model,
}

impl Embedder {
    /// Load the configured model (from RECALL_MODEL env var or default).
    pub fn new() -> Result<Self> {
        Self::with_model(configured_model())
    }

    /// Load a specific model.
    pub fn with_model(which: Model) -> Result<Self> {
        let cache_dir = model_cache_dir();
        let model = TextEmbedding::try_new(
            InitOptions::new(which.fastembed_model())
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true)
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
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let results = self.model.embed(texts.to_vec(), None)?;
        Ok(results)
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
        assert_eq!(Model::from_name(Model::BgeBase.name()), Some(Model::BgeBase));
        assert_eq!(Model::from_name(Model::BgeSmall.name()), Some(Model::BgeSmall));
    }
}
