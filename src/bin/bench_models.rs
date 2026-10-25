/// Benchmark: compare bge-base vs bge-small on real recall corpus.
/// Measures embedding speed, search latency, and retrieval quality overlap.
use std::time::Instant;

use recall::embed::Model;
use recall::{embed, search, store};

/// 20 test queries representing real recall usage patterns.
const QUERIES: &[&str] = &[
    // Decision recall
    "what did we decide about authentication",
    "why did we choose Rust over Go",
    "which embedding model should we use",
    // Architecture
    "how does the scan cache work",
    "database schema design",
    "how are sessions chunked into messages",
    // Specific technical
    "fastembed model loading performance",
    "SQLite WAL mode crash safety",
    "FTS5 BM25 search implementation",
    // Vague recollection
    "that thing about parallel file scanning",
    "something about JWT tokens",
    "the refactoring we discussed last week",
    // Cross-project
    "deployment pipeline configuration",
    "error handling patterns",
    "test fixtures and integration testing",
    // Domain-specific
    "shader compilation optimization",
    "game save file format",
    "UI component state management",
    // Meta/process
    "project planning and ticket breakdown",
    "code review feedback from last session",
];

fn main() {
    let db_path = std::env::var("RECALL_DB")
        .unwrap_or_else(|_| panic!("Set RECALL_DB to the test database path"));
    println!("Database: {}", db_path);
    println!("Queries: {}", QUERIES.len());
    println!();

    // === Performance: Model Loading ===
    println!("=== Model Loading ===");

    let t = Instant::now();
    let base_embedder = embed::Embedder::with_model(Model::BgeBase).unwrap();
    let base_load = t.elapsed();
    println!("  bge-base cold start: {:?}", base_load);

    let t = Instant::now();
    let small_embedder = embed::Embedder::with_model(Model::BgeSmall).unwrap();
    let small_load = t.elapsed();
    println!("  bge-small cold start: {:?}", small_load);
    println!();

    // === Performance: Single Embedding ===
    println!("=== Single Embedding (average of 10) ===");
    let test_text =
        "We decided to use Rust for the rebuild because fastembed-rs gives native local embeddings";

    let t = Instant::now();
    for _ in 0..10 {
        base_embedder.embed_one(test_text).unwrap();
    }
    let base_single = t.elapsed() / 10;
    println!("  bge-base: {:?}/embed", base_single);

    let t = Instant::now();
    for _ in 0..10 {
        small_embedder.embed_one(test_text).unwrap();
    }
    let small_single = t.elapsed() / 10;
    println!("  bge-small: {:?}/embed", small_single);
    println!(
        "  Ratio: {:.1}x faster (small)",
        base_single.as_micros() as f64 / small_single.as_micros() as f64
    );
    println!();

    // === Performance: Batch Embedding (64 texts) ===
    println!("=== Batch Embedding (64 texts) ===");
    let batch_texts: Vec<&str> = (0..64).map(|i| QUERIES[i % QUERIES.len()]).collect();

    let t = Instant::now();
    base_embedder.embed_batch(&batch_texts).unwrap();
    let base_batch = t.elapsed();
    println!(
        "  bge-base: {:?} ({:.0} texts/sec)",
        base_batch,
        64.0 / base_batch.as_secs_f64()
    );

    let t = Instant::now();
    small_embedder.embed_batch(&batch_texts).unwrap();
    let small_batch = t.elapsed();
    println!(
        "  bge-small: {:?} ({:.0} texts/sec)",
        small_batch,
        64.0 / small_batch.as_secs_f64()
    );
    println!(
        "  Ratio: {:.1}x faster (small)",
        base_batch.as_micros() as f64 / small_batch.as_micros() as f64
    );
    println!();

    // === Search Quality Comparison ===
    println!(
        "=== Search Quality (top-5 overlap on {} queries) ===",
        QUERIES.len()
    );
    let conn = store::open_db().unwrap();

    // Get baseline results with bge-base
    let mut base_results: Vec<Vec<i64>> = Vec::new();
    let t = Instant::now();
    for query in QUERIES {
        let results = search::hybrid_search(&conn, &base_embedder, query, None, 5).unwrap();
        base_results.push(results.iter().map(|r| r.id).collect());
    }
    let base_search_total = t.elapsed();
    println!(
        "  bge-base total search time: {:?} ({:?}/query)",
        base_search_total,
        base_search_total / QUERIES.len() as u32
    );

    // Get comparison results with bge-small (same BM25, different vector component)
    let mut small_results: Vec<Vec<i64>> = Vec::new();
    let t = Instant::now();
    for query in QUERIES {
        let results = search::hybrid_search(&conn, &small_embedder, query, None, 5).unwrap();
        small_results.push(results.iter().map(|r| r.id).collect());
    }
    let small_search_total = t.elapsed();
    println!(
        "  bge-small total search time: {:?} ({:?}/query)",
        small_search_total,
        small_search_total / QUERIES.len() as u32
    );
    println!();

    // === Quality Metrics ===
    println!("=== Quality Metrics ===");

    let mut total_overlap = 0;
    let mut total_possible = 0;
    let mut exact_match_count = 0;
    let mut top1_match_count = 0;
    let mut divergent_queries: Vec<(usize, f64)> = Vec::new();

    for (i, (base, small)) in base_results.iter().zip(small_results.iter()).enumerate() {
        let base_set: std::collections::HashSet<&i64> = base.iter().collect();
        let small_set: std::collections::HashSet<&i64> = small.iter().collect();
        let overlap = base_set.intersection(&small_set).count();
        let possible = base.len().max(small.len());

        total_overlap += overlap;
        total_possible += possible;

        if base == small {
            exact_match_count += 1;
        }
        if !base.is_empty() && !small.is_empty() && base[0] == small[0] {
            top1_match_count += 1;
        }

        let overlap_pct = if possible > 0 {
            overlap as f64 / possible as f64
        } else {
            1.0
        };
        if overlap_pct < 0.6 {
            divergent_queries.push((i, overlap_pct));
        }
    }

    let overall_overlap = total_overlap as f64 / total_possible as f64 * 100.0;
    println!(
        "  Top-5 overlap: {:.1}% ({}/{})",
        overall_overlap, total_overlap, total_possible
    );
    println!(
        "  Exact top-5 match: {}/{} queries",
        exact_match_count,
        QUERIES.len()
    );
    println!(
        "  Top-1 match: {}/{} queries",
        top1_match_count,
        QUERIES.len()
    );
    println!();

    if !divergent_queries.is_empty() {
        println!("  Divergent queries (< 60% overlap):");
        for (idx, pct) in &divergent_queries {
            println!(
                "    [{:2}] {:.0}% — \"{}\"",
                idx,
                pct * 100.0,
                QUERIES[*idx]
            );
        }
        println!();
    }

    // === Rank Correlation (Spearman-like) ===
    // For each query, compute how many of base's top-5 appear in small's top-5 at similar positions
    let mut rank_displacement_sum = 0.0;
    let mut rank_pairs = 0;
    for (base, small) in base_results.iter().zip(small_results.iter()) {
        for (base_rank, base_id) in base.iter().enumerate() {
            if let Some(small_rank) = small.iter().position(|id| id == base_id) {
                rank_displacement_sum += (base_rank as f64 - small_rank as f64).abs();
                rank_pairs += 1;
            }
        }
    }
    let avg_displacement = if rank_pairs > 0 {
        rank_displacement_sum / rank_pairs as f64
    } else {
        f64::NAN
    };
    println!(
        "  Avg rank displacement: {:.2} positions (0 = identical ranking)",
        avg_displacement
    );
    println!(
        "  (across {} ID pairs found in both result sets)",
        rank_pairs
    );
    println!();

    // === Summary ===
    println!("=== Summary ===");
    println!(
        "  Speed:   bge-small is {:.1}x faster for single embeds, {:.1}x for batch",
        base_single.as_micros() as f64 / small_single.as_micros() as f64,
        base_batch.as_micros() as f64 / small_batch.as_micros() as f64
    );
    println!(
        "  Quality: {:.1}% top-5 overlap, top-1 agrees {}/{} times",
        overall_overlap,
        top1_match_count,
        QUERIES.len()
    );
    println!(
        "  Storage: bge-base = 768×4 = 3072 bytes/chunk, bge-small = 384×4 = 1536 bytes/chunk"
    );
    println!("  At 194K chunks: bge-base = ~570MB embeddings, bge-small = ~285MB embeddings");
}
