//! Cross-runtime embedding parity harness (ticket 075).
//!
//! Embeds a fixed string set with the currently-cached ONNX Runtime and either
//! dumps the vectors to JSON (`--dump <file>`) or compares against a prior dump
//! (`--compare <file>`), reporting max abs diff and top-k ranking stability.
//!
//! Usage (two-runtime comparison):
//!   1. with the OLD runtime cached:  parity_dump --dump old.json
//!   2. with the NEW runtime cached:  parity_dump --compare old.json
//!
//! A pass means the ONNX RT version bump did not materially move embeddings, so
//! the existing corpus need not be re-embedded.

use recall::embed::Embedder;

const FIXTURES: &[&str] = &[
    "We decided to use JWT tokens with 15-minute expiry and refresh rotation.",
    "The scan cache uses mtime plus file size for fast change detection.",
    "Rust was chosen for the rebuild because fastembed gives native embeddings.",
    "Database schema uses FTS5 for BM25 keyword search with WAL mode.",
    "Shader compilation uses a two-pass approach with intermediate SPIR-V.",
    "The deployment pipeline runs tests, builds, and health-checks on promote.",
    "Vector search computes cosine similarity over 768-dimensional embeddings.",
    "Session transcripts are chunked at 800 characters before embedding.",
    "The ONNX Runtime library is cached locally after the first download.",
    "Hybrid search fuses keyword and semantic ranks via reciprocal rank fusion.",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mode, path) = match args.get(1).map(|s| s.as_str()) {
        Some("--dump") => ("dump", args.get(2).expect("--dump <file>")),
        Some("--compare") => ("compare", args.get(2).expect("--compare <file>")),
        _ => {
            eprintln!("usage: parity_dump --dump <file> | --compare <file>");
            std::process::exit(2);
        }
    };

    let embedder = Embedder::new().expect("embedder init");
    let vectors: Vec<Vec<f32>> = FIXTURES
        .iter()
        .map(|s| embedder.embed_one(s).expect("embed"))
        .collect();

    if mode == "dump" {
        let json = serde_json::to_string(&vectors).unwrap();
        std::fs::write(path, json).unwrap();
        println!(
            "dumped {} vectors ({} dims) to {path}",
            vectors.len(),
            vectors[0].len()
        );
        return;
    }

    // compare
    let prev: Vec<Vec<f32>> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(prev.len(), vectors.len(), "fixture count changed");

    // Numerical: max abs diff component-wise.
    let mut max_abs = 0f32;
    for (a, b) in prev.iter().zip(&vectors) {
        for (x, y) in a.iter().zip(b) {
            max_abs = max_abs.max((x - y).abs());
        }
    }

    // Ranking: for each fixture as query, rank all others by cosine under each
    // runtime; compare top-k membership.
    let cos = |a: &[f32], b: &[f32]| -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|y| y * y).sum::<f32>().sqrt();
        dot / (na * nb)
    };
    let rank = |set: &[Vec<f32>], qi: usize| -> Vec<usize> {
        let mut idx: Vec<usize> = (0..set.len()).filter(|&j| j != qi).collect();
        idx.sort_by(|&a, &b| {
            cos(&set[qi], &set[b])
                .partial_cmp(&cos(&set[qi], &set[a]))
                .unwrap()
        });
        idx
    };
    let k = 3usize;
    let mut jaccard_sum = 0f32;
    let mut top1_agree = 0usize;
    for qi in 0..vectors.len() {
        let ra = rank(&prev, qi);
        let rb = rank(&vectors, qi);
        if ra[0] == rb[0] {
            top1_agree += 1;
        }
        let sa: std::collections::HashSet<_> = ra.iter().take(k).collect();
        let sb: std::collections::HashSet<_> = rb.iter().take(k).collect();
        let inter = sa.intersection(&sb).count() as f32;
        let uni = sa.union(&sb).count() as f32;
        jaccard_sum += inter / uni;
    }
    let n = vectors.len() as f32;
    let top1 = top1_agree as f32 / n;
    let jaccard = jaccard_sum / n;

    println!("parity: max_abs={max_abs:.3e}  top1_agree={top1:.3}  top{k}_jaccard={jaccard:.3}");
    let pass = max_abs < 1e-4 && top1 >= 0.999 && jaccard >= 0.99;
    println!(
        "verdict: {}",
        if pass {
            "PASS (no re-embed needed)"
        } else {
            "REVIEW (rankings moved)"
        }
    );
    std::process::exit(if pass { 0 } else { 1 });
}
