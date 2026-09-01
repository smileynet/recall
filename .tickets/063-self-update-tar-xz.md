---
id: "063"
title: "Self-update: extract .tar.xz (Linux/macOS) — needs pure-Rust xz spike"
status: done
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

### Deepening (2026-08-31, research + direct code review)

Dispatched research (tar-extraction safety, self-update security, xz prior art)
and reviewed the code directly (the review subagent batch throttled twice — the
corpus is 2 small files, so per sizing guidance it was reviewed in main context).

**1. Constructor signature resolved.** `lzma-rust2` exposes
`XzReader::new(inner: R, allow_multiple_streams: bool)`. `.xz` can concatenate
independent streams and cargo-dist output may be multi-stream, so pass **`true`**.
This closes the "exact constructor args" open question.

**2. Prior art confirms the pattern.** `tar::Archive::new(XzReader::new(r, true))`
is the exact mirror of the existing `.tar.gz` arm (`GzDecoder` → `tar::Archive`).
cargo-binstall takes the C route (`liblzma` static-vendored + a tar fork) — we
stay C-free with `lzma-rust2`, the genuinely maintained pure-Rust streaming crate.
Footgun to avoid (same as flate2): use the **read**-side reader, not a write-side.

**3. Verify ordering is already correct (update.rs:206-211).** `download_asset` →
`verify_digest` (hard-fails on absent/mismatch) → `extract_binary` → `replace_self`.
The digest is checked on the **compressed bytes before decompression**, so the
`.tar.xz` arm only ever decodes an already-integrity-verified official asset. Keep
this order — do not extract before verifying.

**4. Extraction-safety hardening (the existing arms are thin).** `extract_named_from_zip`
(archive.rs) and `extract_binary_tar_gz` (update.rs) match by **basename/suffix
only** and don't gate entry type or cap output. Because digest-verify precedes
extraction, a malicious archive would have to be a *signed-by-digest official
release* — low risk — but the new arm should still add cheap defense-in-depth that
the tar-safety research flags as standard:
  - Accept only `EntryType::Regular` (reject symlink/hardlink/dir entries — the
    class behind CVE-2026-33056 in tar-rs ≤0.4.44).
  - Read bytes ourselves and return them (we already do — we never `unpack()` to an
    attacker-controlled path, so Zip Slip traversal does not apply to the write).
  - Cap decompressed output with `Read::take(MAX)` (xz can expand ~1000:1; set MAX a
    safe margin above the ~25 MB release binary, e.g. 128 MB) so a decompression
    bomb can't exhaust memory. Do NOT pre-allocate from `entry.size()`.
  - **Pin `tar = ">=0.4.45"`** (current lockfile is 0.4.46 ✓; 0.4.45 fixed the
    symlink CVE). Optional: backport the same `EntryType::Regular` gate to the
    existing `.tar.gz` arm for consistency (small, in scope-adjacent).

**5. Test pattern (mirror archive.rs).** archive.rs builds in-memory fixtures with
`make_zip(entries)` using `zip::ZipWriter`. Do the same for `.tar.xz`: build a tar
with `tar::Builder`, compress with `lzma-rust2`'s XZ encoder into an in-memory
`.tar.xz`, then assert `extract_binary(bytes, "recall-x.tar.xz")` returns the fake
`recall` entry. **Flip the existing `extract_binary_tar_xz_bails_pointing_at_063`
test** (update.rs:553) from asserting a bail to asserting successful extraction.
Add a negative test: an archive whose only entry is a symlink is rejected.

**6. Security caveat to record forward (not this ticket).** self-update-security
research: a SHA-256 digest proves integrity, not provenance — if the same host/MITM
serves the binary it can serve a matching digest. recall verifies digest only (no
publisher signature). This is a pre-existing property, unchanged by 063, but worth a
follow-up ticket (minisign/Sigstore-style signature + version monotonicity) if
supply-chain hardening is wanted. Flag, don't scope-creep 063.

### Change plan

- [ ] Add dep (pinned, C-free, safe build):
      `lzma-rust2 = { version = "=0.20.1", default-features = false, features = ["std", "xz"] }`
      (keep `std` for `XzReader`; omit `optimization` to stay 100% safe Rust).
- [ ] Bump the `tar` floor to `>=0.4.45` (symlink CVE-2026-33056 fix; lockfile
      already at 0.4.46).
- [ ] `extract_binary_tar_xz(archive_bytes, binary_name)` — mirror
      `extract_binary_tar_gz` but: decoder is
      `lzma_rust2::XzReader::new(archive_bytes, true)` (multi-stream) wrapped in
      `Read::take(MAX_DECOMPRESSED)` (~128 MB cap vs the ~25 MB binary); iterate
      `tar::Archive` entries; accept only `EntryType::Regular`; match the binary,
      read bytes into a `Vec` (no `entry.size()` pre-alloc), return.
- [ ] Replace the `.tar.xz` bail arm with a call to the new helper.
- [ ] Unit test: build an in-memory `.tar.xz` fixture (tar::Builder → lzma-rust2 XZ
      encoder) with a fake `recall` entry, assert round-trip. **Flip** the existing
      `extract_binary_tar_xz_bails_pointing_at_063` (update.rs:553) to assert success.
      Add a negative test: a symlink-only archive is rejected (EntryType gate).
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

- [x] `recall update` extracts `.tar.xz` on Linux/macOS (verified against a real
      release asset)
- [x] `cargo tree -i liblzma-sys` empty (no C toolchain dependency)
- [x] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- Unit: `.tar.xz` fixture → recall binary bytes extracted
- `cargo tree -i liblzma-sys` → empty
- Manual on Linux/macOS: real release `.tar.xz` extract + digest verify

## Resolution (2026-09-01)

Added .tar.xz self-update extraction via pure-Rust lzma-rust2 (=0.20.1, no liblzma-sys). extract_binary_tar_xz uses streaming XzReader::new(bytes, true) (multi-stream) into a shared extract_binary_from_tar that gates EntryType::Regular (rejects symlink/hardlink/dir — tar-rs CVE-2026-33056 class) and wraps the decoder in Read::take(128MB) as a decompression-bomb cap; same safety backported to the .tar.gz arm. Bumped tar to 0.4.45. Commit 88b783b.

### Verification
1. ✓ recall update extracts .tar.xz on Linux/macOS — "Unit: extract_binary_tar_xz_extracts_binary round-trips a fake recall entry through an in-memory tar::Builder+XzWriter .tar.xz fixture; extract_binary_tar_xz_rejects_symlink_entry confirms the EntryType gate. Manual e2e: downloaded the REAL v0.1.0 recall-x86_64-unknown-linux-gnu.tar.xz, verify_digest passed against published sha256, extract_binary pulled a 9,458,592-byte ELF binary — proves lzma-rust2 decodes genuine cargo-dist XZ. 109 lib + all integration tests pass; clippy --all-targets clean."
2. ✓ cargo tree -i liblzma-sys empty (no C toolchain) — "cargo tree -i liblzma-sys empty and liblzma empty (dev-only, excluded) — no C toolchain dependency. lzma-rust2 default-features=false, features=[std,xz,encoder]. Release --locked build OK, binary 8.9M (< 25MB ceiling)."
