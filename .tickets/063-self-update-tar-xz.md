---
id: "063"
title: "Self-update: extract .tar.xz (Linux/macOS) — needs pure-Rust xz spike"
status: open
blocked_by: ["051"]
priority: high
validation_criteria:
  - "recall update extracts .tar.xz on Linux/macOS"
  - "cargo tree -i liblzma-sys empty (no C toolchain)"
---

# Self-update: extract .tar.xz (Linux/macOS) — needs pure-Rust xz spike

## Context

Split from 051 (decision b, 2026-08-28). 051 ships the Windows `.zip` extraction
+ checksums + timeout + ORT checksum. Linux/macOS release assets are `.tar.xz`
(cargo-dist default), which `extract_binary` can't decode. Deferred here because
the xz decoder needs a spike before committing a dependency.

## Spike first (blocks implementation)

Research (`.scratch/research/xz-decoder.md`) found the pure-Rust decode-only
options — the ticket's original "liblzma-rs" was actually the C-linked xz2 fork.
- [ ] Spike `lzma-rs` (152★, `forbid(unsafe)`, decode-first): does it decode a
      REAL cargo-dist `.tar.xz` asset (multi-stream / check types / BCJ+delta
      filters)? Buffers whole tar in memory (no incremental Read).
- [ ] If `lzma-rs` rejects it, spike `lzma-rust2` (full XZ, streaming `XzReader`
      implements `Read` → pairs with `tar::Archive` with no full buffer).
- [ ] Reject anything pulling `liblzma-sys` (C toolchain) — verify via
      `cargo tree -i liblzma-sys` (must be empty).

## What to build (after spike)

- [ ] Add the chosen pure-Rust xz dep (pinned).
- [ ] In `update.rs::extract_binary`, add the `.tar.xz` arm: xz-decode → `tar`
      extract of the `recall`/`recall.exe` entry (dispatch seam already added by
      051).
- [ ] Unit test: extract the recall binary from an in-memory `.tar.xz` fixture.
- [ ] Manual: fetch this-platform's live `.tar.xz` asset (on a Linux/macOS box)
      and confirm `recall update`'s extract + digest-verify succeed end-to-end.

## Acceptance criteria

- [ ] `recall update` extracts `.tar.xz` on Linux/macOS (verified against a real
      release asset)
- [ ] `cargo tree -i liblzma-sys` empty (no C toolchain dependency)
- [ ] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- Unit: `.tar.xz` fixture → recall binary bytes extracted
- `cargo tree -i liblzma-sys` → empty
- Manual on Linux/macOS: real release `.tar.xz` extract + digest verify
