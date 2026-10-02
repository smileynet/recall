---
id: "075"
title: "Uplift fastembed 4.9.1 -> 7.x (ort rc.13 + api-NN + ONNX RT 1.23)"
status: open
blocked_by: ["068"]
priority: medium
---

# Uplift fastembed 4.9.1 -> 7.x (ort rc.13 + api-NN + ONNX RT 1.23)

## Context

Deferred from ticket 068 (which stabilizes the *current* rc.9 stack). This ticket
is the forward move: get recall onto a newer, maintained ONNX Runtime line.

recall cannot upgrade `ort` directly — `fastembed` exact-pins it. The version
chain (verified 2026-10-02 from crates.io + pykeio/ort source,
`.scratch/research/ort-fastembed-compat.md`):

| fastembed | ort | ort-sys | ONNX Runtime native lib |
|-----------|-----|---------|-------------------------|
| 4.9.1 (current) | =2.0.0-rc.9 | =2.0.0-rc.9 | 1.20.x (hardcoded) |
| 5.x | rc.10 | rc.10 | 1.22.0 |
| 6.x | rc.13 | rc.13 | up to 1.23.x |
| 7.1.0 (latest) | =rc.13 | =rc.13 | ort default = **1.28** (api-27); 1.24 NEVER released; standardize on **1.28.2** |

The payoff of moving to fastembed 7.x / ort rc.13:
- `api-NN` cargo features let recall pin the LOWEST ONNX RT API it needs (bge
  inference is basic session+run — likely api-17/18), widening the set of
  acceptable DLLs instead of hard-matching 1.20.x.
- Lands on a maintained ort line (rc.13 is 2026-07-28 vs rc.9 2024-11-21).

## Findings from investigation (2026-10-02, post-068) — verified exact targets

Research: `.scratch/research/fastembed7-exact.md`, `.scratch/research/embedding-parity.md`
(L1 crate metadata + ort source). Local facts confirmed on this host.

**Exact target (corrects the matrix below):**
- `fastembed 7.1.0` exact-pins `ort =2.0.0-rc.13` with features
  `["ndarray","std","api-24"]`; `ort` rc.13 exact-pins `ort-sys =2.0.0-rc.13`.
  So the "select the lowest api-NN" step is **moot** — fastembed dictates
  **api-24**, i.e. ONNX Runtime **>= 1.24** (not 1.23). recall just inherits it.
- `ort`'s default ONNX RT download is 1.28; `api-NN` maps linearly 1.17..1.28.
  recall uses `load-dynamic`, so it must **ship a dylib >= 1.24** — pick one minor
  in [1.24, 1.28] and standardize (recommend matching whatever `ort` rc.13's
  download default is, or 1.24 as the floor the api pin requires).
- **Feature names unchanged**: `ort-load-dynamic` and `hf-hub-rustls-tls` still
  exist in 7.1.0 — recall's `fastembed` feature list carries over as-is.
- **Public API source-compatible** for recall's path: `TextEmbedding::try_new`,
  `EmbeddingModel::BGEBaseENV15`, `embed(docs, None)` all unchanged. Only change:
  `InitOptions` is now a `#[deprecated]` alias of `TextInitOptions` (compiles with
  a warning) — rename to silence.

**Blockers / preconditions — all CLEAR on this host:**
- fastembed 7.0 MSRV is **rust 1.88**. This host is **rustc 1.96.0** — clears it.
  (Note for CI/other dev machines: must be >= 1.88.)
- `libloading = "0.8"` is already a **direct dep** (added in 068 for the
  pre-flight). rc.13's load-dynamic no longer force-enables libloading, but
  recall already declares it — the ADR-0003 pre-flight survives the bump.

**Embedding parity risk: LOW (verified).** Same model weights (BGE-base-en-v1.5);
only the ONNX RT runtime moves. ONNX RT CPU inference is not bit-exact across
versions, but drift is ~1e-6..1e-5 — 4-5 orders below what moves cosine rankings
on L2-normalized 768-dim vectors, and recall's BM25+vector RRF fusion damps it
further. The "always re-embed on an upgrade" rule applies to WEIGHT changes, not
a same-weights runtime point-bump. **Plan: run a parity test, re-embed only if
rankings churn** (they shouldn't). Risk rises only if the bump drags in a
re-exported `model.onnx` or a changed default graph-opt level — verify it doesn't.

## CRITICAL correction (round 2, 2026-10-02) — target ONNX RT 1.28, not 1.24

The "api-24 => ONNX RT 1.24" target in the matrix and earlier notes is **wrong and
undeployable**:

- **ONNX RT 1.24.0 was never released.** Microsoft's series skips it (verified:
  `v1.24.0` asset => HTTP 404; `v1.25.0/1.27.0/1.28.0/1.28.2` => 302). "1.24" is a
  dead download target.
- **ort rc.13's actual default is api-27 / ONNX RT 1.28.0** (its
  `ort-sys/build/download/dist.tsv` points at `ms@1.28.0`; the top-level `ort`
  default features bump the compiled `ORT_API_VERSION` to 27). fastembed may
  request `api-24`, but Cargo unions features so the effective floor is the
  higher one; 1.28 satisfies everything (ORT C API is additive/monotonic — a
  runtime with API 28 satisfies any build compiled against api-N, N<=28).

**Decision: standardize recall on ONNX RT 1.28 (pin the latest 1.28 patch, e.g.
1.28.2).** It exactly matches ort rc.13's default (zero API skew), is shipped +
patched + stable, and backward-compat is free. Set `ORT_VERSION = "1.28.2"` (not
1.24). The DECISION QUESTION in the prior proposal (1.24 vs 1.28) is resolved: 1.24
was never a real option.

**Archive layout holds at 1.28** (verified identical to 1.20): asset naming
`onnxruntime-{slug}-{version}.{ext}` unchanged, internal
`lib/libonnxruntime.so.<ver>` + `libonnxruntime_providers_shared.so` sidecar
unchanged — recall's extractor (reject `libonnxruntime_*` sidecar, assert >1MB)
stays valid. **Caveat:** no `osx-x86_64` asset exists at >=1.25 (macOS is arm64
only now) — the `ort_platform()` `osx-x86_64` arm will have no archive to vendor;
handle with a logged gap or drop that target.

## Round-2 build gotchas (verified)

- **MSRV**: fastembed 7.0 needs rust 1.88; this host is 1.96 — clear.
- **`fastembed::Error` became a real enum in 6.0.0** (was an `anyhow::Error`
  alias). Any code naming/downcasting it breaks — audit recall's error handling
  of fastembed calls (likely fine; recall uses `?` into anyhow).
- **download-binaries footgun**: `ort` `load-dynamic` does NOT disable
  `download-binaries` — leaving default features on makes the BUILD hit pyke's
  CDN. recall already uses `default-features = false` on both `ort` and
  `fastembed`, so this is handled — but VERIFY with an offline build after the
  bump (network-blocked `cargo build`).
- **One potential compile break to check**: `read_ort_dylib_version`
  (`src/embed.rs`) reads `ort_sys::OrtApiBase.GetVersionString` as `Option<fn>`
  via `.ok_or_else`. The rc.10 ABI break flipped `Option<fn>` -> bare `fn` on the
  `OrtApi` struct — confirm whether `OrtApiBase` (the small bootstrap struct, not
  `OrtApi`) is still `Option`-typed in `ort-sys` rc.13; adjust the pre-flight if
  it became a bare `fn`.
- **golden_queries.rs asserts RANKINGS/keyword-presence, not float scores**
  (`assert_relevant_in_top_k`) — robust to ~1e-6 drift; expect it to pass
  unchanged, no re-baseline unless a keyword drops out of top-5.
- **Two literals that FAIL CI if forgotten**: the health JSON snapshot
  (`ort_version_expected: "1.20.0"`) and the embed unit test
  `assert_eq!(expected_ort_minor(), "1.20")` — both must move to the new version.

## Why this is high blast radius (do NOT fold into 068)

1. **Required native DLL moves 1.20 -> 1.24+.** recall's `src/embed.rs` hardcodes
   `ORT_VERSION = "1.20.0"` and builds the per-platform download URL + the
   pre-flight expected-minor check from it. The DLL auto-downloads in-binary on
   first run (the deploy scripts do NOT fetch it — confirmed in 068). So the
   change is: bump `ORT_VERSION`, re-vendor the 5 per-platform SHA-256 hashes
   (064), and let the in-binary download + pre-flight follow.
2. **ort rc.10+ ABI break** (`Option<fn>` -> bare `fn`): if recall ever touched
   the raw OrtApi it would break, but recall only uses `ort::init_from().commit()`
   — confirm no public-API signatures recall calls changed across rc.9->rc.13.
3. **Embedding parity is not guaranteed.** A different ONNX RT version can produce
   bit-different embeddings. The golden-query suite must be re-baselined
   deliberately, and the corpus may need a re-embed if vectors shift materially
   (embeddings are incompatible across model/runtime changes — same caution as a
   model switch, per README).

## What to build

- [ ] Bump `fastembed` 4.9.1 -> 7.1.0 in `Cargo.toml`; update recall's direct
      `ort`/`ort-sys` pins to `=2.0.0-rc.13`, and set `ort` features to include
      `api-24` (match fastembed's pin) alongside the existing `load-dynamic`,
      `ndarray`. Keep the `ort-load-dynamic` + `hf-hub-rustls-tls` fastembed
      features (names unchanged). Keep `libloading = "0.8"` direct (pre-flight).
- [ ] Rename `InitOptions` -> `TextInitOptions` at recall's call site (the old
      name is `#[deprecated]` in 7.x; compiles-with-warning otherwise).
- [ ] Update `src/embed.rs` `ORT_VERSION` from `1.20.0` to **`1.28.2`** (ort
      rc.13's default line; 1.24 was never released). The URL, sha256 getter, and
      pre-flight expected-minor all derive from it. Re-vendor the per-platform
      SHA-256 in `ort_platform()` for the 1.28.2 archives (reuse 064's
      fetch-and-sha256sum). **Handle the `osx-x86_64` arm**: no such asset exists
      at >=1.25 (macOS arm64 only) — drop that target or mark it a logged gap.
      Also update the `rc.9` literals in the pre-flight error message + doc
      comment, the health-snapshot `ort_version_expected`, and the
      `expected_ort_minor` unit test (`"1.20"` -> `"1.28"`).
- [ ] Verify `ort_sys::OrtApiBase.GetVersionString` is still `Option<fn>` in
      rc.13 (rc.10 flipped `OrtApi` fields to bare `fn`; confirm `OrtApiBase`);
      fix `read_ort_dylib_version` if it changed.
- [ ] Offline-build guard: after setting `default-features = false` on ort +
      fastembed (already the case), run a network-blocked `cargo build` to confirm
      no build-time CDN download (the load-dynamic + download-binaries footgun).
- [ ] **Parity test before deciding re-embed.** Embed a fixed 200-500 string set
      on the old (1.20) and new (1.24+) runtime; assert max_abs < 1e-4 and
      top-10 Jaccard >= 0.99 (recipe in `.scratch/research/embedding-parity.md`).
      Wire it behind the existing `bench_quality.rs` harness. Re-embed the corpus
      ONLY if rankings churn beyond tolerance.
- [ ] Re-baseline `tests/golden_queries.rs` if (and only if) the parity test
      shows movement; document the rationale either way.
- [ ] Confirm the bump does NOT drag in a re-exported `model.onnx` or a changed
      default graph-opt level (would escalate parity risk to near-model-change).
- [ ] Add the post-uplift row to ADR 0003's matrix; update AGENTS.md Deployment
      (`ORT_VERSION`, corpus size) + README Performance if embedding timings move.

## Acceptance criteria

- [ ] `cargo build --release` and `cargo install --path .` succeed on the new
      stack from a clean resolve (rustc >= 1.88)
- [ ] Model loads (BGE-base-en-v1.5) against ONNX RT 1.28.x; `recall search`
      returns sensible results. Offline `cargo build` confirms no build-time
      binary download.
- [ ] Parity test run and recorded: max_abs < 1e-4 AND top-10 Jaccard >= 0.99
      (or a documented decision to re-embed if not)
- [ ] All tests pass; golden-query suite re-baselined only if parity moved, with
      rationale
- [ ] `recall health` reports the new ONNX RT version; pre-flight expected-minor
      check updated and verified against a bad-version DLL
- [ ] Per-platform SHA-256 re-vendored for the new ORT archive (064 invariant)
- [ ] Corpus re-embed decision recorded (needed / not needed, with parity evidence)

## Notes / relations

- blocked_by **068** (stabilize rc.9 first: fix the resolve, graceful init,
  version logging — gives a known-good baseline to diff against).
- Pairs with **064** (per-platform SHA-256 for the DLL download — the uplift
  changes the DLL, so the pinned hashes change).
- Research: `.scratch/research/ort-fastembed-compat.md` (version table, L1 sources).
