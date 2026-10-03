# Native Dependency Pinning — the ort/ONNX saga

The deepest lesson of the project, spanning tickets 049 → 064 → 068 → 075. If you depend
on a Rust crate that wraps a native library through a `-sys` crate, read this before you
pin or upgrade anything.

## The dependency chain

`fastembed` → `ort` (pyke ONNX Runtime bindings) → `ort-sys` (raw FFI) → **native ONNX
Runtime shared library** (loaded at runtime via `load-dynamic`).

These four layers are version-locked:
- `fastembed` **exact-pins** both `ort` and `ort-sys` (`=2.0.0-rc.X`). So **`ort` is not
  independently upgradable** — its version is a function of the `fastembed` version.
  (4.9.1→rc.9/ONNX RT 1.20; 7.1.0→rc.13/ONNX RT 1.28.)
- The compiled `ORT_API_VERSION` equals the ONNX RT minor. The C API is additive, so a
  **newer** runtime satisfies an older build (API M ≥ N).

## The break (ticket 049→068)

`cargo install` failed on fresh toolchains with 50+ type errors, while
`cargo build --release` worked fine. Root cause:
- `ort` rc.9 required `ort-sys` with a **loose** range (`"2.0.0-rc.9"`, no `=`).
- **Cargo's prerelease rule auto-upgrades a loose same-release prerelease range to newer
  prereleases** — so a fresh resolve pulled `ort-sys` **rc.10**.
- rc.10 was an **ABI break** (`Option<fn>` → bare `fn` on the raw `OrtApi` struct) that
  fails to compile against `ort` rc.9.
- The committed `Cargo.lock` pinned `ort-sys` to rc.9 — which is exactly why the daily
  dev loop (lockfile-respecting) never saw it, but `cargo install` / lockfile-free CI
  (fresh resolve) did.

**Fix:** a direct `ort-sys = "=2.0.0-rc.9"` in recall's own `Cargo.toml`. A direct
dependency constrains the transitive resolve on a *cold* resolve (not just via the
lockfile); `=` excludes the auto-upgrade; it's a no-op `use`-free dep on stable (no unused
warning).

## The DLL integrity + extraction trap (ticket 064)

The runtime DLL is downloaded on first run. The guard was a weak `>1MB` size check.
Replacing it with a **pinned per-platform SHA-256, verified before extraction** exposed a
latent bug: the archive ships `libonnxruntime_providers_shared.so` (a ~14KB sidecar)
*before* the real multi-MB lib, and the extractor matched the sidecar — caching a stub
with no `OrtGetApiBase` that poisons every later load. **The size check had been masking
it.** Fix: reject `libonnxruntime_*` sidecars in the matcher + keep a post-extract size
assertion as defense-in-depth.

## Graceful init (ticket 068)

`ort::init_from(path).commit()` loads the dylib **lazily** and `.expect()`s inside the
crate on a bad lib — a **panic, not an `Err`**, that recall can't `?`-handle. The fix is a
**libloading pre-flight** (dlopen → `OrtGetApiBase` → `GetVersionString` → compare minor)
that returns a domain error *before* `ort` ever touches the lib. `recall health` now
reports the loaded version.

## The uplift (ticket 075)

Moving to `fastembed` 7.1.0 / `ort` rc.13 / ONNX RT 1.28.2. API breaks encountered (several
not in the upfront research — found at build time):
- `ort-sys` rc.13: `OrtApiBase.GetVersionString` is now a **bare `fn`** (was `Option<fn>`)
  — the pre-flight calls it directly.
- `ort::init_from` returns `Result<EnvironmentBuilder, _>`; `commit()` returns `bool`.
- `fastembed` 7's `TextEmbedding::embed` takes **`&mut self`** → wrap the model in a
  `Mutex`.
- `InitOptions` → `TextInitOptions` (deprecated alias).
- A **feature-unification regression**: the `ort`/`fastembed` bump dropped whatever
  transitively enabled `ureq`'s `json` feature → had to enable it explicitly.
- **ONNX RT 1.24 was never released** (Microsoft's series skips 1.23→1.25) — a plausible
  pin target that is a dead 404.

**Embedding parity verified → no corpus re-embed:** 1.20 vs 1.28 on BGE-base measured
`max_abs = 1.9e-7`, top-1 agreement 1.000, top-3 Jaccard 1.000 (`src/bin/parity_dump.rs`).

## Portable rules

1. **A committed `Cargo.lock` does NOT protect `cargo install` or library consumers** —
   they do a fresh resolve. Test that path in CI.
2. **Pin transitively-coupled `-sys` crates with a direct `=` dep** when the parent uses a
   loose prerelease range. `[patch]` is the wrong tool (replaces source, not version);
   `cargo update --precise` is lockfile-only.
3. **Drive every derived artifact (URL, hash, version check) from one constant.**
4. **Don't trust a native lib's error path** — pre-validate the artifact; expose the
   loaded version in diagnostics.
5. **Verify an archive by hash before extraction; never trust a size heuristic** — and
   watch for sidecar files that sort before the real one.
6. **A runtime point-bump (same model weights) does NOT require re-embedding** — ONNX RT
   CPU inference isn't bit-exact, but ~1e-6 drift doesn't move cosine rankings. Measure
   with a parity harness; re-embed only on a *weights* change.
7. **Record the version-coupling matrix in an ADR** — it turns the next upgrade from
   archaeology into a checklist.
