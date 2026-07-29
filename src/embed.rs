use anyhow::Result;
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

/// Embedding model wrapper — loads once, reuses for batch operations.
pub struct Embedder {
    model: TextEmbedding,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_show_download_progress(false)
        )?;
        Ok(Embedder { model })
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
