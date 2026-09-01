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

## Proposal (2026-08-31, spike resolved + code-verified)

Blocker 051 is `done` (Windows `.zip` + checksums + timeout + ORT checksum
shipped). The prior `.scratch/research/xz-decoder.md` was wiped (scratch is
ephemeral), so the spike was re-run this session — findings below re-establish
it. Verdict: **the spike resolves cleanly; this ticket is ready to implement.**

### Current seam (verified in `src/update.rs`)

`extract_binary(archive_bytes, asset_name)` already dispatches on suffix:
- `.zip` → `archive::extract_named_from_zip` (Windows)
- `.tar.gz` / `.tgz` → `extract_binary_tar_gz`: `flate2::read::GzDecoder` wrapped
  in `tar::Archive`, iterate entries, match `file_name() == "recall"`, read out.
- `.tar.xz` → currently `anyhow::bail!(... see ticket 063)`
- else → `bail!(unrecognized ...)`

Baseline deps already present: `tar = "0.4"`, `flate2 = "1"`. `cargo tree -i
liblzma-sys` → empty (no C dependency today — the constraint holds at baseline).
The `.tar.xz` arm is a drop-in twin of the gz arm: swap the decoder, keep the
`tar::Archive` entry-scan loop verbatim.

### Spike result — use `lzma-rust2` (not `lzma-rs`)

| | lzma-rs (gendx) | **lzma-rust2 (hasenbanck)** |
|---|---|---|
| Latest / maint | 0.3.0 (2025-06), dormant | **0.20.1 (2026-08-30), active** |
| Streaming `Read`? | No — `xz_decompress(BufRead, Write)` buffers whole stream | **Yes — `XzReader: Read`, composes with `tar::Archive`** |
| Real cargo-dist asset (BCJ/multi-stream)? | Partial — no BCJ filters, weak multi-stream | **Full FilterType (all BCJ + Delta + Lzma2) + CheckType** |
| unsafe | none (`forbid(unsafe_code)`) | safe by default; unsafe only behind opt-in `optimization` feature |
| liblzma-sys | none ✅ | none in normal build (liblzma is dev-only/benchmarks) ✅ |

cargo-dist's default `unix-archive` **is `.tar.xz`**, and its XZ output can use
BCJ filters + multi-block streams — which only `lzma-rust2` decodes fully.
`lzma-rs`'s buffer-to-buffer API also can't stream into `tar` without a full
intermediate buffer. So `lzma-rust2` wins on correctness AND on fitting the
existing gz pattern. Sources: docs.rs/lzma-rust2, github hasenbanck/lzma-rust2,
cargo-dist config reference (see `.scratch/research/xz-decoder.md`).

### Change plan

- [ ] Add dep (pinned, C-free, safe build):
      `lzma-rust2 = { version = "=0.20.1", default-features = false, features = ["std", "xz"] }`
      (keep `std` for `XzReader`; omit `optimization` to stay 100% safe Rust).
- [ ] `extract_binary_tar_xz(archive_bytes, binary_name)` — a copy of
      `extract_binary_tar_gz` with `GzDecoder::new(archive_bytes)` replaced by
      `lzma_rust2::XzReader::new(archive_bytes)` (confirm the exact `XzReader::new`
      constructor arg against docs.rs — `Read`-based shape is confirmed).
- [ ] Replace the `.tar.xz` bail arm with a call to the new helper.
- [ ] Unit test: build an in-memory `.tar.xz` fixture containing a fake `recall`
      entry, assert the bytes round-trip out. (The existing
      `extract_binary_tar_xz_bails_pointing_at_063` test flips to asserting
      successful extraction — update it.)
- [ ] Grep/tree gate: `cargo tree -i liblzma-sys` empty; `cargo tree -i lzma-rust2`
      shows only the intended dep.
- [ ] Manual (Linux/macOS): fetch this platform's live release `.tar.xz`, run
      `recall update`, confirm extract + digest-verify succeed end-to-end.

### Open questions to close during implementation

- Exact `XzReader::new` signature (plain `R: Read` vs options/dict-size) — verify
  on docs.rs before wiring.
- Whether `lzma-rust2`'s `xz` feature pulls `sha2` (for XZ SHA-256 stream checks)
  and that it stays pure-Rust (RustCrypto). Confirm via `cargo tree`.
- `lzma-rust2` MSRV vs the project's toolchain floor.
- Fixture generation: can we build a `.tar.xz` fixture in-test with `lzma-rust2`'s
  encoder, or check in a small binary fixture? (Prefer generating to avoid a
  binary blob in the repo.)

Research artifact: `.scratch/research/xz-decoder.md` (re-run 2026-08-31).

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
