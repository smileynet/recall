---
id: "076"
title: "fix: tar.xz extract test fails on Windows - searches recall.exe but .tar.xz holds recall"
status: open
blocked_by: []
priority: medium
---

# fix: tar.xz extract test fails on Windows (binary-name/suffix mismatch)

## Context

Found during the 2026-10-09 ticket-review pass (`cargo test --lib` at head 643251c):

```
test update::tests::extract_binary_tar_xz_extracts_binary ... FAILED
thread panicked at src\update.rs:614:86:
called `Result::unwrap()` on an `Err` value: binary 'recall.exe' not found in archive
```

115 pass, this 1 fails — **Windows-only**. The feature itself (#063 pure-Rust xz
extraction) works; this is a test/platform mismatch.

## Root cause

`extract_binary` (`src/update.rs:384-388`) chooses the binary name by platform:

```rust
let binary_name = if cfg!(windows) { "recall.exe" } else { "recall" };
```

The test `extract_binary_tar_xz_extracts_binary` (`src/update.rs:610-617`) hardcodes a
tar entry named `recall` (no suffix), then calls `extract_binary(.., ".tar.xz")`. On
Windows, `extract_binary` searches for `recall.exe`, which is not in the archive -> the
`.unwrap()` at line 614 panics.

The content is correct for the *format*: a `.tar.xz` is only ever a Linux/macOS release
asset (cargo-dist produces `.zip` for Windows), so its payload is always `recall`, never
`recall.exe`. The defect is that `extract_binary` applies the host-platform suffix
(`cfg!(windows)`) to a decision that should follow the **asset format**, not the host.

## Fix options

- **A (preferred): derive the expected name from the asset, not the host.** For a
  `.tar.xz`/`.tar.gz` asset always look for `recall`; for `.zip` look for `recall.exe`.
  This is also more correct in principle — the asset's platform is encoded in its name,
  and a self-update only ever downloads the asset matching the running platform anyway,
  but the extraction logic shouldn't assume host == asset.
- **B (minimal): make the test platform-aware.** Build the test archive entry with the
  same `cfg!(windows)` suffix the code expects. Fixes the red test but leaves the
  host/asset coupling in place.

Prefer A — it removes a latent cross-platform footgun; B only silences the test.

## Acceptance criteria

- [ ] `cargo test --lib` passes on Windows (and Linux/macOS) — no platform-skipped test
- [ ] `.tar.xz` / `.tar.gz` extraction targets `recall`; `.zip` targets `recall.exe`,
      regardless of host OS (if Option A)
- [ ] A regression test asserts the asset-format -> binary-name mapping directly

## Notes

- Introduced with #063 (tar.xz self-update). Pure test/logic fix; no new dependency.
- Does not block anything; raises CI reliability on Windows runners.
