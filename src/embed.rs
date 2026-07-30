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
}

/// Default model — bge-base to match the Python recall corpus.
pub const DEFAULT_MODEL: Model = Model::BgeBase;

/// Embedding model wrapper — loads once, reuses for batch operations.
pub struct Embedder {
    model: TextEmbedding,
    which: Model,
}

impl Embedder {
    /// Load the default model (bge-base-en-v1.5).
    pub fn new() -> Result<Self> {
        Self::with_model(DEFAULT_MODEL)
    }

    /// Load a specific model.
    pub fn with_model(which: Model) -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(which.fastembed_model())
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
