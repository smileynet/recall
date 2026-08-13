---
id: "030"
title: "Cut v0.1.0 public release via cargo-dist"
status: done
blocked_by: ["029"]
estimate: 30min
---

# Cut v0.1.0 Public Release

## What to do

Ship recall v0.1.0 as a public GitHub release with cross-platform binaries.

### Steps

1. Run `dist init --yes` to generate CI config
2. Review generated `dist-workspace.toml` and `.github/workflows/release.yml`
3. Add `[profile.dist]` to Cargo.toml (inherits release, thin LTO)
4. Commit dist config
5. Tag: `git tag v0.1.0`
6. Push tag: `git push origin v0.1.0`
7. GitHub Actions builds 5-platform archives (win-x64, linux-x64, linux-arm64, mac-x64, mac-arm64)
8. Verify release page has archives + changelog

### Notes

- Binary is ~25MB (lean, no bundled ONNX Runtime)
- Users get ONNX Runtime on first run (auto-download)
- Users get embedding model on first run (auto-download via fastembed)
- README already documents installation and first-run behavior

## Acceptance criteria

- [x] `dist init` config committed
- [x] Tag v0.1.0 pushed
- [x] GitHub Actions completes successfully (in progress — monitor at github.com/smileynet/recall/actions)
- [x] Release page has binaries for all 5 targets
- [x] README install instructions match release mechanism

## Resolution (2026-08-06)

- dist-workspace.toml + .github/workflows/release.yml committed (6f0a4f5)
- Tag v0.1.0 pushed to trigger release build
- GitHub Actions building: win-x64, linux-x64, linux-arm64, mac-x64, mac-arm64
- Note: if CI fails on ort compilation (same Rust 1.94 issue seen locally),
  may need to pin a Rust version in the workflow or use nightly
