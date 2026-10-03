# Patterns

Reusable shapes discovered building recall. Each: the problem, the shape, where to find
the reference implementation (`src/...`). Copy the shape, not the recall-specifics.

## Native / FFI

**1. Pre-flight a dylib before handing it to a panicky library.**
A native-loading lib (e.g. `ort`) may `.expect()` internally on a missing symbol or wrong
version — a panic you can't `?`-handle (and can't `catch_unwind` under `panic=abort`).
Shape: `dlopen` via `libloading` yourself → check the entry symbol exists → call the
version function → compare → return a domain `Err` with remediation. If it passes, the
real lib takes the same good path. *(`embed.rs`: `preflight_ort_dylib`,
`read_ort_dylib_version` — shared with the health reporter.)*

**2. Verify-before-extract with a pinned per-platform hash.**
Downloading a native archive: SHA-256 the bytes against a vendored constant *before*
extraction, extract to a temp file, assert a plausible size, then atomically persist. A
size check alone is insufficient — a large wrong-version file passes. *(`embed.rs`:
`verify_archive_sha256`, `download_ort_runtime`.)*

**3. `compile_error!` an unsupported platform cell.**
When a `#[cfg]` platform matrix has a cell with no valid artifact (e.g. no Intel-macOS
ONNX RT ≥1.25), make that arm a `compile_error!` with a clear message — a build-time
failure beats a runtime 404. *(`embed.rs`: `ort_platform` macOS-x86_64 arm.)*

**4. One constant drives all derived artifacts.**
A single `const ORT_VERSION` feeds the download URL, the expected-version check, and the
error messages. A version bump touches one line (plus vendored hashes). *(`embed.rs`:
`ORT_VERSION` → `expected_ort_minor`, `ort_download_url`.)*

**5. Interior mutability to keep a `&self` API over a `&mut` dependency.**
A dependency's method became `&mut self` (fastembed 7's `embed`), but your type is shared
as `&T` everywhere. Wrap the field in a `Mutex` (not `RefCell` — you likely need `Sync`
for a shared `static`/test harness). Negligible cost when the work dwarfs the lock.
*(`embed.rs`: `struct Embedder { model: Mutex<...> }`.)*

**6. Bounded sub-batching to cap peak memory of a parallel call.**
A lib that fans a whole input across cores can OOM on large inputs (observed: 44MB →
~55k chunks → >20GB RSS). Loop in fixed sub-batches; results are identical. *(`embed.rs`:
`embed_batch`, `SUB_BATCH=256`.)*

## Data / ingestion

**7. Read-only reader for a foreign LIVE SQLite DB.**
`Connection::open_with_flags(path, SQLITE_OPEN_READ_ONLY)` + `busy_timeout`; never
`immutable=1` on a DB being written; prefer `rusqlite` `bundled`. *(`sqlite_source.rs`:
`open_readonly`.)*

**8. Source-key namespacing for cross-source dedup.**
Give every ingest source a distinct `source` key prefix (`kiro-sqlite:<id>`, `import:`,
`agent`) stored on each row + indexed. Re-ingest = `delete_chunks_by_source(key)` then
re-insert — idempotent, and sources can't clobber each other. *(`store.rs` source column;
`ingest.rs` key schemes.)*

**9. `>=` watermark + idempotent sink for incremental ingest.**
Query `WHERE updated_at >= :watermark` (inclusive — strict `>` drops equal-ms boundary
rows forever); the idempotent delete-by-source sink absorbs the one re-read. Advance the
watermark to max-observed-in-batch, after commit. *(`sqlite_source.rs`: `read_since`;
`ingest.rs`: `ingest_sqlite_source`.)*

**10. Stat-before-hash change detection.**
Cache `(path, mtime, size, content_hash)`; only hash a file when mtime/size differ from
cache (2800 files in 42ms). *(`scan.rs`: `scan_for_changes`, `update_cache`.)*

**11. Zero-data guard: don't write the success marker on empty input.**
If a run produced nothing, log a visible warning and do **not** write the
freshness/last-ingest marker — otherwise an empty run masquerades as a fresh one. Put the
guard at the orchestration layer so it sees all sources. *(`ingest.rs`:
`run_ingest_with_embedder_lazy`.)*

**12. Single source of truth for a normalized key.**
One idempotent `normalize_wing` (PEP 503-style: lowercase, map separators, collapse)
routes every derivation + the CLI boundary, so the same input always lands in the same
bucket. *(`store.rs`: `normalize_wing`.)*

**13. Ordered self-detecting parsers returning `Option` with a minimum-signal gate.**
Try format parsers in order; each sniffs its own fingerprint and returns `None` if it
doesn't match or yields too little signal. *(`ingest.rs`: `parse_session_file` → v3/v2/codex.)*

## Concurrency / lifecycle

**14. Process single-instance lock with a tri-state result.**
`try_acquire` returns acquired / benign-contention (another instance holds it — not an
error) / real-error, classifying OS codes (incl. Windows 32/33). *(`guard.rs`:
`ProcessGuard`, `is_benign_contention`.)*

**15. Workload-scaled watchdog timeout.**
Timeout = floor + per-unit × workload, capped by a ceiling, env-overridable; a pure
`resolve_timeout_from` that's unit-tested. *(`guard.rs`: `scaled_timeout`.)*

## Testing

**16. `OnceLock` shared heavy resource + one-time init that replays its error.**
Load the embedding model once per test binary via `OnceLock`; init the runtime once per
process via `OnceLock<Result<..>>` so a failure is cached and replayed, not retried.
*(`tests/common/mod.rs`: `shared_embedder`; `embed.rs`: `ORT_INIT`.)*

**17. Isolated test DB via temp dir + env override through the real open path.**
Point `RECALL_DB` at a `TempDir` and open through the production opener — tests exercise
the real path, not a bespoke one. *(`tests/common/mod.rs`: `test_db`.)*

**18. Golden parity harness as a committed `src/bin` tool.**
A `--dump` / `--compare` binary that embeds a fixed set and emits an exit-code-gated
verdict (`max_abs<1e-4 && top1>=0.999 && top-k Jaccard>=0.99`). In-tree so it can't
bit-rot against the lib API. *(`src/bin/parity_dump.rs`.)*

## Cross-cutting
- Document transaction-atomicity contracts at the call site (`BEGIN IMMEDIATE` … COMMIT).
- Fail loud on durability-critical ambiguity (`recall_home` errors rather than guessing a
  volatile path).
- Pure-core / impure-shell: keep the testable logic (timeout math, normalization) pure.
- Cite the ADR in a comment at the point of enforcement.
