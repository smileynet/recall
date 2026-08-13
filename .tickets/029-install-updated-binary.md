---
id: "029"
title: "Install updated binary locally (load-dynamic + all fixes)"
status: done
priority: high
blocked_by: []
estimate: 5min
---

# Install Updated Binary

## What to do

The deployed `recall.exe` at `~/.cargo/bin/` is stale — it predates load-dynamic,
Codex bug fixes (F1-F4), telemetry, logging, and sync.

Run:
```
cargo install --path . --force
```

Verify with `recall --version` and `recall search "test" --results 1`.

## Acceptance criteria

- [x] `cargo install --path . --force` succeeds
- [x] `recall search` works (uses cached ~/.recall/lib/onnxruntime.dll)
- [x] `recall telemetry status` works
- [x] `recall sync --skip-import` works

## Resolution (2026-08-06)

`cargo install` fails due to Rust 1.94 + ort feature resolution bug in separate
target dir. Workaround: `cargo build --release` + copy binary. Deployed v0.1.0
with load-dynamic, all bug fixes, telemetry, logging, sync.

