---
id: "068"
title: "embed: graceful ONNX RT init (no panic) + pin ort ~2.x/api-NN + log version in health"
status: open
blocked_by: []
priority: medium
validation_criteria:
  - "Embedder::new returns a domain Err (not a panic) naming expected-vs-found when the ONNX RT DLL is missing/incompatible, pointing at the deploy script"
  - "ort pinned as ~2.x with the lowest api-NN recall actually needs"
  - "recall health logs the loaded ONNX RT version string"
---

# embed: graceful ONNX RT init (no panic) + pin ort ~2.x/api-NN + log version in health

## Context

From research `.scratch/subagent-raw/r2-onnx.md`. Telemetry shows ~63 model-load errors
plus a 2026-08-02 crash: "ort 2.0.0-rc.9 is not compatible ... expected GetVersionString
to return '1.20.x', but got '1.17.1'".

- load-dynamic resolves ONNX RT at runtime; the compat axis is `ORT_API_VERSION` (the
  DLL minor), not the filename. `ort::init_from(path)` returns `Err` — it does NOT panic.
  Any panic is an `.unwrap()` on our side. [r2 L4:verified]
- Pin `ort = "~2.x"` (patch ok, no surprise minor bump that changes bundled ORT) and
  select the LOWEST `api-NN` recall needs (bge inference is basic session+run — likely
  `api-17`/`api-18`), widening the set of acceptable DLLs. [r2 L4]

## What to build

- [ ] Make `Embedder::new()` resolve the explicit `~/.recall/lib/onnxruntime.dll` and
      `init_from(..)?`, converting any `Err` into a domain error that names expected vs
      found version and points at the deploy script that installs the DLL.
- [ ] Pin `ort` to `~2.x` + lowest viable `api-NN` in Cargo.toml (verify against recall's
      ort call sites; keep Cargo.lock working — ticket 049 context).
- [ ] Log `GetVersionString()` in `recall health` for diagnosability.

## Acceptance criteria

- [ ] No panic on missing/incompatible ONNX RT DLL — graceful domain error with remediation
- [ ] `ort` pinned `~2.x` + lowest api-NN
- [ ] `recall health` shows the loaded ONNX RT version

## Notes / relations

- Relates to 049 (ort/cargo-install breakage) and **064 (ORT download SHA-256)** — 064 is
  likely the actual cause of the 50 `Load model from D` failures (weak >1MB heuristic lets
  a corrupt DLL through, poisoning every model-load). Do 064 first.
- r2 Open-Qs: confirm ort surfaces the `GetApi(N)==nullptr` case as a typed error; find
  recall's actual lowest api-NN by scanning ort call sites.
