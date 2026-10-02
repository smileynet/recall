---
id: "068"
title: "embed: ONNX RT robustness — fix cargo-install resolve, graceful init, version pin + log (folds 049)"
status: done
blocked_by: ["064"]
priority: high
validation_criteria:
  - "cargo install --path . succeeds on current stable Rust from a clean resolve (no lockfile)"
  - "A libloading pre-flight returns a domain Err (not a panic) naming expected-vs-found ONNX RT version when the DLL is missing/incompatible, with correct remediation (re-run or delete ~/.recall/lib/, NOT the deploy script)"
  - "ort-sys pinned =2.0.0-rc.9 so a fresh resolve cannot drift to rc.10"
  - "recall health logs the loaded ONNX RT version string (or 'not loaded')"
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

### The actual cargo-install break (folded from 049, reproduced + root-caused 2026-10-02)

A fresh resolve (deleted `Cargo.lock`) pulls `ort 2.0.0-rc.9` + **`ort-sys
2.0.0-rc.10`** — a mismatch. **Root cause (verified from `ort` rc.9's registry
`Cargo.toml`):** `ort` rc.9 requires `ort-sys = "2.0.0-rc.9"` — a *loose*
requirement with NO `=`. Cargo's prerelease rule auto-upgrades a loose
same-release prerelease range to newer prereleases, so `2.0.0-rc.9` admits
`rc.10`. rc.10 carries the ABI break (`Option<fn>` → bare `fn` on the raw OrtApi
struct), so ort rc.9's `.unwrap_or_else()` call sites fail to compile → 50+ type
errors. recall's and fastembed's `=2.0.0-rc.9` exact pins cover `ort` but not
`ort-sys`. The committed `Cargo.lock` pins `ort-sys` to rc.9, which is why
`cargo build --release` (locked) works but `cargo install` / lockfile-free CI
breaks.

**Fix (verified correct):** a direct `ort-sys = "=2.0.0-rc.9"` in `[dependencies]`
intersects ort's loose range down to the single point rc.9. A direct dep
constrains the transitive resolve on a cold resolve (not just via the lockfile);
`=` reliably excludes rc.10 (the auto-upgrade rule applies only to loose ranges,
not `=` points); no "unused crate" warning on stable (that lint is nightly-only);
and it cannot conflict with fastembed's own `=rc.9` (all requirements intersect
to rc.9, and a future incompatible bump would fail loudly, not mispick).

### Init behavior (CORRECTED — the r2 assumption was wrong)

The r2 note said "`init_from` returns Err, any panic is our `.unwrap()`." Verified
against ort rc.9 source — **this is wrong**:

- `init_from(path)` only stashes the path in a `OnceLock`. The dylib is dlopen'd
  **lazily**, inside `ort::api()`, which first runs during `.commit()`
  (`src/embed.rs:158`). So a missing/garbage/wrong-version DLL does NOT surface at
  `init_from`; it hits `.commit()` or first use.
- The load/symbol/version checks live inside ort's `OnceLock` closure as
  `.expect("OrtGetApiBase must be present...")` / `.expect("GetVersionString...")`
  / `panic!` — **panics, not `Err`s**, and ort exposes no fallible equivalent.
  recall cannot turn them into a domain error through normal `?` handling. (This
  is the exact panic the 064 stub reproduced, during `recall add`.)
- recall's own ORT path has only 3 unwraps (`embed.rs:208` parent — infallible;
  `:479` next — post-load) — none is the bad-DLL crash. The crash is inside ort.
- Current size guards catch too-SMALL files only; a >1MB wrong-version/arch DLL
  passes the guard and panics inside `.commit()`.

**Therefore graceful init requires a libloading PRE-FLIGHT** (not Err-handling):
before `ort::init_from`, dlopen the cached lib, check the `OrtGetApiBase` symbol
exists, call `GetVersionString()`, compare the minor to the expected 20 (1.20.x),
and return a domain `Err` with remediation on any failure. If Ok, ort's
subsequent load takes the same successful path. (A `catch_unwind` backstop is
possible but secondary, and only works if the release profile isn't
`panic="abort"` — confirm that; the panic hook also prints to stderr first.)

## What to build

- [ ] **Fix the resolve (folds 049).** Add `ort-sys = "=2.0.0-rc.9"` to
      `[dependencies]` in `Cargo.toml` (there is currently NO ort-sys line; `ort`
      is at `Cargo.toml:12`). Keep the existing direct `ort = "=2.0.0-rc.9"`.
      Verify `cargo generate-lockfile` resolves `ort-sys` to rc.9 (not rc.10),
      `cargo install --path .` succeeds from a clean resolve, and
      `cargo build --release` still works.
- [ ] **Graceful init via pre-flight** (NOT Err-handling — see Init behavior).
      Add `preflight_ort_dylib(path)` in `src/embed.rs`, called in
      `ensure_ort_runtime_inner` just before `ort::init_from` (`embed.rs:158`):
      dlopen via `libloading` → check `OrtGetApiBase` symbol → call
      `GetVersionString()` → parse minor, compare to expected 20 → domain `Err`
      on any failure, naming expected (`ORT_VERSION` 1.20.x) vs found and pointing
      at remediation. Make `ORT_VERSION` (`embed.rs:10`) readable (pub or
      accessor). Confirm the release profile isn't `panic="abort"` if adding a
      `catch_unwind` backstop.
- [ ] **Log the version.** Add `ort_version: Option<String>` to `HealthReport`
      (`cli.rs:654-668`; `#[derive(Serialize)]` makes JSON automatic). Populate in
      `build_health_report` (`cli.rs:670`) — reuse the pre-flight's
      `GetVersionString` read so health does NOT need to construct an `Embedder`
      (it never loads ORT today). Print a text line in `cmd_health` between the
      Last-ingest and Last-log lines (~`cli.rs:637-642`). Report `None` as "not
      loaded / not verified". Watch `tests/cli_contract.rs` + the health `--json`
      snapshot.
- [ ] **ADR 0003** (`.memory/adr/0003-ort-fastembed-coupling.md`) documenting the
      fastembed↔ort↔ort-sys↔ONNX-RT coupling + the loose-prerelease-range pin
      hazard, mirroring the 0001/0002 format (plain `#` header + bullet metadata,
      no frontmatter; include `## Alternatives considered` for the pin mechanism).
- [ ] **Reconcile the install-path docs.** When the resolve fix lands, update
      `AGENTS.md:106` ("cargo install broken, #049") — it will be fixed. Related
      stale mentions: ticket 050:17 ("broken indefinitely").

### Remediation target (CORRECTED — the deploy scripts do NOT install the DLL)

The deploy scripts (`scripts/deploy-local.{sh,ps1}`) only build+copy the `recall`
binary; the ONNX RT DLL is auto-downloaded in-binary on first run
(`ensure_ort_runtime` → `download_ort_runtime`), cached at `~/.recall/lib/`. So
the graceful-init error must NOT say "run the deploy script to install the DLL"
(no such step exists). Correct remediation to surface: re-run the command (the
download self-heals a corrupt cache), or delete `~/.recall/lib/` to force a clean
re-download. The AC wording is updated accordingly below.

## Deferred to a separate ticket (do NOT do here)

Bumping fastembed (4.9.1→7.x) to move onto ort rc.13 + `api-NN` features + newer
ONNX RT (1.23.x). That is a larger change: it moves the required native DLL
(1.20→1.23), collides with recall's hardcoded `ORT_VERSION` and the deploy-script
download URL, and needs embedding-output parity validation. Create a new ticket
for it once this stabilizes the current stack. (The `~2.x` + `api-NN` pin from the
original 068 belongs to that ticket, not this one.)

## Acceptance criteria

- [x] `cargo install --path .` succeeds on current stable Rust from a clean
      resolve (no `Cargo.lock`); `cargo build --release` still produces a working
      binary (folds 049 AC)
- [x] A fresh `cargo generate-lockfile` resolves `ort-sys` to rc.9, not rc.10
- [x] No panic on missing/incompatible/wrong-version ONNX RT DLL — a libloading
      pre-flight returns a graceful domain error (expected-vs-found version) with
      remediation: re-run the command, or delete `~/.recall/lib/` to force a clean
      re-download (NOT "run the deploy script" — it does not install the DLL)
- [x] `recall health` shows the loaded ONNX RT version (or "not loaded")
- [x] All tests pass; golden-query embedding-parity suite unchanged (folds 049 AC)
- [x] ADR documents the fastembed/ort/ort-sys/ONNX-RT version coupling

## Notes / relations

- **064 DONE (2026-10-02) — likely already resolves the "~50 Load model"
  failures.** 064 replaced the weak `<1MB` heuristic with SHA-256 archive
  verification AND root-caused the actual poisoning: the extractor was selecting
  `libonnxruntime_providers_shared.so` (14KB sidecar) instead of the real lib,
  caching a stub with no `OrtGetApiBase`. That stub is exactly what produces
  model-load failures. So 068 should FIRST re-check telemetry — the load-error
  rate may have already dropped. 068's remaining value is the graceful *error
  message* (vs the current `OrtGetApiBase must be present` panic from ort) and
  version logging, not the corrupt-DLL cause itself.
- Supersedes/folds **049** (ort/cargo-install breakage) — same version surface.
- r2 Open-Q resolved: on rc.9 there are no `api-NN` features to minimize; that
  lever only appears after the fastembed uplift (deferred ticket 075).

## Resolution (2026-10-02)

Folded 049. (1) Pinned ort-sys =2.0.0-rc.9 directly (root cause: ort rc.9's loose ort-sys range auto-upgraded to the ABI-incompatible rc.10 on fresh resolve) - unbreaks cargo install. (2) Added libloading pre-flight (read_ort_dylib_version + preflight_ort_dylib) before ort::init_from: dlopen, check OrtGetApiBase + GetVersionString minor vs ORT_VERSION, domain Err with remediation on failure - ort otherwise panics internally (lazy, at commit()), uncatchable via ?. (3) recall health reports loaded ONNX RT version (shared reader, never triggers ort panic). (4) ADR 0003 documents the coupling; reconciled the stale AGENTS.md cargo-install-broken note. 064's sidecar fix likely already dropped the ~50 Load-model failures; this adds graceful degradation + diagnosability on top.

### Verification
1. ✓ cargo install --path . succeeds on current stable Rust from a clean resolve (no lockfile) — "cargo install clean-resolve: deleting Cargo.lock + cargo generate-lockfile resolves ort-sys to rc.9 (was rc.10); fresh graph builds; cargo build --release unaffected. 116 lib tests + all integration suites green."
2. ✓ A libloading pre-flight returns a domain Err (not a panic) naming expected-vs-found ONNX RT version when the DLL is missing/incompatible, with correct remediation (re-run or delete ~/.recall/lib/, NOT the deploy script) — "Pre-flight: preflight_rejects_non_dylib_file proves a >1MB non-dylib returns a domain Err (no panic) with remediation (re-run / delete ~/.recall/lib/, not the deploy script); preflight_passes_on_cached_real_dylib proves the real lib passes; stub-file run self-heals via the cache check before pre-flight. No panic='abort' in release profile."
3. ✓ ort-sys pinned =2.0.0-rc.9 so a fresh resolve cannot drift to rc.10 — "ort-sys pinned =2.0.0-rc.9 in Cargo.toml; fresh resolve verified to pick rc.9."
4. ✓ recall health logs the loaded ONNX RT version string (or 'not loaded') — "recall health shows 'ONNX Runtime: 1.20.0 (expected 1.20.0)' in text and ort_version/ort_version_expected in --json (verified live); snapshot updated + portable; reports 'not loaded' when absent."
