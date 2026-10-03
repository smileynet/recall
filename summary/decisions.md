# Decisions

Six architecture choices, their rationale, alternatives, and verdict. Portable lesson in
**bold**. Evidence in `(parens)`.

## 1. Single SQLite file, WAL mode
All state (FTS index, vectors, scan cache, meta) in one `~/.recall/recall.sqlite3`; no
daemon; an `fs2` process lock that auto-releases on crash; WAL + batch commits + a
`wal_checkpoint(TRUNCATE)` after bulk ops. **Verdict: right, no pain.**
**Lesson: for single-user local tooling, default to one embedded-DB file in WAL rather
than a server or a pile of sidecar files — crash safety and concurrency come almost
free.** (`AGENTS.md` Architecture; ADR 0001)

## 2. FTS5 (BM25) + vector, fused by RRF
Keyword and semantic search combined via Reciprocal Rank Fusion — fuse *ranks*, not
scores, so you never have to normalize across two incomparable score scales. FTS5 ships
inside SQLite (zero extra infra). **Verdict: right.**
**Lesson: when combining two rankers, RRF (`1/(k+rank)`) sidesteps score-normalization
entirely.** (`AGENTS.md`; `README.md` How It Works)

## 3. load-dynamic ONNX + a libloading pre-flight
The ONNX Runtime native lib is resolved at runtime (`load-dynamic`), not linked. Because
`ort` panics internally on a bad dylib, recall validates the lib itself first. **Verdict:
right, load-bearing.** **Lesson: don't trust a native-loading dependency's error path;
pre-validate the artifact and surface the loaded version in `health`.** (ADR 0003 §3;
details in [native-dependency-pinning.md](native-dependency-pinning.md))

## 4. Read-only reads of a live foreign DB
recall ingests kiro-cli's SQLite session DB, which another process writes concurrently.
Open it strictly `SQLITE_OPEN_READ_ONLY` via a *separate* helper (not the read-write
opener that runs `init_schema`), with a `busy_timeout`, never `immutable=1`. **Verdict:
right.** Known gap: timestamp-watermark reads can't detect hard deletes in the source.
**Lesson: a read-only consumer of someone else's live DB needs its own minimal opener;
reusing your read-write opener risks a schema write on a DB you don't own.** (ADR 0001)

## 5. Exact dependency pinning down the native-ABI chain — THE SCAR
`fastembed` exact-pins `ort`/`ort-sys`, so `ort` is not independently upgradable. A
*loose* transitive pre-release range let Cargo auto-upgrade `ort-sys` rc.9→rc.10 across
an ABI break — breaking `cargo install` and lockfile-free CI while release builds stayed
green. Fixed by a direct `=`-pin and a single `ORT_VERSION` constant driving the download
URL, the vendored SHA-256, and the pre-flight. **Verdict: caused real pain, fixed
correctly, made a later uplift cheap.** Full story + lessons:
[native-dependency-pinning.md](native-dependency-pinning.md). (ADR 0003; tickets 049→068→075)

## 6. Watermark change-detection on the source's natural signal
Incremental ingest keys off the foreign DB's `updated_at`. Query **`>=` watermark**
(inclusive) + an **idempotent delete-by-source/re-insert sink**; advance the watermark to
the max value *observed in the batch*, only *after* commit. Chosen over strict `>` (drops
rows sharing the max millisecond — a permanent silent gap) and over a composite cursor
(premature complexity). **Verdict: right, defensively.**
**Lesson: inclusive-boundary + idempotent sink beats a clever cursor; advance a watermark
from data you read, never from the wall clock; re-reading one boundary row each run is a
cheap price for never dropping one.** (ADR 0002; ticket 074)

## Cross-cutting meta-lessons
- **Test the resolve path users take** — the daily `cargo build` loop hid a `cargo
  install` break for months.
- **Name self-hiding gaps as first-class risks** — empty-ingest-reports-success,
  equal-ms-row-dropped, wrong-DLL-passes-size-check. Guard each explicitly.
- **Put version-coupling matrices in the ADR** — it made the 075 uplift mechanical.
- **Prefer an existing dependency** — the SQLite source reused `rusqlite` (already a dep);
  zero new crates for a whole feature.
