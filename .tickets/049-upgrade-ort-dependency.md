---
id: "049"
title: "Upgrade ort dependency — cargo install broken on newer Rust toolchains"
status: done
blocked_by: []
---

# Upgrade ort dependency — cargo install broken on newer Rust toolchains

## Problem

`cargo install --path .` fails on Rust 1.87+ because the pinned `ort` v2.0.0-rc.9
is incompatible with the newer `ort-sys` crate it resolves. The `ortsys!` macro
calls `.unwrap_or_else()` on raw function pointers — this worked when `ort-sys`
defined those fields as `Option<fn(...)>`, but newer `ort-sys` versions changed them
to bare `fn(...)` pointers (no `Option` wrapper), breaking the call sites.

The locked `cargo build --release` still works (Cargo.lock pins compatible versions),
but fresh resolves (cargo install, CI without lockfile) fail with 50+ type errors in
the `ort` crate.

## Impact

- `cargo install --path .` is broken (the documented install path)
- CI builds without lockfile will fail
- Current workaround: `cargo build --release` + copy binary manually

## What to build

Upgrade `ort` from 2.0.0-rc.9 to 2.0.0-rc.13 (or latest compatible release).
Verify the `fastembed` dependency chain pulls compatible versions. May require
upgrading `fastembed` as well since it pins the `ort` version.

## Acceptance criteria

- [ ] `cargo install --path .` succeeds on current stable Rust
- [ ] `cargo build --release` still produces a working binary
- [ ] All 81 tests pass after upgrade
- [ ] Embedding output is unchanged (golden query test passes)
- [ ] Search quality unaffected (golden_queries.rs regression suite)
- [ ] Update Cargo.lock with new versions
- [ ] Verify model loading still works (BGE-base-en-v1.5)

## Resolution (2026-10-02)

Superseded by / folded into ticket 068. Investigation (2026-10-02) found the ticket's prescribed fix (upgrade ort rc.9->rc.13) is impossible without bumping fastembed: fastembed 4.9.1 exact-pins ort AND ort-sys to =2.0.0-rc.9 (verified in its registry Cargo.toml). The real cargo-install break is a fresh resolve pulling ort-sys rc.10 (ABI break: Option<fn>->bare fn) against ort rc.9 — the =rc.9 exact pin covers ort but not ort-sys. Reproduced by deleting Cargo.lock + cargo generate-lockfile -> ort-sys 2.0.0-rc.10. Fix (pin ort-sys =2.0.0-rc.9) now lives in 068 alongside the ONNX-RT graceful-init/version work on the same dependency surface. The larger fastembed 7.x / ort rc.13 / api-NN uplift is deferred to a new ticket. Research: .scratch/research/ort-fastembed-compat.md.
