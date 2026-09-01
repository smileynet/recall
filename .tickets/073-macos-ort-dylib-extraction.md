---
id: "073"
title: "embed: extract macOS infix-versioned ONNX RT dylib from tgz"
status: done
blocked_by: []
priority: high
validation_criteria:
  - "ort_lib_entry_matches accepts libonnxruntime.<ver>.dylib and rejects the zero-size symlink and .dSYM debug copy"
  - "recall ingest initializes ONNX RT on Darwin arm64 (no 'Could not find libonnxruntime.dylib' error)"
---

# embed: extract macOS infix-versioned ONNX RT dylib from tgz

## Context

Discovered 2026-08-31 during the first real ingest on macOS (Darwin arm64).
`recall ingest` failed at ORT init with:

```
recall: ONNX Runtime initialization failed: Could not find libonnxruntime.dylib in the downloaded archive
```

The ORT tgz downloaded fine (7.5MB), but `extract_lib_from_tgz` could not
locate the library inside it.

## Root cause

The tgz matcher used `filename == lib_name || filename.starts_with(lib_name)`
with `lib_name = "libonnxruntime.dylib"`. That assumes Linux-style SUFFIX
versioning (`libonnxruntime.so.1.20.0`, which starts with `libonnxruntime.so`).

The macOS `onnxruntime-osx-arm64-1.20.0.tgz` layout is different:
- `lib/libonnxruntime.1.20.0.dylib` — the real 25.4MB library (version INFIXED
  before `.dylib`, so it does NOT start with `libonnxruntime.dylib`)
- `lib/libonnxruntime.dylib` — a zero-size symlink to the versioned file
  (matches `lib_name` exactly, but `entry_size == 0` guard correctly skips it)
- `lib/*.dSYM/.../DWARF/libonnxruntime.1.20.0.dylib` — a 9MB debug-symbol copy
  with the same filename (must NOT be extracted)

Both real candidates were rejected → bail.

## What to build

- [x] Add `ort_lib_entry_matches(path, filename, lib_name, entry_size)`: accepts
      exact-name, suffix-versioned (Linux), and infix-versioned (macOS) forms;
      requires `entry_size > 0` (skips symlinks); rejects any path containing
      `.dSYM`.
- [x] Route `extract_lib_from_tgz` through the predicate.
- [x] Unit tests for the real Linux/macOS/Windows archive layouts.

## Acceptance criteria

- [x] `ort_lib_entry_matches` accepts `libonnxruntime.<ver>.dylib`, rejects the
      zero-size symlink and the `.dSYM` debug copy
- [x] `recall ingest` initializes ONNX RT on Darwin arm64 with no "Could not
      find" error (verified: 24M dylib extracted to `~/.recall/lib/`)
- [x] `cargo test --lib embed` green; `cargo clippy --lib` clean

## Relations

- Sibling to 063 (tar.xz extraction), 064 (ORT download SHA-256), 068 (graceful
  ORT init). This is the "locate the right entry" defect; those cover format,
  integrity, and init-error handling respectively.

## Resolution (2026-09-01)

Added ort_lib_entry_matches() handling exact/suffix(Linux)/infix(macOS) dylib naming, requiring entry data (skips zero-size symlink) and rejecting .dSYM debug copies. Routed extract_lib_from_tgz through it. 4 new unit tests for real archive layouts. Root cause: matcher assumed Linux suffix-versioning; macOS infixes version before .dylib. Fix committed in 0c8f3bf (rebased onto origin/main).

### Verification
1. ✓ ort_lib_entry_matches accepts libonnxruntime.<ver>.dylib and rejects the zero-size symlink and .dSYM debug copy — "cargo test --lib embed → 12 passed incl ort_lib_entry_matches_macos_versioned_dylib, ort_lib_entry_matches_rejects_macos_symlink_and_dsym, ort_lib_entry_matches_linux_suffix_versioned_so, ort_lib_entry_matches_exact_name; clippy --lib clean"
2. ✓ recall ingest initializes ONNX RT on Darwin arm64 (no 'Could not find libonnxruntime.dylib' error) — "After deploying fix, recall ingest extracted the real 24M libonnxruntime.dylib to ~/.recall/lib/ (was empty/failing before); ingest now running at 575% CPU with DB grown 64KB→3.6MB embedding chunks — no 'Could not find' error"
