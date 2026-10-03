# Tools and Setup

Portable build / deploy / test tooling lessons. Lesson + tradeoff + where (`file`).

## Build profiles

**Two release profiles.** `release` = `strip + lto=true + codegen-units=1` (best
runtime/size, slowest link); `dist` inherits release but `lto="thin"` (~90% of the win,
far faster link). **Tradeoff:** full-LTO link is the long pole on iteration; `strip` loses
backtrace symbols in prod. *(`Cargo.toml`)*

## Local deploy script

**Shape: test → `build --locked` → backup → atomic swap → `--version` verify (rollback on
fail) → health-check (advisory) → report scheduler state.** Fail-**closed** at deploy,
fail-**soft** at runtime. `--locked` prevents dependency drift during the deploy build.
**Tradeoff:** the atomic mv-swap assumes same filesystem; scheduler install stays manual.
*(`scripts/deploy-local.sh`)*

**Windows specifics.** A running `.exe` can't be overwritten — wait-for-exit then
force-rename-swap. Must check `$LASTEXITCODE` after native commands (`$ErrorActionPreference
= "Stop"` does **not** catch non-zero exits of external exes). *(`scripts/deploy-local.ps1`)*

**ASCII-only PowerShell scripts.** PowerShell 5.1 decodes BOM-less files with the ANSI
codepage and mangles any non-ASCII byte (em-dash, smart quote) into a parse error; `pwsh`
7 defaults to UTF-8 so the bug is invisible there. Keep `.ps1` helpers ASCII. *(`AGENTS.md`
Constraints; ticket 065)*

## Background work

**OS scheduler, no daemon.** A scheduled task runs `recall sync` every 6h. Set the task's
execution time limit *above* the app's own self-guard ceiling so the app's watchdog fires
first (predictable cleanup). **Tradeoff:** one scheduler mechanism per OS; the scripts only
detect/suggest, they don't install it. *(`AGENTS.md` Deployment)*

## Testing

**insta snapshots with env-dependent-value filters.** Normalize temp-DB paths, timestamps,
host-dependent counts, and nullable fields (e.g. `ort_version`) before snapshotting, so
the `.snap` is portable across machines. Pair with a contract test that asserts
field-presence/type to cover what the filters blind. *(`tests/cli_snapshot.rs`,
`tests/cli_contract.rs`)*

**Benchmark + parity binaries live in `src/bin/`.** `bench_models`, `bench_quality`,
`parity_dump`. In-tree means they compile against the real lib API and can't bit-rot.
`parity_dump --dump/--compare` gives an exit-code-gated verdict for "did a runtime bump
move embeddings enough to force a re-embed?" *(`src/bin/*.rs`; ticket 075)*

**Shared heavy fixtures via `OnceLock`.** Load the embedding model once per test binary;
isolate each test's DB in a `TempDir` with `RECALL_DB` pointed at it, opened through the
real production opener. *(`tests/common/mod.rs`)*

## Runtime posture

**No network at runtime; model cached locally.** The ~416MB model + the ONNX RT dylib
cache under `~/.recall/` (stable, non-CWD-relative). First-run download is SHA-256-verified
before extraction, with sidecar rejection and a size assertion; a libloading pre-flight
validates the dylib before `ort` loads it. **Tradeoff:** a ~416MB first-run download; after
that, fully offline. *(`Cargo.toml`, `AGENTS.md`, `README.md`)*

## Cross-cutting themes
- Fail-closed at deploy, fail-soft at runtime.
- Pin exactly where ABI coupling exists; document *why* inline.
- Encode safety decisions as exit codes (parity harness, process guard).
- Normalize for portability (snapshot filters, wing normalization).
