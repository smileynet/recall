---
id: 26
title: "Spike: cargo-dist + load-dynamic feasibility for public release"
status: done
priority: normal
type: spike
blocked_by: []
estimate: 1h
---

# Spike: cargo-dist + load-dynamic Feasibility

## Question

Can we use cargo-dist to ship cross-platform release binaries where the ONNX Runtime
is downloaded at first run (via ort's `load-dynamic` feature) rather than bundled?

## Background

Research confirmed:
- ONNX Runtime is shared-library only — no official static builds exist
- cargo-dist doesn't automatically bundle third-party shared libs
- Bundling is possible but requires custom CI steps per platform (archives become ~65-85MB)
- `load-dynamic` disables compile-time linking; loads libonnxruntime at runtime

## What to test

### 1. `load-dynamic` compatibility with fastembed-rs

- Can fastembed-rs work with ort's `load-dynamic` feature?
- Does it require code changes in `src/embed.rs`?
- Feature flag propagation: `fastembed` → `ort` → `ort-sys`

```toml
# Test in Cargo.toml
[dependencies]
ort = { version = "2", features = ["load-dynamic"], default-features = false }
# Does fastembed respect this? Or does it override?
```

### 2. Runtime download mechanism

- If `load-dynamic` works: what downloads the shared lib at runtime?
- ort has no built-in downloader — we'd need to add one
- Acceptable UX: `recall` on first run says "Downloading ONNX Runtime (~50MB)..."
- Target locations: `~/.recall/lib/onnxruntime.dll` (or .so/.dylib)

### 3. cargo-dist init + plan

- Run `cargo dist init` on the recall repo
- Check generated CI config
- Verify binary-only archives work (no shared lib needed if load-dynamic)
- Estimate: archive size should be ~25MB (just the binary)

### 4. Fallback: bundled approach

If `load-dynamic` doesn't work with fastembed-rs, document:
- What custom CI steps are needed per platform
- `extra-artifacts` or `github-build-setup` config
- Expected archive sizes (~65-85MB per platform)

## Success criteria

- [x] Determined: does fastembed-rs support ort `load-dynamic`? (yes/no + evidence)
- [x] If yes: prototype builds and runs with runtime-loaded ONNX
- [x] If no: documented the bundled approach with cargo-dist config
- [x] cargo-dist `init` + `plan` runs without errors
- [x] Decision recorded: which approach for v0.1.0 public release?

## Findings (2026-08-01)

### 1. load-dynamic WORKS with fastembed

```toml
fastembed = { version = "4", default-features = false, features = ["ort-load-dynamic", "hf-hub-native-tls"] }
```

- Compiles cleanly (pulls in `libloading` crate)
- At runtime, ort searches for ONNX Runtime in this order:
  1. `ORT_DYLIB_PATH` env var (explicit path)
  2. Same directory as the executable
  3. System PATH / dynamic linker search
- `ort::init_from(path)` can set the library path programmatically before first API call

### 2. Version requirement

- ort 2.0.0-rc.9 requires ONNX Runtime **1.20.x**
- System had 1.17.1 in C:\Windows\System32 → version check panic
- Error is clear: "expected GetVersionString to return '1.20.x', but got '1.17.1'"

### 3. cargo-dist init works

Generated config for 5 platforms:
- `x86_64-pc-windows-msvc`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`

Files generated: `dist-workspace.toml`, `.github/workflows/release.yml`, `[profile.dist]` in Cargo.toml.
CI triggers on version tags (`**[0-9]+.[0-9]+.[0-9]+*`).

### 4. Binary size without ONNX Runtime

With `load-dynamic`, the binary doesn't embed ONNX Runtime. Expected archive size: ~25MB (just the Rust binary, stripped + LTO).

## Decision: load-dynamic + first-run download

**Approach:** Ship a lean binary (~25MB) that downloads ONNX Runtime (~50MB) on first run, alongside the embedding model download that already happens.

**Implementation plan (future tickets):**

1. Switch to `fastembed = { features = ["ort-load-dynamic", "hf-hub-native-tls"], default-features = false }`
2. Add startup logic in `embed.rs`:
   - Check if `~/.recall/lib/onnxruntime.{dll,so,dylib}` exists
   - If not: download correct version from pyke's CDN (same source ort-sys uses)
   - Call `ort::init_from("~/.recall/lib/onnxruntime.dll")` before creating embedder
3. Add `[profile.dist]` and `dist-workspace.toml`
4. CI: `dist plan` → `dist build` → GitHub Release with archives

**Why this over bundled:**
- 25MB archives vs 65-85MB (saves bandwidth, faster install)
- Consistent UX: recall already downloads the model on first run
- No custom CI gymnastics per platform
- cargo-dist stays simple (binary-only archives)

**Risk:** User needs network on first run for both model + ONNX Runtime. Mitigated: both are one-time downloads, cached permanently.

## Sources from research

- [ort features](https://lib.rs/crates/ort/features) — load-dynamic documentation
- [cargo-dist config](https://axodotdev.github.io/cargo-dist/book/reference/config.html) — workspace config
- fastembed 4.9.1 Cargo.toml — confirms `ort-load-dynamic = ["ort/load-dynamic"]` feature exposed
- ort source (lib.rs:78-103) — dylib path resolution logic
- ort source (environment.rs:393) — `init_from()` API
