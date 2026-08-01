---
id: 26
title: "Spike: cargo-dist + load-dynamic feasibility for public release"
status: open
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

- [ ] Determined: does fastembed-rs support ort `load-dynamic`? (yes/no + evidence)
- [ ] If yes: prototype builds and runs with runtime-loaded ONNX
- [ ] If no: documented the bundled approach with cargo-dist config
- [ ] cargo-dist `init` + `plan` runs without errors
- [ ] Decision recorded: which approach for v0.1.0 public release?

## Sources from research

- [ort features](https://lib.rs/crates/ort/features) — load-dynamic documentation
- [cargo-dist config](https://axodotdev.github.io/cargo-dist/book/reference/config.html) — include, extra-artifacts, build-local-artifacts
- [pykeio/ort](https://github.com/pykeio/ort) — ort crate source
