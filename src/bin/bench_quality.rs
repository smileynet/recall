/// Quality comparison: embed same corpus with both models, compare retrieved results.
/// This is a fair apples-to-apples test — each model searches its own embedding space.
use std::io::BufRead;
use std::time::Instant;

use recall::embed::{Embedder, Model};

const QUERIES: &[(&str, &str)] = &[
    // (query, expected_keywords_in_good_results)
    ("what did we decide about authentication", "auth,jwt,token,login,session"),
    ("why did we choose Rust over Go", "rust,go,fastembed,cgo,performance"),
    ("which embedding model should we use", "bge,embedding,model,dimension,fastembed"),
    ("how does the scan cache work", "scan,cache,mtime,size,hash,stat"),
    ("database schema design", "schema,table,column,migration,sqlite"),
    ("how are sessions chunked into messages", "chunk,message,session,role,user,assistant"),
    ("fastembed model loading performance", "fastembed,model,load,cold,onnx"),
    ("SQLite WAL mode crash safety", "wal,sqlite,crash,journal,lock"),
    ("FTS5 BM25 search implementation", "fts5,bm25,search,rank,match"),
    ("that thing about parallel file scanning", "scan,parallel,jwalk,file,walk"),
    ("something about JWT tokens", "jwt,token,auth,refresh,expir"),
    ("the refactoring we discussed last week", "refactor,module,extract,simplif"),
    ("deployment pipeline configuration", "deploy,pipeline,ci,build,release"),
    ("error handling patterns", "error,result,anyhow,handle,unwrap"),
    ("test fixtures and integration testing", "test,fixture,integration,assert"),
    ("shader compilation optimization", "shader,compile,optim,gpu,glsl"),
    ("game save file format", "save,file,format,serial,load"),
    ("UI component state management", "state,component,ui,render,update"),
    ("project planning and ticket breakdown", "plan,ticket,spec,milestone,task"),
    ("code review feedback from last session", "review,feedback,change,suggest"),
];

fn main() {
    println!("=== Fair Quality Comparison: bge-base vs bge-small ===");
    println!("Same 5000 chunks embedded with each model, same queries against each.\n");

    // Load both models
    println!("Loading models...");
    let t = Instant::now();
    let base_embedder = Embedder::with_model(Model::BgeBase).unwrap();
    println!("  bge-base loaded: {:?}", t.elapsed());
    let t = Instant::now();
    let small_embedder = Embedder::with_model(Model::BgeSmall).unwrap();
    println!("  bge-small loaded: {:?}", t.elapsed());
    println!();

    // Load sample chunks
    let chunks = load_sample_chunks("D:\\tmp\\sample-chunks.jsonl");
    println!("Loaded {} sample chunks", chunks.len());

    // Embed all chunks with both models
    println!("\nEmbedding {} chunks with bge-base...", chunks.len());
    let t = Instant::now();
    let base_embeddings = embed_all(&base_embedder, &chunks);
    println!("  Done: {:?} ({:.0} chunks/sec)", t.elapsed(), chunks.len() as f64 / t.elapsed().as_secs_f64());

    println!("Embedding {} chunks with bge-small...", chunks.len());
    let t = Instant::now();
    let small_embeddings = embed_all(&small_embedder, &chunks);
    println!("  Done: {:?} ({:.0} chunks/sec)", t.elapsed(), chunks.len() as f64 / t.elapsed().as_secs_f64());

    // Run queries against both
    println!("\n=== Query Results Comparison ===\n");

    let mut base_relevance_total = 0.0;
    let mut small_relevance_total = 0.0;
    let mut base_wins = 0;
    let mut small_wins = 0;
    let mut ties = 0;

    for (i, (query, keywords)) in QUERIES.iter().enumerate() {
        let kw_list: Vec<&str> = keywords.split(',').collect();

        // Embed query with each model
        let base_query_emb = base_embedder.embed_one(query).unwrap();
        let small_query_emb = small_embedder.embed_one(query).unwrap();

        // Find top-5 by cosine similarity
        let base_top5 = find_top_k(&base_query_emb, &base_embeddings, 5);
        let small_top5 = find_top_k(&small_query_emb, &small_embeddings, 5);

        // Score relevance: how many expected keywords appear in top-5 results
        let base_score = score_relevance(&base_top5, &chunks, &kw_list);
        let small_score = score_relevance(&small_top5, &chunks, &kw_list);

        base_relevance_total += base_score;
        small_relevance_total += small_score;

        let winner = if base_score > small_score + 0.01 {
            base_wins += 1;
            "BASE >"
        } else if small_score > base_score + 0.01 {
            small_wins += 1;
            "< SMALL"
        } else {
            ties += 1;
            "  TIE  "
        };

        println!("[{:2}] {:50} base={:.2} small={:.2} {}",
            i + 1, query, base_score, small_score, winner);
    }

    // Print detailed results for a few interesting queries
    println!("\n=== Detailed Results (3 sample queries) ===\n");
    for &idx in &[0, 2, 9] {
        let (query, keywords) = QUERIES[idx];
        let kw_list: Vec<&str> = keywords.split(',').collect();

        let base_query_emb = base_embedder.embed_one(query).unwrap();
        let small_query_emb = small_embedder.embed_one(query).unwrap();
        let base_top3 = find_top_k(&base_query_emb, &base_embeddings, 3);
        let small_top3 = find_top_k(&small_query_emb, &small_embeddings, 3);

        println!("  Query: \"{}\"", query);
        println!("  Expected keywords: {}", keywords);
        println!("  --- bge-base top-3 ---");
        for (rank, (idx, sim)) in base_top3.iter().enumerate() {
            let preview: String = chunks[*idx].content.chars().take(100).collect();
            let has_kw = kw_list.iter().any(|kw| chunks[*idx].content.to_lowercase().contains(kw));
            println!("    [{}] sim={:.3} {} | {}", rank + 1, sim, if has_kw { "✓" } else { "✗" }, preview.replace('\n', " "));
        }
        println!("  --- bge-small top-3 ---");
        for (rank, (idx, sim)) in small_top3.iter().enumerate() {
            let preview: String = chunks[*idx].content.chars().take(100).collect();
            let has_kw = kw_list.iter().any(|kw| chunks[*idx].content.to_lowercase().contains(kw));
            println!("    [{}] sim={:.3} {} | {}", rank + 1, sim, if has_kw { "✓" } else { "✗" }, preview.replace('\n', " "));
        }
        println!();
    }

    // Summary
    println!("=== Summary ===");
    println!("  bge-base total relevance score:  {:.1}/{}", base_relevance_total, QUERIES.len());
    println!("  bge-small total relevance score: {:.1}/{}", small_relevance_total, QUERIES.len());
    println!("  bge-base wins: {}, bge-small wins: {}, ties: {}", base_wins, small_wins, ties);
    let base_avg = base_relevance_total / QUERIES.len() as f64;
    let small_avg = small_relevance_total / QUERIES.len() as f64;
    println!("  Average relevance: base={:.3}, small={:.3}", base_avg, small_avg);
    if base_avg > small_avg {
        println!("  → bge-base produces {:.1}% more relevant results", (base_avg - small_avg) / small_avg * 100.0);
    } else if small_avg > base_avg {
        println!("  → bge-small produces {:.1}% more relevant results", (small_avg - base_avg) / base_avg * 100.0);
    } else {
        println!("  → Models produce equivalent quality results");
    }
}

struct Chunk {
    content: String,
    #[allow(dead_code)]
    wing: String,
}

fn load_sample_chunks(path: &str) -> Vec<Chunk> {
    let file = std::fs::File::open(path).unwrap();
    let reader = std::io::BufReader::new(file);
    reader.lines()
        .filter_map(|line| {
            let line = line.ok()?;
            let v: serde_json::Value = serde_json::from_str(&line).ok()?;
            Some(Chunk {
                content: v["content"].as_str()?.to_string(),
                wing: v["wing"].as_str()?.to_string(),
            })
        })
        .collect()
}

fn embed_all(embedder: &Embedder, chunks: &[Chunk]) -> Vec<Vec<f32>> {
    let mut all_embeddings = Vec::with_capacity(chunks.len());
    // Batch in groups of 64
    for batch in chunks.chunks(64) {
        let texts: Vec<&str> = batch.iter().map(|c| c.content.as_str()).collect();
        let embs = embedder.embed_batch(&texts).unwrap();
        all_embeddings.extend(embs);
    }
    all_embeddings
}

fn find_top_k(query_emb: &[f32], embeddings: &[Vec<f32>], k: usize) -> Vec<(usize, f64)> {
    let mut scores: Vec<(usize, f64)> = embeddings.iter()
        .enumerate()
        .map(|(i, emb)| (i, cosine_sim(query_emb, emb)))
        .collect();
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scores.truncate(k);
    scores
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

fn score_relevance(top_k: &[(usize, f64)], chunks: &[Chunk], keywords: &[&str]) -> f64 {
    // Score: what fraction of top-k results contain at least one expected keyword?
    // Weighted by position (top result worth more)
    let weights = [0.35, 0.25, 0.20, 0.12, 0.08]; // sum = 1.0
    let mut score = 0.0;
    for (rank, (idx, _sim)) in top_k.iter().enumerate() {
        let content_lower = chunks[*idx].content.to_lowercase();
        let has_keyword = keywords.iter().any(|kw| content_lower.contains(kw));
        if has_keyword {
            score += weights.get(rank).copied().unwrap_or(0.05);
        }
    }
    score
}
