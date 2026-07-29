use std::collections::HashMap;

use anyhow::Result;
use rusqlite::Connection;

use crate::embed::Embedder;
use crate::store::{self, SearchResult};

/// Hybrid search: BM25 (FTS5) + vector similarity, fused with RRF.
pub fn hybrid_search(
    conn: &Connection,
    embedder: &Embedder,
    query: &str,
    wing: Option<&str>,
    max_results: usize,
) -> Result<Vec<SearchResult>> {
    // BM25 search via FTS5
    let bm25_results = store::bm25_search(conn, query, wing, max_results * 3)?;

    // Vector search: embed query, compute cosine similarity against all embeddings
    let query_embedding = embedder.embed_one(query)?;
    let all_embeddings = store::all_embeddings(conn, wing)?;

    let mut vector_scores: Vec<(i64, f64)> = all_embeddings.iter()
        .map(|(id, emb)| (*id, cosine_similarity(&query_embedding, emb)))
        .collect();
    vector_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    vector_scores.truncate(max_results * 3);

    // RRF fusion
    let k = 60.0; // RRF constant
    let mut rrf_scores: HashMap<i64, f64> = HashMap::new();

    for (rank, result) in bm25_results.iter().enumerate() {
        *rrf_scores.entry(result.id).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }
    for (rank, (id, _)) in vector_scores.iter().enumerate() {
        *rrf_scores.entry(*id).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }

    // Sort by fused score, fetch top N
    let mut ranked: Vec<(i64, f64)> = rrf_scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(max_results);

    // Fetch full chunks for top results
    let mut results = Vec::new();
    for (id, score) in ranked {
        if let Ok(mut chunk) = store::get_chunk(conn, id) {
            chunk.score = score;
            results.push(chunk);
        }
    }

    Ok(results)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
