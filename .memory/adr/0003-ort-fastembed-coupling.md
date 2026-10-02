# ADR 0003 — fastembed / ort / ort-sys / ONNX Runtime version coupling

- Status: accepted
- Date: 2026-10-02
- Ticket: 068 (folds 049)

## Context

recall embeds via `fastembed`, which wraps the `ort` crate (pyke ONNX Runtime
bindings), which wraps `ort-sys` (raw FFI), which loads a native ONNX Runtime
shared library at runtime (`load-dynamic`). These four layers are version-locked
in a way that is easy to get wrong:

- `fastembed 4.9.1` **exact-pins** both `ort = "=2.0.0-rc.9"` and
  `ort-sys = "=2.0.0-rc.9"`. `ort` is therefore NOT independently upgradable by
  recall — its version is a function of the fastembed version
  (4.9.1→rc.9; 5.x→rc.10; 6.x/7.x→rc.13).
- `ort 2.0.0-rc.9` requires `ort-sys` with a **loose** `"2.0.0-rc.9"` range.
  Cargo's prerelease rule auto-upgrades a loose same-release prerelease to newer
  prereleases, so a fresh resolve pulled `ort-sys 2.0.0-rc.10` — an ABI break
  (`Option<fn>` → bare `fn` on the raw OrtApi struct) that fails to compile
  against `ort` rc.9. This broke `cargo install` and lockfile-free CI while
  `cargo build --release` (lockfile-pinned) kept working.
- `ort` rc.9 requires the native ONNX Runtime at a fixed minor, **1.20.x**
  (hardcoded `ONNXRUNTIME_VERSION` in `ort-sys/build.rs`, mirrored by recall's
  `ORT_VERSION` constant in `src/embed.rs`). A mismatched DLL makes `ort` panic.

## Decision

1. **Pin `ort-sys` exactly.** recall declares a direct
   `ort-sys = "=2.0.0-rc.9"` in `Cargo.toml`. A direct dependency constrains the
   transitive resolve on a cold build (not just via the lockfile); `=` excludes
   the rc.10 auto-upgrade; it intersects cleanly with fastembed's own `=rc.9`.
2. **Keep ONNX Runtime version as a single constant.** `ORT_VERSION` in
   `src/embed.rs` drives the download URL, the vendored per-platform SHA-256
   (ADR precedent: ticket 064), and the pre-flight version check. A version bump
   changes exactly that constant plus the vendored hashes.
3. **Pre-flight, don't trust ort's error path.** `ort::init_from().commit()`
   loads the dylib lazily and `.expect()`s inside the crate — a panic recall
   cannot `?`-handle. recall validates the dylib itself (dlopen + `OrtGetApiBase`
   + `GetVersionString` minor check) before handing it to `ort`.

## Alternatives considered

- **Lockfile-only pin (`cargo update --precise` + commit `Cargo.lock`)** — fixes
  our own builds but does NOT survive a fresh resolve for `cargo install` or
  library consumers. Kept as defense-in-depth (`--locked` in CI), not the
  primary mechanism.
- **`[patch]`** — wrong tool: replaces a crate's source, not its version within
  a range; the patched version must still satisfy the range. Rejected.
- **Bump fastembed to 7.x (ort rc.13)** to escape the rc.9 line entirely — the
  real forward move, but high blast radius (moves the native DLL 1.20→1.23,
  touches `ORT_VERSION` + deploy download + embedding parity). Deferred to
  ticket 075.
- **`catch_unwind` around `ort` init** — works only if the profile isn't
  `panic="abort"` and the panic hook still prints to stderr first. Kept as a
  possible backstop; the libloading pre-flight is the primary, cleaner path.

## Consequences

- `cargo install --path .` works again from a clean resolve.
- A bad/missing/wrong-version ONNX RT DLL yields a graceful domain error with
  remediation instead of a panic.
- `recall health` surfaces the loaded ONNX RT version for diagnosability.
- The coupling is documented, so the ticket-075 uplift starts from this matrix
  rather than rediscovering it. Any ONNX RT version bump MUST update
  `ORT_VERSION` AND re-vendor the per-platform SHA-256 hashes (ticket 064).

## Update — ticket 075 (2026-10-02): uplift to fastembed 7.1.0 / ort rc.13

recall moved off the rc.9 line onto the maintained one. Post-uplift matrix:

| layer | before (068) | after (075) |
|-------|--------------|-------------|
| fastembed | 4.9.1 | 7.1.0 |
| ort / ort-sys | =2.0.0-rc.9 | =2.0.0-rc.13 |
| ort features | load-dynamic, ndarray | + `api-24` (matches fastembed's pin) |
| ONNX Runtime | 1.20.0 | 1.28.2 |

Notes carried forward:
- fastembed 7.1.0 exact-pins `ort =2.0.0-rc.13` with `["ndarray","std","api-24"]`;
  recall's direct `ort`/`ort-sys` pins match. ort is still not independently
  choosable — it tracks fastembed.
- **ONNX RT 1.24 was never released** (Microsoft's series skips 1.23→1.25); the
  api-24 floor is satisfied by 1.28, which is ort rc.13's own download default.
- ABI: ort-sys rc.13 made `OrtApiBase.GetVersionString` a bare `fn` (was
  `Option<fn>`); the pre-flight calls it directly now.
- API: fastembed 7's `TextEmbedding::embed` takes `&mut self` → recall wraps the
  model in a `Mutex` to keep the shared `&Embedder` API and stay `Sync`.
  `InitOptions` → `TextInitOptions` (old name deprecated). MSRV → rust 1.88.
- **Embedding parity verified (no corpus re-embed):** 1.20 vs 1.28 on
  BGE-base-en-v1.5 measured max_abs = 1.9e-7, top-1 agreement 1.000, top-3
  Jaccard 1.000 (`src/bin/parity_dump.rs`). Same weights + a runtime point-bump
  do not move rankings.
- `osx-x86_64` (Intel macOS) is unsupported at ORT ≥1.25 (no asset); that
  `ort_platform()` arm is a `compile_error!`.
