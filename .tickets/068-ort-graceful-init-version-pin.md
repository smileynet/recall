---
id: "068"
title: "embed: ONNX RT robustness — fix cargo-install resolve, graceful init, version pin + log (folds 049)"
status: open
blocked_by: ["064"]
priority: high
validation_criteria:
  - "cargo install --path . succeeds on current stable Rust from a clean resolve (no lockfile)"
  - "Embedder::new / ensure_ort_runtime returns a domain Err (not a panic) naming expected-vs-found ONNX RT version when the DLL is missing/incompatible, pointing at the deploy script"
  - "ort/ort-sys pinned so a fresh resolve cannot drift off the fastembed-dictated version"
  - "recall health logs the loaded ONNX RT version string"
---

# embed: ONNX RT robustness — resolve, graceful init, version pin + log

Folds in **ticket 049** (cargo-install breakage). Both tickets touch the same
`ort`/`ort-sys`/ONNX-RT surface; splitting them re-litigates the version decision
twice. 049 is now closed as superseded-by-068.

## Context

From research `.scratch/subagent-raw/r2-onnx.md` and ticket 049 investigation
(2026-10-02, `.scratch/research/ort-fastembed-compat.md`). Telemetry shows ~63
model-load errors plus a 2026-08-02 crash: "ort 2.0.0-rc.9 is not compatible ...
expected GetVersionString to return '1.20.x', but got '1.17.1'".

### Version reality (verified from primary sources — corrects earlier assumptions)

`ort` is **not independently version-pinnable** by recall. `fastembed 4.9.1`
exact-pins BOTH `ort = "=2.0.0-rc.9"` AND `ort-sys = "=2.0.0-rc.9"` (confirmed in
its registry `Cargo.toml`). So:

- The old 049 instruction "upgrade ort to rc.13" is impossible without bumping
  fastembed (4.9.1→rc.9; 5.x→rc.10; 6.x/7.x→rc.13). ort version is a function of
  the fastembed version.
- The old 068 instruction "pin ort `~2.x` + lowest `api-NN`" is also not
  achievable on the current stack: the `api-NN` cargo features only exist on ort
  **rc.12/rc.13**, not rc.9. On rc.9 the ONNX RT requirement is fixed at 1.20.x
  (hardcoded `ONNXRUNTIME_VERSION="1.20.0"` in ort-sys/build.rs, mirrored by
  recall's `ORT_VERSION` constant in `src/embed.rs`).

### The actual cargo-install break (folded from 049, reproduced 2026-10-02)

A fresh resolve (deleted `Cargo.lock`) pulls `ort 2.0.0-rc.9` + **`ort-sys
2.0.0-rc.10`** — a mismatch. The `=2.0.0-rc.9` exact pin (on both recall and
fastembed) covers `ort` but NOT `ort-sys`; `ort` rc.9 depends on `ort-sys` with a
loose range that admits rc.10. rc.10 is where the ABI break landed (`Option<fn>`
→ bare `fn` on the raw OrtApi struct, verified from source), so ort rc.9's
`.unwrap_or_else()` call sites fail to compile → 50+ type errors. The committed
`Cargo.lock` pins `ort-sys` to rc.9, which is why `cargo build --release` (locked)
works but `cargo install` / lockfile-free CI breaks.

### Init behavior

- load-dynamic resolves ONNX RT at runtime; the compat axis is `ORT_API_VERSION`
  (the DLL minor), not the filename. `ort::init_from(path).commit()` returns
  `Err` — it does NOT panic. recall's `ensure_ort_runtime_inner` already uses `?`
  on it, so the panic risk is a weak corrupt-DLL heuristic (the `< 1MB` size
  check in `src/embed.rs`), which **064** replaces with a SHA-256 verify. [r2 L4]

## What to build

- [ ] **Fix the resolve (folds 049).** Add a direct `ort-sys = "=2.0.0-rc.9"`
      dependency in `Cargo.toml` (matching fastembed's pin) so a fresh resolve
      cannot drift `ort-sys` to rc.10. Keep recall's existing direct
      `ort = "=2.0.0-rc.9"`. Verify `cargo install --path .` succeeds from a
      clean resolve, and `cargo build --release` still works.
- [ ] **Graceful init.** Audit `ensure_ort_runtime` / `Embedder::new` for any
      `.unwrap()`/`.expect()` on the ONNX RT path; ensure a missing/incompatible
      DLL surfaces a domain `Err` that names expected (`ORT_VERSION`, 1.20.x) vs
      found (from `GetVersionString`) and points at the deploy script that
      installs the DLL. The `init_from(..).commit()?` already propagates — the
      work is the error *message* quality and killing any remaining unwrap.
- [ ] **Log the version.** `recall health` logs the loaded ONNX RT version string
      (`GetVersionString()`), and the expected `ORT_VERSION`, for diagnosability.
- [ ] **Document the fastembed↔ort↔ONNX-RT coupling** in `.memory/` (ADR) so the
      next upgrade starts from the matrix, not a re-discovery.

## Deferred to a separate ticket (do NOT do here)

Bumping fastembed (4.9.1→7.x) to move onto ort rc.13 + `api-NN` features + newer
ONNX RT (1.23.x). That is a larger change: it moves the required native DLL
(1.20→1.23), collides with recall's hardcoded `ORT_VERSION` and the deploy-script
download URL, and needs embedding-output parity validation. Create a new ticket
for it once this stabilizes the current stack. (The `~2.x` + `api-NN` pin from the
original 068 belongs to that ticket, not this one.)

## Acceptance criteria

- [ ] `cargo install --path .` succeeds on current stable Rust from a clean
      resolve (no `Cargo.lock`); `cargo build --release` still produces a working
      binary (folds 049 AC)
- [ ] A fresh `cargo generate-lockfile` resolves `ort-sys` to rc.9, not rc.10
- [ ] No panic on missing/incompatible ONNX RT DLL — graceful domain error with
      expected-vs-found version and remediation pointing at the deploy script
- [ ] `recall health` shows the loaded ONNX RT version
- [ ] All tests pass; golden-query embedding-parity suite unchanged (folds 049 AC)
- [ ] ADR documents the fastembed/ort/ort-sys/ONNX-RT version coupling

## Notes / relations

- **blocked_by 064** (ORT download SHA-256): 064 replaces the weak `<1MB`
  corrupt-DLL heuristic that likely causes the 50 "Load model from D" failures —
  a corrupt DLL poisons every model-load. 068's graceful-init work builds on a
  trustworthy DLL, so 064 lands first. (068 itself said "do 064 first.")
- Supersedes/folds **049** (ort/cargo-install breakage) — same version surface.
- r2 Open-Q resolved: on rc.9 there are no `api-NN` features to minimize; that
  lever only appears after the fastembed uplift (deferred ticket).
